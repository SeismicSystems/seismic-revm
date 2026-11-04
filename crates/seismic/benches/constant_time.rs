//! Benchmarks comparing upstream (short-circuiting) vs constant-time comparison opcodes.
//!
//! Run with: cargo bench -p seismic-revm --bench constant_time
//!
//! Three input classes are tested for each opcode:
//!   - **equal**: identical values (worst case for short-circuit — must check all limbs)
//!   - **diff_first_limb**: differ in the most-significant limb (best case for short-circuit)
//!   - **diff_last_limb**: differ only in the least-significant limb

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use revm::primitives::U256;

// ---------------------------------------------------------------------------
// Upstream (short-circuiting) comparison functions
// ---------------------------------------------------------------------------

mod upstream {
    use core::cmp::Ordering;
    use revm::primitives::U256;

    pub(crate) fn eq(a: &U256, b: &U256) -> bool {
        a == b
    }

    pub(crate) fn lt(a: &U256, b: &U256) -> bool {
        a < b
    }

    fn i256_sign_raw(val: &U256) -> i8 {
        if val.bit(U256::BITS - 1) {
            -1
        } else if val.is_zero() {
            0
        } else {
            1
        }
    }

    pub(crate) fn slt(a: &U256, b: &U256) -> bool {
        let sa = i256_sign_raw(a);
        let sb = i256_sign_raw(b);
        match sa.cmp(&sb) {
            Ordering::Equal => a.cmp(b) == Ordering::Less,
            Ordering::Less => true,
            Ordering::Greater => false,
        }
    }

    pub(crate) fn iszero(a: &U256) -> bool {
        a.is_zero()
    }
}

// ---------------------------------------------------------------------------
// Constant-time comparison functions (same as in constant_time.rs)
// ---------------------------------------------------------------------------

mod ct {
    use revm::primitives::U256;

    #[inline]
    pub(crate) fn eq(a: &U256, b: &U256) -> bool {
        let al = a.as_limbs();
        let bl = b.as_limbs();
        let d0 = core::hint::black_box(al[0] ^ bl[0]);
        let d1 = core::hint::black_box(al[1] ^ bl[1]);
        let d2 = core::hint::black_box(al[2] ^ bl[2]);
        let d3 = core::hint::black_box(al[3] ^ bl[3]);
        (d0 | d1 | d2 | d3) == 0
    }

    #[inline]
    fn lt_limbs(al: &[u64; 4], bl: &[u64; 4]) -> bool {
        let mut borrow: u64 = 0;
        for i in 0..4 {
            let wide = core::hint::black_box(
                (al[i] as u128)
                    .wrapping_sub(bl[i] as u128)
                    .wrapping_sub(borrow as u128),
            );
            borrow = (wide >> 127) as u64;
        }
        borrow != 0
    }

    #[inline]
    pub(crate) fn lt(a: &U256, b: &U256) -> bool {
        lt_limbs(a.as_limbs(), b.as_limbs())
    }

    #[inline]
    pub(crate) fn slt(a: &U256, b: &U256) -> bool {
        let al = a.as_limbs();
        let bl = b.as_limbs();
        let a_flipped = [al[0], al[1], al[2], al[3] ^ 0x8000_0000_0000_0000];
        let b_flipped = [bl[0], bl[1], bl[2], bl[3] ^ 0x8000_0000_0000_0000];
        lt_limbs(&a_flipped, &b_flipped)
    }

    #[inline]
    pub(crate) fn iszero(a: &U256) -> bool {
        let al = a.as_limbs();
        let d = core::hint::black_box(al[0] | al[1] | al[2] | al[3]);
        d == 0
    }
}

// ---------------------------------------------------------------------------
// Test inputs
// ---------------------------------------------------------------------------

/// Two equal values (worst case for short-circuit, best case for CT since both
/// do the same amount of work).
fn equal_pair() -> (U256, U256) {
    let v = U256::from_limbs([0xDEAD, 0xBEEF, 0xCAFE, 0xFACE]);
    (v, v)
}

/// Differ in the most-significant limb (limb 3, checked first by short-circuit Ord).
/// Best case for upstream LT/GT/SLT — the short-circuit exits after 1 limb.
fn diff_first_limb() -> (U256, U256) {
    (
        U256::from_limbs([0xDEAD, 0xBEEF, 0xCAFE, 0x0001]),
        U256::from_limbs([0xDEAD, 0xBEEF, 0xCAFE, 0x0002]),
    )
}

/// Differ only in the least-significant limb (limb 0, checked last by short-circuit Ord).
/// Worst case for upstream LT/GT — the short-circuit must check all 4 limbs.
fn diff_last_limb() -> (U256, U256) {
    (
        U256::from_limbs([0x0001, 0xBEEF, 0xCAFE, 0xFACE]),
        U256::from_limbs([0x0002, 0xBEEF, 0xCAFE, 0xFACE]),
    )
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_eq(c: &mut Criterion) {
    let mut g = c.benchmark_group("eq");

    let (a, b) = equal_pair();
    g.bench_function("upstream/equal", |bench| {
        bench.iter(|| upstream::eq(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/equal", |bench| {
        bench.iter(|| ct::eq(black_box(&a), black_box(&b)))
    });

    let (a, b) = diff_first_limb();
    g.bench_function("upstream/diff_msb", |bench| {
        bench.iter(|| upstream::eq(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/diff_msb", |bench| {
        bench.iter(|| ct::eq(black_box(&a), black_box(&b)))
    });

    let (a, b) = diff_last_limb();
    g.bench_function("upstream/diff_lsb", |bench| {
        bench.iter(|| upstream::eq(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/diff_lsb", |bench| {
        bench.iter(|| ct::eq(black_box(&a), black_box(&b)))
    });

    g.finish();
}

fn bench_lt(c: &mut Criterion) {
    let mut g = c.benchmark_group("lt");

    let (a, b) = equal_pair();
    g.bench_function("upstream/equal", |bench| {
        bench.iter(|| upstream::lt(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/equal", |bench| {
        bench.iter(|| ct::lt(black_box(&a), black_box(&b)))
    });

    let (a, b) = diff_first_limb();
    g.bench_function("upstream/diff_msb", |bench| {
        bench.iter(|| upstream::lt(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/diff_msb", |bench| {
        bench.iter(|| ct::lt(black_box(&a), black_box(&b)))
    });

    let (a, b) = diff_last_limb();
    g.bench_function("upstream/diff_lsb", |bench| {
        bench.iter(|| upstream::lt(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/diff_lsb", |bench| {
        bench.iter(|| ct::lt(black_box(&a), black_box(&b)))
    });

    g.finish();
}

fn bench_slt(c: &mut Criterion) {
    let mut g = c.benchmark_group("slt");

    let (a, b) = equal_pair();
    g.bench_function("upstream/equal", |bench| {
        bench.iter(|| upstream::slt(black_box(&a), black_box(&b)))
    });
    g.bench_function("ct/equal", |bench| {
        bench.iter(|| ct::slt(black_box(&a), black_box(&b)))
    });

    // Signed: positive vs negative (different signs — upstream short-circuits immediately)
    let pos = U256::from_limbs([1, 0, 0, 0]);
    let neg = U256::from_limbs([0, 0, 0, 0x8000_0000_0000_0000]); // min negative
    g.bench_function("upstream/diff_sign", |bench| {
        bench.iter(|| upstream::slt(black_box(&pos), black_box(&neg)))
    });
    g.bench_function("ct/diff_sign", |bench| {
        bench.iter(|| ct::slt(black_box(&pos), black_box(&neg)))
    });

    g.finish();
}

fn bench_iszero(c: &mut Criterion) {
    let mut g = c.benchmark_group("iszero");

    let zero = U256::ZERO;
    g.bench_function("upstream/zero", |bench| {
        bench.iter(|| upstream::iszero(black_box(&zero)))
    });
    g.bench_function("ct/zero", |bench| {
        bench.iter(|| ct::iszero(black_box(&zero)))
    });

    let nonzero = U256::from_limbs([0, 0, 0, 1]); // only MSB limb is nonzero
    g.bench_function("upstream/nonzero_msb", |bench| {
        bench.iter(|| upstream::iszero(black_box(&nonzero)))
    });
    g.bench_function("ct/nonzero_msb", |bench| {
        bench.iter(|| ct::iszero(black_box(&nonzero)))
    });

    let nonzero_lsb = U256::from_limbs([1, 0, 0, 0]); // only LSB limb is nonzero
    g.bench_function("upstream/nonzero_lsb", |bench| {
        bench.iter(|| upstream::iszero(black_box(&nonzero_lsb)))
    });
    g.bench_function("ct/nonzero_lsb", |bench| {
        bench.iter(|| ct::iszero(black_box(&nonzero_lsb)))
    });

    g.finish();
}

criterion_group!(benches, bench_eq, bench_lt, bench_slt, bench_iszero);
criterion_main!(benches);
