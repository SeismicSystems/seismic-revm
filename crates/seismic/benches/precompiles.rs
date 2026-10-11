//! Benchmarks for Seismic's ECDH and HKDF precompiles.
//!
//! Run with: `cargo bench -p seismic-revm --bench precompiles`
//!
//! Upstream `ecrecover` is included only as a familiar secp256k1 Ethereum
//! precompile and pricing reference; it is not operationally equivalent to ECDH.
//! SHA-256 provides a reference for HKDF's size-dependent hashing work.
//! Timings are host-specific and must not be treated as production or TDX gas-pricing data.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use revm::{
    precompile::{hash::sha256_run, secp256k1::ec_recover_run},
    primitives::{hex, keccak256, Bytes},
};
use secp256k1::{Message, Secp256k1, SecretKey};
use seismic_revm::precompiles::{
    ecdh_derive_sym_key::derive_symmetric_key, hkdf_derive_sym_key::hkdf_derive_symmetric_key,
};

const GAS_LIMIT: u64 = u64::MAX;
const HKDF_INPUT_SIZES: [usize; 10] = [0, 1, 31, 32, 33, 63, 64, 65, 256, 1024];

fn ecdh_input() -> Bytes {
    let secret_key = hex!("7e38022030c40773cc561c1cc9c0053e48b0be2cee33c13495f096942ea176ef");
    let public_key = hex!("03f176e697b5b0c4799f1816f5fe114263d1c01a84ad296129f994278499f0842e");
    [secret_key.as_slice(), public_key.as_slice()]
        .concat()
        .into()
}

fn ecrecover_input() -> Bytes {
    let message_hash = keccak256(b"seismic precompile benchmark");
    let secret_key = SecretKey::from_byte_array([1u8; 32]).expect("fixed key is valid");
    let signature = Secp256k1::signing_only()
        .sign_ecdsa_recoverable(Message::from_digest(message_hash.0), &secret_key);
    let (recovery_id, signature_bytes) = signature.serialize_compact();

    let mut input = [0u8; 128];
    input[..32].copy_from_slice(message_hash.as_slice());
    input[63] = i32::from(recovery_id) as u8 + 27;
    input[64..].copy_from_slice(&signature_bytes);
    input.into()
}

fn bench_ecdh(c: &mut Criterion) {
    let ecdh_input = ecdh_input();
    let ecrecover_input = ecrecover_input();

    assert_eq!(
        derive_symmetric_key(&ecdh_input, GAS_LIMIT)
            .expect("benchmark input is valid")
            .bytes
            .len(),
        32
    );
    assert_eq!(
        ec_recover_run(&ecrecover_input, GAS_LIMIT)
            .expect("benchmark input is valid")
            .bytes
            .len(),
        32
    );

    let mut group = c.benchmark_group("precompiles/ecdh");

    group.bench_function("seismic_ecdh", |b| {
        b.iter(|| {
            black_box(
                derive_symmetric_key(black_box(ecdh_input.as_ref()), GAS_LIMIT)
                    .expect("benchmark input is valid"),
            )
        })
    });

    group.bench_function("upstream_ecrecover", |b| {
        b.iter(|| {
            black_box(
                ec_recover_run(black_box(ecrecover_input.as_ref()), GAS_LIMIT)
                    .expect("benchmark input is valid"),
            )
        })
    });

    group.finish();
}

fn bench_hkdf(c: &mut Criterion) {
    let mut group = c.benchmark_group("precompiles/hkdf");

    for input_size in HKDF_INPUT_SIZES {
        let input = vec![0x42; input_size];

        group.bench_with_input(
            BenchmarkId::new("seismic_hkdf", input_size),
            &input,
            |b, input| {
                b.iter(|| {
                    black_box(
                        hkdf_derive_symmetric_key(black_box(input.as_slice()), GAS_LIMIT)
                            .expect("HKDF accepts arbitrary input"),
                    )
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("upstream_sha256", input_size),
            &input,
            |b, input| {
                b.iter(|| {
                    black_box(
                        sha256_run(black_box(input.as_slice()), GAS_LIMIT)
                            .expect("benchmark has sufficient gas"),
                    )
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_ecdh, bench_hkdf);
criterion_main!(benches);
