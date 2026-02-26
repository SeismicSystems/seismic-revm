use alloy_primitives::{keccak256, B256};
use alloy_trie::proof::ProofRetainer;
use alloy_trie::HashBuilder;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nybbles::Nibbles;
use revm::primitives::Bytes;
use seismic_revm::precompiles::mpt_verify::mpt_verify;
use std::collections::BTreeMap;

fn hash_key(logical_key: &[u8]) -> [u8; 32] {
    keccak256(logical_key).into()
}

fn build_test_trie(entries: &[(&[u8], &[u8])]) -> (B256, BTreeMap<B256, Vec<u8>>) {
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        let k = B256::from_slice(key);
        map.insert(k, value.to_vec());
    }
    let mut builder = HashBuilder::default();
    for (key, value) in &map {
        builder.add_leaf(Nibbles::unpack(key), value);
    }
    let root = builder.root();
    (root, map)
}

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
        builder.add_leaf(Nibbles::unpack(key), value);
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

fn encode_input(root: &[u8; 32], items: &[(Vec<u8>, Option<Vec<u8>>, Vec<Vec<u8>>)]) -> Bytes {
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
    Bytes::from(input)
}

/// Build a trie with `trie_size` entries and prove `query_count` of them.
fn prepare_bench_input(trie_size: usize, query_count: usize) -> Bytes {
    let entries: Vec<([u8; 32], Vec<u8>)> = (0..trie_size)
        .map(|i| {
            let key = hash_key(format!("key_{}", i).as_bytes());
            let value = (i as u64).to_be_bytes().to_vec();
            (key, value)
        })
        .collect();

    let trie_entries: Vec<(&[u8], &[u8])> = entries
        .iter()
        .map(|(k, v)| (k.as_slice(), v.as_slice()))
        .collect();
    let (root, entry_map) = build_test_trie(&trie_entries);

    // Pick evenly spaced keys to query
    let step = trie_size.max(1) / query_count.max(1);
    let query_indices: Vec<usize> = (0..query_count).map(|i| i * step.max(1)).collect();
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

    encode_input(&root.0, &items)
}

fn bench_mpt_verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpt_verify");

    // Vary trie size with 1 query item
    for trie_size in [1, 10, 100, 1000] {
        let input = prepare_bench_input(trie_size, 1);
        group.bench_with_input(
            BenchmarkId::new("trie_size", trie_size),
            &input,
            |b, input| {
                b.iter(|| mpt_verify(black_box(input), 1_000_000).unwrap());
            },
        );
    }

    // Vary query count with trie size 100
    for query_count in [1, 5, 10, 20] {
        let input = prepare_bench_input(100, query_count);
        group.bench_with_input(
            BenchmarkId::new("query_count", query_count),
            &input,
            |b, input| {
                b.iter(|| mpt_verify(black_box(input), 1_000_000).unwrap());
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_mpt_verify);
criterion_main!(benches);
