//! Gas-pricing benchmarks for the Seismic precompiles.
//!
//! Tracking issue: #104 — "Properly pricing our precompiles."
//!
//! The Seismic precompiles are priced defensively rather than from measurement.
//! `ECDH` (0x65) is charged 3,120 gas and `secp256k1_sign` (0x69) is charged
//! 3,000, both anchored to `ECRecover`'s 3,000 even though neither performs the
//! work `ECRecover` performs; the module comments say so outright ("we price it
//! near or above `ECRecover` to be safe"). #104 asks for those numbers to be
//! grounded in reality instead.
//!
//! This bench measures every Seismic precompile next to two Ethereum
//! precompiles whose gas is already fixed by consensus, so a gas figure can be
//! *derived* from the measurement rather than guessed:
//!
//! ```text
//! gas_per_ns      = reference_gas / reference_ns       (ECRecover, SHA-256)
//! derived_gas(op) = measured_ns * gas_per_ns
//! ```
//!
//! [`report`] runs that arithmetic once and prints a table, so a reviewer gets
//! the numbers directly instead of parsing criterion's output. The criterion
//! groups afterwards track the same operations over time.
//!
//! Read the derived figures as a **lower bound** on safe pricing. They price
//! the arithmetic only, and deliberately say nothing about the constant-time
//! and side-channel constraints that keep these precompiles on flat schedules.
//! A precompile that measures cheaper than its flat price is a candidate for
//! repricing, not an instruction to reprice it.
//!
//! ```text
//! cargo bench -p seismic-revm --bench precompile_pricing
//! ```

use std::time::Instant;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use revm::precompile::{hash::sha256_run, secp256k1::ec_recover_run, PrecompileResult};
use revm::primitives::hex;

use seismic_revm::precompiles::{
    aes::{precompile_decrypt, precompile_encrypt},
    ecdh_derive_sym_key::derive_symmetric_key,
    hkdf_derive_sym_key::hkdf_derive_symmetric_key,
    secp256k1_sign::secp256k1_sign_ecdsa_recoverable,
};

// ---------------------------------------------------------------------------
// Measurement
// ---------------------------------------------------------------------------

/// Gas limit handed to every call. Comfortably above every flat cost here, and
/// irrelevant to timing because none of these precompiles short-circuit on a
/// larger limit.
const GAS_LIMIT: u64 = 1_000_000;

/// Timed iterations per round.
const ITERS: u32 = 250;
/// Rounds timed; the fastest is kept.
const ROUNDS: u32 = 7;
/// Untimed iterations run first, to page in the code and settle branch
/// predictors.
const WARMUP: u32 = 25;

/// Nanoseconds per call, as the minimum over several rounds.
///
/// The minimum rather than the mean: a round that loses the CPU to a scheduler
/// preemption only ever reports *slower* than the truth, so the fastest round is
/// the closest estimate of the cost of the work itself. This is the same
/// reasoning `codspeed` applies, done without its instrumentation so the
/// derived gas can be computed inline.
fn time_ns(mut f: impl FnMut()) -> f64 {
    for _ in 0..WARMUP {
        f();
    }

    let mut best = f64::INFINITY;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        for _ in 0..ITERS {
            f();
        }
        let per_call = start.elapsed().as_nanos() as f64 / f64::from(ITERS);
        best = best.min(per_call);
    }
    best
}

/// Gas a precompile charges, read back from the call rather than restated here,
/// so this file cannot drift from the constants it is measuring.
///
/// Panics if the input is rejected: a benchmark feeding a precompile an input it
/// refuses would otherwise silently time the error path.
fn charged_gas(result: PrecompileResult) -> u64 {
    result
        .expect("benchmark input must be a valid call for this precompile")
        .gas_used
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Fixed inputs, so the numbers are reproducible across runs and machines.
///
/// Everything is built from the repo's own primitives — no external signing
/// crate — which also keeps the bench honest about what it measures: the
/// `ECRecover` reference input below is produced by `secp256k1_sign`, so it is a
/// signature this VM can actually recover, rather than a zeroed buffer that
/// would hit `ECRecover`'s early return and time the wrong path.
fn inputs() -> Inputs {
    // ECDH: 32-byte secret key ‖ 33-byte compressed public key. These are the
    // two key pairs the ECDH precompile's own tests use, so the input is known
    // to derive a key.
    let sk1 = hex!("7e38022030c40773cc561c1cc9c0053e48b0be2cee33c13495f096942ea176ef");
    let pk2 = hex!("02555d7b94d8afc4afdf5a03e9da73a408b6d19c865036bae833864d2353e85a25");
    let mut ecdh = Vec::with_capacity(65);
    ecdh.extend_from_slice(&sk1);
    ecdh.extend_from_slice(&pk2);

    // secp256k1 sign: 32-byte secret key ‖ 32-byte digest. 0x01 repeated is a
    // valid scalar (well below the group order); the digest is arbitrary.
    let sk_bytes = [0x01u8; 32];
    let digest = [0x42u8; 32];
    let mut sign = Vec::with_capacity(64);
    sign.extend_from_slice(&sk_bytes);
    sign.extend_from_slice(&digest);

    // HKDF: raw key material, length is the only input that moves the cost.
    let hkdf_short = vec![0xcd; 32];
    let hkdf_long = vec![0xcd; 256];

    // AES-GCM: 32-byte key ‖ 12-byte nonce ‖ payload. Encryption's output is
    // `ciphertext ‖ tag`, which is exactly what decryption expects, so feeding
    // one into the other guarantees the decryption input authenticates.
    let aes_key = [0x11; 32];
    let nonce = [0x22; 12];
    let mut enc_small = Vec::new();
    enc_small.extend_from_slice(&aes_key);
    enc_small.extend_from_slice(&nonce);
    enc_small.extend_from_slice(&[0x33; 16]);
    let mut enc_large = Vec::new();
    enc_large.extend_from_slice(&aes_key);
    enc_large.extend_from_slice(&nonce);
    enc_large.extend_from_slice(&[0x33; 1024]);

    let dec_from = |payload: &[u8]| -> Vec<u8> {
        let out = precompile_encrypt(payload, GAS_LIMIT)
            .expect("valid AES-GCM encryption input")
            .bytes;
        let mut input = Vec::new();
        input.extend_from_slice(&aes_key);
        input.extend_from_slice(&nonce);
        input.extend_from_slice(&out);
        input
    };
    let dec_small = dec_from(&enc_small);
    let dec_large = dec_from(&enc_large);

    // ECRecover reference. The sign precompile returns `r ‖ s ‖ recid`
    // (65 bytes), and ECRecover reads `msg ‖ v ‖ r ‖ s` (128 bytes) with `v`
    // right-aligned in its 32-byte slot, so one maps onto the other directly.
    let signed = secp256k1_sign_ecdsa_recoverable(&sign, GAS_LIMIT)
        .expect("signing a known-good input must succeed")
        .bytes;
    let mut ecrecover = Vec::with_capacity(128);
    ecrecover.extend_from_slice(&digest);
    ecrecover.extend_from_slice(&[0u8; 31]);
    ecrecover.push(27 + signed[64]);
    ecrecover.extend_from_slice(&signed[0..64]);
    debug_assert_eq!(ecrecover.len(), 128);
    // Guard the reference: ECRecover returns empty output for a `v` it does not
    // recognise, and that early path would silently become the baseline every
    // derived figure is divided by.
    assert!(
        !ec_recover_run(&ecrecover, GAS_LIMIT)
            .expect("valid ecrecover input")
            .bytes
            .is_empty(),
        "the ecrecover reference input did not recover an address, so it would \
         time the early-return path instead of the work"
    );

    // SHA-256 reference, measured at the same 32-byte size the HKDF extract
    // step hashes.
    let sha256 = vec![0x77; 32];

    Inputs {
        ecdh,
        sign,
        hkdf_short,
        hkdf_long,
        enc_small,
        enc_large,
        dec_small,
        dec_large,
        ecrecover,
        sha256,
    }
}

struct Inputs {
    ecdh: Vec<u8>,
    sign: Vec<u8>,
    hkdf_short: Vec<u8>,
    hkdf_long: Vec<u8>,
    enc_small: Vec<u8>,
    enc_large: Vec<u8>,
    dec_small: Vec<u8>,
    dec_large: Vec<u8>,
    ecrecover: Vec<u8>,
    sha256: Vec<u8>,
}

// ---------------------------------------------------------------------------
// The derivation
// ---------------------------------------------------------------------------

/// One measured precompile: what it charges today, and what it costs in time.
struct Row {
    name: &'static str,
    charged: u64,
    ns: f64,
    /// The reference this row is converted with.
    ///
    /// Gas-per-nanosecond is *not* constant across operation classes — the two
    /// references below differ by roughly two orders of magnitude — so a row is
    /// only meaningful against the reference doing the same kind of work.
    /// Converting the byte-oriented precompiles with the EC ratio, or with the
    /// mean of the two, produces numbers that are simply wrong.
    ratio: f64,
    /// Which reference `ratio` came from, shown in the table.
    reference: &'static str,
}

/// Prints the derivation table.
///
/// Each row is converted with its own class-matched reference: the EC
/// precompiles against `ECRecover`, the byte-oriented ones against SHA-256.
/// Both ratios and the spread between them are printed, because that
/// disagreement is a result in itself — it is why no single
/// gas-per-nanosecond scalar can price this instruction set.
///
/// The absolute numbers also move with the Cargo feature set; the caveat
/// printed under the table says how.
fn report() {
    let i = inputs();

    // --- references -------------------------------------------------------
    let ecrecover_ns = time_ns(|| {
        let _ = ec_recover_run(black_box(&i.ecrecover), black_box(GAS_LIMIT));
    });
    let sha256_ns = time_ns(|| {
        let _ = sha256_run(black_box(&i.sha256), black_box(GAS_LIMIT));
    });

    let ecrecover_gas = charged_gas(ec_recover_run(&i.ecrecover, GAS_LIMIT));
    let sha256_gas = charged_gas(sha256_run(&i.sha256, GAS_LIMIT));

    let gas_per_ns_ec = ecrecover_gas as f64 / ecrecover_ns;
    let gas_per_ns_bytes = sha256_gas as f64 / sha256_ns;

    // --- Seismic precompiles ---------------------------------------------
    let rows = vec![
        Row {
            name: "secp256k1_sign (0x69)",
            charged: charged_gas(secp256k1_sign_ecdsa_recoverable(&i.sign, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = secp256k1_sign_ecdsa_recoverable(black_box(&i.sign), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_ec,
            reference: "ECRecover",
        },
        Row {
            name: "ECDH+HKDF (0x65)",
            charged: charged_gas(derive_symmetric_key(&i.ecdh, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = derive_symmetric_key(black_box(&i.ecdh), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_ec,
            reference: "ECRecover",
        },
        Row {
            name: "HKDF (0x68, 32B in)",
            charged: charged_gas(hkdf_derive_symmetric_key(&i.hkdf_short, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = hkdf_derive_symmetric_key(black_box(&i.hkdf_short), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
        Row {
            name: "HKDF (0x68, 256B in)",
            charged: charged_gas(hkdf_derive_symmetric_key(&i.hkdf_long, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = hkdf_derive_symmetric_key(black_box(&i.hkdf_long), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
        Row {
            name: "AES-GCM enc (0x66, 16B)",
            charged: charged_gas(precompile_encrypt(&i.enc_small, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = precompile_encrypt(black_box(&i.enc_small), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
        Row {
            name: "AES-GCM enc (0x66, 1KiB)",
            charged: charged_gas(precompile_encrypt(&i.enc_large, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = precompile_encrypt(black_box(&i.enc_large), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
        Row {
            name: "AES-GCM dec (0x67, 16B)",
            charged: charged_gas(precompile_decrypt(&i.dec_small, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = precompile_decrypt(black_box(&i.dec_small), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
        Row {
            name: "AES-GCM dec (0x67, 1KiB)",
            charged: charged_gas(precompile_decrypt(&i.dec_large, GAS_LIMIT)),
            ns: time_ns(|| {
                let _ = precompile_decrypt(black_box(&i.dec_large), black_box(GAS_LIMIT));
            }),
            ratio: gas_per_ns_bytes,
            reference: "SHA-256",
        },
    ];

    // --- output -----------------------------------------------------------
    println!();
    println!("Seismic precompile pricing — measured, not modelled");
    println!("  references (gas fixed by consensus, same machine, same build):");
    println!(
        "    ECRecover  {:>6} gas @ {:>9.1} ns  ->  {:>8.4} gas/ns   [EC class]",
        ecrecover_gas, ecrecover_ns, gas_per_ns_ec
    );
    println!(
        "    SHA-256    {:>6} gas @ {:>9.1} ns  ->  {:>8.4} gas/ns   [byte class]",
        sha256_gas, sha256_ns, gas_per_ns_bytes
    );
    println!(
        "    spread between the two classes: {:.0}x",
        gas_per_ns_bytes / gas_per_ns_ec
    );
    println!();
    println!(
        "  {:<26} {:>11} {:>12} {:>13} {:>9}  {:<10}",
        "precompile", "charged", "measured", "derived gas", "charged/", "vs"
    );
    println!(
        "  {:<26} {:>11} {:>12} {:>13} {:>9}  {:<10}",
        "", "gas", "ns", "= ns x ratio", "derived", "reference"
    );
    println!("  {}", "-".repeat(88));
    for row in &rows {
        let derived = row.ns * row.ratio;
        let ratio = row.charged as f64 / derived;
        let verdict = if ratio > 1.25 {
            "  over"
        } else if ratio < 0.80 {
            "  under"
        } else {
            ""
        };
        println!(
            "  {:<26} {:>11} {:>12.1} {:>13.0} {:>8.2}x  {:<10}{}",
            row.name, row.charged, row.ns, derived, ratio, row.reference, verdict
        );
    }
    println!();
    println!("  derived gas = measured ns x that row's class-matched ratio. Above 1.00x means");
    println!("  the precompile charges more than the arithmetic costs on this machine; below");
    println!("  0.80x, less. Timing only: constant-time and side-channel requirements are not");
    println!("  priced, and a cheaper figure is a candidate for repricing rather than an");
    println!("  instruction to reprice.");
    println!();
    println!("  CAVEAT — compare these numbers only within one build configuration. Which");
    println!("  crypto backend ECRecover uses is decided by Cargo feature unification:");
    println!("  `op-revm` enables `revm/secp256k1` by default, so a workspace-wide build links");
    println!("  libsecp256k1 while `-p seismic-revm` alone falls back to the pure-Rust `k256`");
    println!("  and reports ECRecover several times slower. Pin the feature set to the one");
    println!("  production runs before treating any figure here as a price.");
    println!();
}

// ---------------------------------------------------------------------------
// Criterion groups
// ---------------------------------------------------------------------------

fn bench_references(c: &mut Criterion) {
    let i = inputs();
    let mut g = c.benchmark_group("precompile_pricing/reference");
    g.bench_function("ecrecover", |b| {
        b.iter(|| ec_recover_run(black_box(&i.ecrecover), black_box(GAS_LIMIT)))
    });
    g.bench_function("sha256_32B", |b| {
        b.iter(|| sha256_run(black_box(&i.sha256), black_box(GAS_LIMIT)))
    });
    g.finish();
}

fn bench_seismic(c: &mut Criterion) {
    let i = inputs();
    let mut g = c.benchmark_group("precompile_pricing/seismic");

    g.bench_function("secp256k1_sign", |b| {
        b.iter(|| secp256k1_sign_ecdsa_recoverable(black_box(&i.sign), black_box(GAS_LIMIT)))
    });
    g.bench_function("ecdh_derive_sym_key", |b| {
        b.iter(|| derive_symmetric_key(black_box(&i.ecdh), black_box(GAS_LIMIT)))
    });
    g.bench_function("hkdf_32B", |b| {
        b.iter(|| hkdf_derive_symmetric_key(black_box(&i.hkdf_short), black_box(GAS_LIMIT)))
    });
    g.bench_function("hkdf_256B", |b| {
        b.iter(|| hkdf_derive_symmetric_key(black_box(&i.hkdf_long), black_box(GAS_LIMIT)))
    });
    g.bench_function("aes_gcm_enc_16B", |b| {
        b.iter(|| precompile_encrypt(black_box(&i.enc_small), black_box(GAS_LIMIT)))
    });
    g.bench_function("aes_gcm_enc_1KiB", |b| {
        b.iter(|| precompile_encrypt(black_box(&i.enc_large), black_box(GAS_LIMIT)))
    });
    g.bench_function("aes_gcm_dec_16B", |b| {
        b.iter(|| precompile_decrypt(black_box(&i.dec_small), black_box(GAS_LIMIT)))
    });
    g.bench_function("aes_gcm_dec_1KiB", |b| {
        b.iter(|| precompile_decrypt(black_box(&i.dec_large), black_box(GAS_LIMIT)))
    });
    g.finish();
}

fn all(c: &mut Criterion) {
    report();
    bench_references(c);
    bench_seismic(c);
}

criterion_group!(groups, all);
criterion_main!(groups);
