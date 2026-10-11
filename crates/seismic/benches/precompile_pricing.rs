use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use seismic_revm::precompiles::aes::aes_gcm_enc::precompile_encrypt;

fn bench_aes_encrypt(c: &mut Criterion) {
    let mut group = c.benchmark_group("precompile/aes_encrypt");

    for size in [0usize, 16, 64, 256, 1024, 4096] {
        let mut input = vec![0u8; 44 + size];

        for (i, byte) in input.iter_mut().enumerate() {
            *byte = (i as u8).wrapping_mul(31);
        }

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &input,
            |b, input| {
                b.iter(|| {
                    black_box(
                        precompile_encrypt(black_box(input), u64::MAX)
                            .expect("benchmark input must be valid"),
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_aes_encrypt);
criterion_main!(benches);
