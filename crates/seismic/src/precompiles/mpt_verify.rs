//! MPT (Merkle Patricia Trie) proof verification precompile.
//!
//! Verifies an MPT inclusion/exclusion proof for one or more key-value pairs
//! against a given root. Each key carries its own proof (root-to-leaf trie
//! nodes), matching the per-key proof format of Ethereum's `eth_getProof`
//! endpoint. Verification uses `alloy_trie::proof::verify_proof`.

use alloy_primitives::{Bytes, B256};
use nybbles::Nibbles;
use revm::precompile::{
    u64_to_address, Precompile, PrecompileError, PrecompileId, PrecompileOutput, PrecompileResult,
};

/// Address of the MPT verify precompile.
pub const MPT_VERIFY_ADDRESS: u64 = 106;

/// Returns the mpt_verify precompile with its address.
pub fn precompiles() -> impl Iterator<Item = Precompile> {
    [MPT_VERIFY].into_iter()
}

/// The MPT verify precompile.
pub const MPT_VERIFY: Precompile = Precompile::new(
    PrecompileId::Custom(std::borrow::Cow::Borrowed("MPT_VERIFY")),
    u64_to_address(MPT_VERIFY_ADDRESS),
    mpt_verify,
);

/// Minimum input length: 32 (root) + 4 (item_count) = 36 bytes.
const MIN_INPUT_LENGTH: usize = 36;

/* --------------------------------------------------------------------------
 Cost Model
-------------------------------------------------------------------------- */

/// Base gas cost, comparable to ecrecover (~3000 gas).
const BASE_COST: u64 = 3000;

/// Per item gas cost (key lookup verification per item).
const PER_ITEM_COST: u64 = 200;

/// Per proof node gas cost (keccak256 hash + RLP decode per node).
const PER_PROOF_NODE_COST: u64 = 500;

/* --------------------------------------------------------------------------
 Precompile Logic
-------------------------------------------------------------------------- */

/// # MPT Verify
///
/// Verifies an MPT proof for one or more key-value pairs against a root.
/// Each key carries its own per-key proof (root-to-leaf trie nodes),
/// compatible with Ethereum's `eth_getProof` format.
///
/// ## Input layout
///
/// ```text
/// [0..32]             : root — 32-byte Merkle root
/// [32..36]            : item_count — u32 big-endian, number of items
/// For each item:
///   [0..32]           : key — 32-byte keccak256-hashed trie key
///   [0..1]            : has_value — 0x01 for inclusion, 0x00 for exclusion
///   If has_value == 0x01:
///     [0..4]          : value_len — u32 big-endian
///     [4..4+L]        : value bytes
///   [next 4 bytes]    : proof_node_count — u32 big-endian for this key
///   For each proof node:
///     [0..4]          : node_len — u32 big-endian
///     [4..4+L]        : node bytes
/// ```
///
/// ## Output layout
///
/// ```text
/// [0..32] : 0x01 left-padded to 32 bytes (success, for Solidity abi.decode)
/// ```
///
/// ## Errors
///
/// Returns `PrecompileError` if:
/// - Input is too short or malformed
/// - Proof nodes are truncated
/// - Proof verification fails (invalid proof, wrong root, or value mismatch)
/// - Gas limit exceeded
pub fn mpt_verify(input: &[u8], gas_limit: u64) -> PrecompileResult {
    if input.len() < MIN_INPUT_LENGTH {
        return Err(PrecompileError::Other(format!(
            "input too short: expected at least {} bytes, got {}",
            MIN_INPUT_LENGTH,
            input.len()
        )));
    }

    // Parse root
    let root: [u8; 32] = input
        .get(0..32)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| PrecompileError::Other("input too short for root".into()))?;
    let root_b256 = B256::from(root);

    // Parse item count
    let item_count = u32::from_be_bytes(
        input
            .get(32..36)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| PrecompileError::Other("input too short for item_count".into()))?,
    );
    if item_count == 0 {
        return Err(PrecompileError::Other("item_count must be > 0".into()));
    }

    // Parse items with per-key proofs and calculate total proof nodes for gas
    let mut offset = 36usize;
    let mut total_proof_nodes: u64 = 0;

    // Collect parsed items: (key_nibbles, expected_value, proof_nodes)
    let mut items: Vec<(Nibbles, Option<Vec<u8>>, Vec<Bytes>)> =
        Vec::with_capacity(item_count as usize);

    for _ in 0..item_count {
        // Read key (32 bytes)
        if offset + 33 > input.len() {
            return Err(PrecompileError::Other(format!(
                "input too short for item at offset {}: need at least 33 more bytes, got {}",
                offset,
                input.len() - offset
            )));
        }
        let key: [u8; 32] = input
            .get(offset..offset + 32)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| {
                PrecompileError::Other(format!("input too short for key at offset {offset}"))
            })?;
        offset += 32;

        let key_nibbles = Nibbles::unpack(B256::from(key));

        // Read has_value flag
        let has_value = *input.get(offset).ok_or_else(|| {
            PrecompileError::Other(format!("input too short for has_value at offset {offset}"))
        })?;
        offset += 1;

        let expected_value = if has_value == 0x01 {
            // Inclusion: read value length + value
            if offset + 4 > input.len() {
                return Err(PrecompileError::Other(
                    "input too short: missing value length".into(),
                ));
            }
            let val_len = u32::from_be_bytes(
                input
                    .get(offset..offset + 4)
                    .and_then(|s| s.try_into().ok())
                    .ok_or_else(|| {
                        PrecompileError::Other("input too short: missing value length".into())
                    })?,
            ) as usize;
            offset += 4;

            if offset + val_len > input.len() {
                return Err(PrecompileError::Other(format!(
                    "input too short: expected {} value bytes, only {} remaining",
                    val_len,
                    input.len() - offset
                )));
            }
            let value = input
                .get(offset..offset + val_len)
                .ok_or_else(|| PrecompileError::Other("input too short for value".into()))?
                .to_vec();
            offset += val_len;
            Some(value)
        } else {
            None
        };

        // Read per-key proof node count
        if offset + 4 > input.len() {
            return Err(PrecompileError::Other(
                "input too short: missing proof_node_count".into(),
            ));
        }
        let proof_node_count = u32::from_be_bytes(
            input
                .get(offset..offset + 4)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| {
                    PrecompileError::Other("input too short: missing proof_node_count".into())
                })?,
        ) as usize;
        offset += 4;

        total_proof_nodes += proof_node_count as u64;

        // Read proof nodes for this key
        let mut proof_nodes: Vec<Bytes> = Vec::with_capacity(proof_node_count);
        for _ in 0..proof_node_count {
            if offset + 4 > input.len() {
                return Err(PrecompileError::Other(
                    "truncated proof node: missing length prefix".into(),
                ));
            }
            let node_len = u32::from_be_bytes(
                input
                    .get(offset..offset + 4)
                    .and_then(|s| s.try_into().ok())
                    .ok_or_else(|| {
                        PrecompileError::Other(
                            "truncated proof node: missing length prefix".into(),
                        )
                    })?,
            ) as usize;
            offset += 4;

            if offset + node_len > input.len() {
                return Err(PrecompileError::Other(format!(
                    "truncated proof node: expected {} bytes, only {} remaining",
                    node_len,
                    input.len() - offset
                )));
            }
            proof_nodes.push(Bytes::copy_from_slice(
                input
                    .get(offset..offset + node_len)
                    .ok_or_else(|| PrecompileError::Other("truncated proof node".into()))?,
            ));
            offset += node_len;
        }

        items.push((key_nibbles, expected_value, proof_nodes));
    }

    // Calculate and check gas
    let gas_cost =
        BASE_COST + (item_count as u64) * PER_ITEM_COST + total_proof_nodes * PER_PROOF_NODE_COST;
    if gas_cost > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }

    // Verify each key's proof individually using alloy-trie
    for (key_nibbles, expected_value, proof_nodes) in &items {
        alloy_trie::proof::verify_proof(
            root_b256,
            key_nibbles.clone(),
            expected_value.clone(),
            false,
            proof_nodes,
        )
        .map_err(|e| PrecompileError::Other(format!("proof verification failed: {e}")))?;
    }

    // Success: return 0x01 left-padded to 32 bytes
    let mut output = vec![0u8; 32];
    if let Some(byte) = output.get_mut(31) {
        *byte = 0x01;
    }

    Ok(PrecompileOutput::new(gas_cost, output.into()))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::identity_op
)]
mod tests {
    use super::*;
    use alloy_primitives::keccak256;
    use alloy_trie::proof::ProofRetainer;
    use alloy_trie::HashBuilder;
    use revm::precompile::PrecompileError;
    use revm::primitives::Bytes as RevmBytes;
    use std::collections::BTreeMap;

    /// Helper: build a trie, insert entries (already-hashed keys), return root
    /// and the entries map for proof generation.
    fn build_test_trie(entries: &[(&[u8], &[u8])]) -> (B256, BTreeMap<B256, Vec<u8>>) {
        let mut map = BTreeMap::new();
        for (key, value) in entries {
            let k = B256::from_slice(key);
            map.insert(k, value.to_vec());
        }
        let mut builder = HashBuilder::default();
        for (key, value) in &map {
            builder.add_leaf(Nibbles::unpack(key), value, false);
        }
        let root = builder.root();
        (root, map)
    }

    /// Helper: generate per-key proofs for given (already-hashed) keys.
    fn generate_per_key_proofs(
        entries: &BTreeMap<B256, Vec<u8>>,
        keys: &[&[u8]],
    ) -> Vec<Vec<Vec<u8>>> {
        let targets: Vec<Nibbles> = keys
            .iter()
            .map(|k| Nibbles::unpack(B256::from_slice(k)))
            .collect();

        let retainer = ProofRetainer::new(targets.clone());
        let mut builder = HashBuilder::default().with_proof_retainer(retainer);
        for (key, value) in entries {
            builder.add_leaf(Nibbles::unpack(key), value, false);
        }
        let _root = builder.root();
        let proof_nodes = builder.take_proof_nodes();

        targets
            .iter()
            .map(|target| {
                proof_nodes
                    .matching_nodes_sorted(target)
                    .into_iter()
                    .map(|(_, node)| node.to_vec())
                    .collect()
            })
            .collect()
    }

    /// Helper: encode precompile input with per-key proofs.
    fn encode_input(
        root: &[u8; 32],
        items: &[(Vec<u8>, Option<Vec<u8>>, Vec<Vec<u8>>)],
    ) -> Vec<u8> {
        let mut input = Vec::new();
        input.extend_from_slice(root);
        input.extend_from_slice(&(items.len() as u32).to_be_bytes());
        for (key, value, proof_nodes) in items {
            input.extend_from_slice(key);
            match value {
                Some(v) => {
                    input.push(0x01);
                    input.extend_from_slice(&(v.len() as u32).to_be_bytes());
                    input.extend_from_slice(v);
                }
                None => {
                    input.push(0x00);
                }
            }
            input.extend_from_slice(&(proof_nodes.len() as u32).to_be_bytes());
            for node in proof_nodes {
                input.extend_from_slice(&(node.len() as u32).to_be_bytes());
                input.extend_from_slice(node);
            }
        }
        input
    }

    /// Helper: hash a logical key with keccak256.
    fn hash_key(logical_key: &[u8]) -> [u8; 32] {
        keccak256(logical_key).into()
    }

    #[test]
    fn test_single_key_inclusion() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key]);

        let items = vec![(hashed_key.to_vec(), Some(value), proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.bytes[31], 0x01, "Should return success");
    }

    #[test]
    fn test_single_key_exclusion() {
        let hashed_key_present = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key_present, &value)]);

        let hashed_key_absent = hash_key(b"nonexistent");
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key_absent]);

        let items = vec![(hashed_key_absent.to_vec(), None, proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_multiple_keys_inclusion() {
        let keys_and_values: Vec<([u8; 32], Vec<u8>)> = (0..5u64)
            .map(|i| {
                let key = hash_key(format!("key_{}", i).as_bytes());
                let value = i.to_be_bytes().to_vec();
                (key, value)
            })
            .collect();

        let trie_entries: Vec<(&[u8], &[u8])> = keys_and_values
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
            .collect();
        let (root, entries) = build_test_trie(&trie_entries);

        // Prove keys 1, 3
        let query_keys: Vec<&[u8]> = vec![
            keys_and_values[1].0.as_slice(),
            keys_and_values[3].0.as_slice(),
        ];
        let proofs = generate_per_key_proofs(&entries, &query_keys);

        let items = vec![
            (
                keys_and_values[1].0.to_vec(),
                Some(keys_and_values[1].1.clone()),
                proofs[0].clone(),
            ),
            (
                keys_and_values[3].0.to_vec(),
                Some(keys_and_values[3].1.clone()),
                proofs[1].clone(),
            ),
        ];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_mixed_inclusion_exclusion() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);

        let hashed_key_absent = hash_key(b"missing");
        let query_keys: Vec<&[u8]> = vec![&hashed_key, &hashed_key_absent];
        let proofs = generate_per_key_proofs(&entries, &query_keys);

        let items = vec![
            (hashed_key.to_vec(), Some(value), proofs[0].clone()),
            (hashed_key_absent.to_vec(), None, proofs[1].clone()),
        ];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_invalid_proof() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let mut proofs = generate_per_key_proofs(&entries, &[&hashed_key]);

        // Tamper with a proof node
        if let Some(node) = proofs[0].first_mut() {
            if !node.is_empty() {
                node[0] ^= 0xFF;
            }
        }

        let items = vec![(hashed_key.to_vec(), Some(value), proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with tampered proof");
    }

    #[test]
    fn test_wrong_root() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (_root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key]);

        let wrong_root = [0xFF; 32];
        let items = vec![(hashed_key.to_vec(), Some(value), proofs[0].clone())];
        let input = encode_input(&wrong_root, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with wrong root");
    }

    #[test]
    fn test_wrong_value() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key]);

        let wrong_value = 99u64.to_be_bytes().to_vec();
        let items = vec![(hashed_key.to_vec(), Some(wrong_value), proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with wrong value");
    }

    #[test]
    fn test_claim_inclusion_for_absent_key() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (_root, entries) = build_test_trie(&[(&hashed_key, &value)]);

        let hashed_key_absent = hash_key(b"nonexistent");
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key_absent]);
        let (root, _) = build_test_trie(&[(&hashed_key, &value)]);

        // Claim inclusion for absent key with fake value
        let items = vec![(
            hashed_key_absent.to_vec(),
            Some(vec![1, 2, 3]),
            proofs[0].clone(),
        )];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(
            result.is_err(),
            "Should fail when claiming inclusion for absent key"
        );
    }

    #[test]
    fn test_out_of_gas() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key]);

        let items = vec![(hashed_key.to_vec(), Some(value), proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_input_length() {
        let input = vec![0u8; 30];
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert!(msg.contains("input too short"));
            }
            other => panic!("Expected input too short error, got {:?}", other),
        }
    }

    #[test]
    fn test_zero_item_count() {
        let mut input = vec![0u8; 32]; // root
        input.extend_from_slice(&0u32.to_be_bytes()); // item_count = 0
        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert!(msg.contains("item_count must be > 0"));
            }
            other => panic!("Expected item_count error, got {:?}", other),
        }
    }

    #[test]
    fn test_truncated_proof_node() {
        let root = [0u8; 32];
        let key = [0u8; 32];
        let mut input = Vec::new();
        input.extend_from_slice(&root);
        input.extend_from_slice(&1u32.to_be_bytes()); // 1 item
        input.extend_from_slice(&key);
        input.push(0x00); // exclusion
        input.extend_from_slice(&1u32.to_be_bytes()); // 1 proof node for this key
        input.extend_from_slice(&100u32.to_be_bytes()); // claims 100 bytes
        input.extend_from_slice(&[0u8; 10]); // only 10 bytes

        let result = mpt_verify(&RevmBytes::from(input), 100_000);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert!(msg.contains("truncated proof node"));
            }
            other => panic!("Expected truncated error, got {:?}", other),
        }
    }

    #[test]
    fn test_gas_calculation() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, entries) = build_test_trie(&[(&hashed_key, &value)]);
        let proofs = generate_per_key_proofs(&entries, &[&hashed_key]);
        let proof_count = proofs[0].len() as u64;

        let items = vec![(hashed_key.to_vec(), Some(value), proofs[0].clone())];
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 100_000).unwrap();

        let expected_gas = BASE_COST + 1 * PER_ITEM_COST + proof_count * PER_PROOF_NODE_COST;
        assert_eq!(result.gas_used, expected_gas);
    }

    #[test]
    fn test_large_trie_proof() {
        // Build a trie with 100 entries, prove a subset
        let entries: Vec<([u8; 32], Vec<u8>)> = (0..100u64)
            .map(|i| {
                let key = hash_key(format!("key_{}", i).as_bytes());
                let value = i.to_be_bytes().to_vec();
                (key, value)
            })
            .collect();

        let trie_entries: Vec<(&[u8], &[u8])> = entries
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
            .collect();
        let (root, entry_map) = build_test_trie(&trie_entries);

        // Prove every 13th key
        let query_indices: Vec<usize> = (0..100).step_by(13).collect();
        let query_keys: Vec<&[u8]> = query_indices
            .iter()
            .map(|&i| entries[i].0.as_slice())
            .collect();
        let proofs = generate_per_key_proofs(&entry_map, &query_keys);

        let items: Vec<(Vec<u8>, Option<Vec<u8>>, Vec<Vec<u8>>)> = query_indices
            .iter()
            .enumerate()
            .map(|(idx, &i)| {
                (
                    entries[i].0.to_vec(),
                    Some(entries[i].1.clone()),
                    proofs[idx].clone(),
                )
            })
            .collect();
        let input = encode_input(&root.0, &items);
        let result = mpt_verify(&RevmBytes::from(input), 1_000_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }
}
