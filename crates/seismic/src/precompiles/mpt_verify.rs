//! MPT (Merkle Patricia Trie) proof verification precompile.
//!
//! Verifies an MPT inclusion/exclusion proof for one or more key-value pairs
//! against a given root. Used to verify Summit consensus state proofs on-chain
//! via EIP-4788.
//!
//! The caller provides the expected values alongside keys. The precompile
//! verifies the proof is valid using `trie_db::proof::verify_proof`. This
//! matches Summit's `getStateProof` RPC endpoint which returns shared proofs
//! for multiple keys.

use revm::precompile::{
    u64_to_address, Precompile, PrecompileError, PrecompileId, PrecompileOutput, PrecompileResult,
};

type Layout = reference_trie::ExtensionLayout;

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
/// [next 4 bytes]      : proof_count — u32 big-endian, number of proof nodes
/// [rest]              : proof_nodes — length-prefixed proof nodes:
///                         For each node: [0..4] u32 BE node length, [4..4+L] node bytes
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
    let root: [u8; 32] = input[0..32].try_into().expect("slice is 32 bytes");

    // Parse item count
    let item_count = u32::from_be_bytes(input[32..36].try_into().expect("slice is 4 bytes"));
    if item_count == 0 {
        return Err(PrecompileError::Other("item_count must be > 0".into()));
    }

    // Parse items: (key, Option<value>)
    let mut offset = 36usize;
    let mut items: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::with_capacity(item_count as usize);

    for _ in 0..item_count {
        // Read key (32 bytes)
        if offset + 33 > input.len() {
            return Err(PrecompileError::Other(format!(
                "input too short for item at offset {}: need at least 33 more bytes, got {}",
                offset,
                input.len() - offset
            )));
        }
        let key = input[offset..offset + 32].to_vec();
        offset += 32;

        // Read has_value flag
        let has_value = input[offset];
        offset += 1;

        if has_value == 0x01 {
            // Inclusion: read value length + value
            if offset + 4 > input.len() {
                return Err(PrecompileError::Other(
                    "input too short: missing value length".into(),
                ));
            }
            let val_len = u32::from_be_bytes(
                input[offset..offset + 4]
                    .try_into()
                    .expect("slice is 4 bytes"),
            ) as usize;
            offset += 4;

            if offset + val_len > input.len() {
                return Err(PrecompileError::Other(format!(
                    "input too short: expected {} value bytes, only {} remaining",
                    val_len,
                    input.len() - offset
                )));
            }
            let value = input[offset..offset + val_len].to_vec();
            offset += val_len;

            items.push((key, Some(value)));
        } else {
            // Exclusion: no value
            items.push((key, None));
        }
    }

    // Parse proof count
    if offset + 4 > input.len() {
        return Err(PrecompileError::Other("input too short: missing proof_count".into()));
    }
    let proof_count = u32::from_be_bytes(
        input[offset..offset + 4]
            .try_into()
            .expect("slice is 4 bytes"),
    ) as u64;
    offset += 4;

    // Calculate and check gas
    let gas_cost =
        BASE_COST + (item_count as u64) * PER_ITEM_COST + proof_count * PER_PROOF_NODE_COST;
    if gas_cost > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }

    // Parse proof nodes
    let mut proof_nodes: Vec<Vec<u8>> = Vec::with_capacity(proof_count as usize);
    for _ in 0..proof_count {
        if offset + 4 > input.len() {
            return Err(PrecompileError::Other(
                "truncated proof node: missing length prefix".into(),
            ));
        }
        let node_len = u32::from_be_bytes(
            input[offset..offset + 4]
                .try_into()
                .expect("slice is 4 bytes"),
        ) as usize;
        offset += 4;

        if offset + node_len > input.len() {
            return Err(PrecompileError::Other(format!(
                "truncated proof node: expected {} bytes, only {} remaining",
                node_len,
                input.len() - offset
            )));
        }
        proof_nodes.push(input[offset..offset + node_len].to_vec());
        offset += node_len;
    }

    // Verify proof using trie_db
    trie_db::proof::verify_proof::<Layout, _, _, _>(&root, &proof_nodes, &items)
        .map_err(|e| PrecompileError::Other(format!("proof verification failed: {e}")))?;

    // Success: return 0x01 left-padded to 32 bytes
    let mut output = vec![0u8; 32];
    output[31] = 0x01;

    Ok(PrecompileOutput::new(gas_cost, output.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hash_db::Hasher;
    use memory_db::{HashKey, MemoryDB};
    use revm::precompile::PrecompileError;
    use revm::primitives::Bytes;

    type KeccakHasher = keccak_hasher::KeccakHasher;
    type TestTrieMemDB = MemoryDB<KeccakHasher, HashKey<KeccakHasher>, Vec<u8>>;

    /// Helper: build a trie, insert entries, return (root, memdb).
    fn build_test_trie(entries: &[(&[u8], &[u8])]) -> ([u8; 32], TestTrieMemDB) {
        let mut memdb = TestTrieMemDB::default();
        let mut root = Default::default();
        {
            use trie_db::{TrieDBMutBuilder, TrieMut};
            let mut trie = TrieDBMutBuilder::<Layout>::new(&mut memdb, &mut root).build();
            for (key, value) in entries {
                trie.insert(key, value).expect("insert failed");
            }
        }
        (root, memdb)
    }

    /// Helper: generate proof for given keys.
    fn generate_proof(memdb: &TestTrieMemDB, root: &[u8; 32], keys: &[&[u8]]) -> Vec<Vec<u8>> {
        let key_vecs: Vec<Vec<u8>> = keys.iter().map(|k| k.to_vec()).collect();
        let key_refs: Vec<&Vec<u8>> = key_vecs.iter().collect();
        trie_db::proof::generate_proof::<_, Layout, _, _>(memdb, root, key_refs)
            .expect("proof generation failed")
    }

    /// Helper: encode precompile input from root, items, and proof nodes.
    fn encode_input(
        root: &[u8; 32],
        items: &[(Vec<u8>, Option<Vec<u8>>)],
        proof_nodes: &[Vec<u8>],
    ) -> Vec<u8> {
        let mut input = Vec::new();
        input.extend_from_slice(root);
        input.extend_from_slice(&(items.len() as u32).to_be_bytes());
        for (key, value) in items {
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
        }
        input.extend_from_slice(&(proof_nodes.len() as u32).to_be_bytes());
        for node in proof_nodes {
            input.extend_from_slice(&(node.len() as u32).to_be_bytes());
            input.extend_from_slice(node);
        }
        input
    }

    /// Helper: hash a logical key with keccak256.
    fn hash_key(logical_key: &[u8]) -> [u8; 32] {
        KeccakHasher::hash(logical_key)
    }

    #[test]
    fn test_single_key_inclusion() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let proof = generate_proof(&memdb, &root, &[&hashed_key]);

        let items = vec![(hashed_key.to_vec(), Some(value))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());

        let output = result.unwrap();
        assert_eq!(output.bytes[31], 0x01, "Should return success");
    }

    #[test]
    fn test_single_key_exclusion() {
        let hashed_key_present = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key_present, &value)]);

        let hashed_key_absent = hash_key(b"nonexistent");
        let proof = generate_proof(&memdb, &root, &[&hashed_key_absent]);

        let items = vec![(hashed_key_absent.to_vec(), None)];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
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
        let (root, memdb) = build_test_trie(&trie_entries);

        // Prove keys 1, 3
        let query_keys: Vec<&[u8]> = vec![
            keys_and_values[1].0.as_slice(),
            keys_and_values[3].0.as_slice(),
        ];
        let proof = generate_proof(&memdb, &root, &query_keys);

        let items = vec![
            (
                keys_and_values[1].0.to_vec(),
                Some(keys_and_values[1].1.clone()),
            ),
            (
                keys_and_values[3].0.to_vec(),
                Some(keys_and_values[3].1.clone()),
            ),
        ];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_mixed_inclusion_exclusion() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);

        let hashed_key_absent = hash_key(b"missing");
        let query_keys: Vec<&[u8]> = vec![&hashed_key, &hashed_key_absent];
        let proof = generate_proof(&memdb, &root, &query_keys);

        let items = vec![
            (hashed_key.to_vec(), Some(value)),
            (hashed_key_absent.to_vec(), None),
        ];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }

    #[test]
    fn test_invalid_proof() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let mut proof = generate_proof(&memdb, &root, &[&hashed_key]);

        // Tamper with a proof node
        if let Some(node) = proof.first_mut() {
            if !node.is_empty() {
                node[0] ^= 0xFF;
            }
        }

        let items = vec![(hashed_key.to_vec(), Some(value))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with tampered proof");
    }

    #[test]
    fn test_wrong_root() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let proof = generate_proof(&memdb, &root, &[&hashed_key]);

        let wrong_root = [0xFF; 32];
        let items = vec![(hashed_key.to_vec(), Some(value))];
        let input = encode_input(&wrong_root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with wrong root");
    }

    #[test]
    fn test_wrong_value() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let proof = generate_proof(&memdb, &root, &[&hashed_key]);

        let wrong_value = 99u64.to_be_bytes().to_vec();
        let items = vec![(hashed_key.to_vec(), Some(wrong_value))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(result.is_err(), "Should fail with wrong value");
    }

    #[test]
    fn test_claim_inclusion_for_absent_key() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);

        let hashed_key_absent = hash_key(b"nonexistent");
        let proof = generate_proof(&memdb, &root, &[&hashed_key_absent]);

        // Claim inclusion for absent key with fake value
        let items = vec![(hashed_key_absent.to_vec(), Some(vec![1, 2, 3]))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000);
        assert!(
            result.is_err(),
            "Should fail when claiming inclusion for absent key"
        );
    }

    #[test]
    fn test_out_of_gas() {
        let hashed_key = hash_key(b"epoch");
        let value = 42u64.to_be_bytes().to_vec();
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let proof = generate_proof(&memdb, &root, &[&hashed_key]);

        let items = vec![(hashed_key.to_vec(), Some(value))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_input_length() {
        let input = vec![0u8; 30];
        let result = mpt_verify(&Bytes::from(input), 100_000);
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
        let result = mpt_verify(&Bytes::from(input), 100_000);
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
        input.extend_from_slice(&1u32.to_be_bytes()); // 1 proof node
        input.extend_from_slice(&100u32.to_be_bytes()); // claims 100 bytes
        input.extend_from_slice(&[0u8; 10]); // only 10 bytes

        let result = mpt_verify(&Bytes::from(input), 100_000);
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
        let (root, memdb) = build_test_trie(&[(&hashed_key, &value)]);
        let proof = generate_proof(&memdb, &root, &[&hashed_key]);
        let proof_count = proof.len() as u64;

        let items = vec![(hashed_key.to_vec(), Some(value))];
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 100_000).unwrap();

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
        let (root, memdb) = build_test_trie(&trie_entries);

        // Prove every 13th key
        let query_indices: Vec<usize> = (0..100).step_by(13).collect();
        let query_keys: Vec<&[u8]> = query_indices
            .iter()
            .map(|&i| entries[i].0.as_slice())
            .collect();
        let proof = generate_proof(&memdb, &root, &query_keys);

        let items: Vec<(Vec<u8>, Option<Vec<u8>>)> = query_indices
            .iter()
            .map(|&i| (entries[i].0.to_vec(), Some(entries[i].1.clone())))
            .collect();
        let input = encode_input(&root, &items, &proof);
        let result = mpt_verify(&Bytes::from(input), 1_000_000);
        assert!(result.is_ok(), "Expected success, got {:?}", result.err());
    }
}
