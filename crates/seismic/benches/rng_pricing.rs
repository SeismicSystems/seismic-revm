//! Benchmarks grounding the RNG precompile (0x64) gas schedule in measured time.
//!
//! Run with: cargo bench -p seismic-revm --bench rng_pricing
//!
//! The precompile charges (see `precompiles/rng/precompile.rs`):
//! ```text
//! rounds = ceil(output_len / 32)
//! info_len = 121 + pers_len + (if output_len > 8160 { 4 } else { 0 })
//! cost = 3500 + 5 * ceil(pers_len / 32)
//!             + rounds * (120 + 24 * ceil((info_len + 33) / 32))
//! ```
//! HKDF-Expand re-hashes the whole `info` in every 32-byte round, so the real
//! work is ~ rounds * info_len. This bench sweeps (pers_len, output_len), times
//! `RootRng::derive_bytes`, and reports nanoseconds per gas unit. A well-priced
//! schedule keeps ns/gas roughly flat across the grid; a spread means some
//! shapes are cheaper per gas than others and could be abused.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use revm::primitives::B256;
use seismic_revm::precompiles::rng::domain_sep_rng::RootRng;
use std::time::Instant;

// Mirrors the private constants in precompiles/rng/precompile.rs.
const RNG_BASE_COST: u64 = 3500;
const RNG_COPY_WORD_COST: u64 = 5;
const RNG_ROUND_BASE_COST: u64 = 2 * 60;
const RNG_HASH_WORD_COST: u64 = 2 * 12;
const INFO_PREFIX_LEN: u64 = 121;
const MAX_HKDF_OUTPUT: u64 = 255 * 32;

fn gas_cost(pers_len: u64, output_len: u64) -> u64 {
    let counter = if output_len > MAX_HKDF_OUTPUT { 4 } else { 0 };
    let round_input = pers_len + INFO_PREFIX_LEN + counter + 32 + 1;
    let round_cost = round_input.div_ceil(32) * RNG_HASH_WORD_COST + RNG_ROUND_BASE_COST;
    pers_len.div_ceil(32) * RNG_COPY_WORD_COST
        + RNG_BASE_COST
        + output_len.div_ceil(32) * round_cost
}

fn make_rng() -> RootRng {
    let mut rng = RootRng::new([7u8; 64]);
    rng.append_parent_block_hash(&B256::ZERO);
    rng.append_tx_hash_accumulator(&B256::ZERO);
    rng.append_tx(&B256::ZERO);
    rng.append_gas_left(1_000_000);
    rng
}

/// (pers_len, output_len) shapes: small calls, pers-heavy, output-heavy, chunked.
const GRID: &[(usize, usize)] = &[
    (0, 32),
    (32, 32),
    (0, 1024),
    (1024, 32),
    (1024, 1024),
    (16 * 1024, 1024),
    (1024, 16 * 1024),
    (64 * 1024, 1024),
    (0, 8160),
    (0, 8161),
    (4096, 64 * 1024),
];

fn bench_rng(c: &mut Criterion) {
    let rng = make_rng();
    let mut group = c.benchmark_group("rng_0x64");
    for &(pers_len, output_len) in GRID {
        let pers = vec![0xAAu8; pers_len];
        let gas = gas_cost(pers_len as u64, output_len as u64);
        group.throughput(Throughput::Elements(gas));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("pers{pers_len}_out{output_len}")),
            &(pers, output_len),
            |b, (pers, output_len)| {
                b.iter(|| black_box(rng.derive_bytes(black_box(pers), black_box(*output_len))))
            },
        );
    }
    group.finish();
}

/// Prints ns/gas per shape so the spread is visible without reading criterion output.
fn report(_c: &mut Criterion) {
    let rng = make_rng();
    println!(
        "\n{:>9} {:>9} {:>12} {:>12} {:>9}",
        "pers", "output", "gas", "ns", "ns/gas"
    );
    for &(pers_len, output_len) in GRID {
        let pers = vec![0xAAu8; pers_len];
        let iters = 20u32;
        let start = Instant::now();
        for _ in 0..iters {
            black_box(rng.derive_bytes(black_box(&pers), black_box(output_len)));
        }
        let ns = start.elapsed().as_nanos() as f64 / iters as f64;
        let gas = gas_cost(pers_len as u64, output_len as u64);
        println!(
            "{pers_len:>9} {output_len:>9} {gas:>12} {ns:>12.0} {:>9.4}",
            ns / gas as f64
        );
    }
}

criterion_group!(benches, report, bench_rng);
criterion_main!(benches);
