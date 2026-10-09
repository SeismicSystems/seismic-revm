//! Extras must be honored even when the plain precompile set was built first.
//!
//! This is the opposite ordering to `mercury_extra_isolation.rs`, and because the cache under
//! test is process-wide it needs its own test binary: a plain `mercury()` runs *before*
//! `mercury_with_extra(Some(..))`.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use revm::{
    database::EmptyDB,
    precompile::{
        u64_to_address, Precompile, PrecompileId, PrecompileOutput, PrecompileResult, Precompiles,
    },
    primitives::Bytes,
};
use seismic_revm::{
    precompiles::{mercury, mercury_with_extra},
    SeismicContext,
};
use std::{borrow::Cow, sync::OnceLock};

type Ctx = SeismicContext<EmptyDB>;

const CUSTOM_ADDRESS: u64 = 0x5201;

fn echo(input: &[u8], _gas_limit: u64) -> PrecompileResult {
    Ok(PrecompileOutput::new(0, Bytes::copy_from_slice(input)))
}

const CUSTOM: Precompile = Precompile::new(
    PrecompileId::Custom(Cow::Borrowed("test_after_plain")),
    u64_to_address(CUSTOM_ADDRESS),
    echo,
);

fn extra_set() -> &'static Precompiles {
    static EXTRA: OnceLock<Precompiles> = OnceLock::new();
    EXTRA.get_or_init(|| {
        let mut precompiles = Precompiles::prague().clone();
        precompiles.extend([CUSTOM]);
        precompiles
    })
}

/// Requesting extras must work regardless of what was built earlier in the process.
///
/// Before the fix the first call — here the plain one — initialized the shared cache, and
/// every later `Some(extra)` silently got that cached set back with its own extras dropped.
#[test]
fn extras_are_honored_after_the_plain_set_was_built() {
    let plain = mercury::<Ctx>().0;
    assert!(
        !plain.contains(&u64_to_address(CUSTOM_ADDRESS)),
        "precondition: the plain set has no custom precompile"
    );

    let with_extra = mercury_with_extra::<Ctx>(Some(extra_set())).0;
    assert!(
        with_extra.contains(&u64_to_address(CUSTOM_ADDRESS)),
        "extras must be honored even when the plain set was built first"
    );

    // Extending must not drop the stock precompiles.
    assert!(
        with_extra.contains(&u64_to_address(101)),
        "the Seismic precompiles must survive the extension"
    );
}

/// Repeated calls with the same `extra` must reuse one set instead of leaking a new one.
#[test]
fn repeated_calls_with_the_same_extra_are_memoized() {
    let first = mercury_with_extra::<Ctx>(Some(extra_set())).0;
    let second = mercury_with_extra::<Ctx>(Some(extra_set())).0;
    assert!(
        core::ptr::eq(first, second),
        "the same extra set must map to the same cached precompile set"
    );
}
