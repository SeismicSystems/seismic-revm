//! A caller's extra precompiles must not leak into everybody else's precompile set.
//!
//! Ordering matters here, and the cache under test is process-wide, so this scenario needs a
//! test binary of its own: `mercury_with_extra(Some(..))` runs *before* any plain `mercury()`.

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

const CUSTOM_ADDRESS: u64 = 0x5101;

fn echo(input: &[u8], _gas_limit: u64) -> PrecompileResult {
    Ok(PrecompileOutput::new(0, Bytes::copy_from_slice(input)))
}

const CUSTOM: Precompile = Precompile::new(
    PrecompileId::Custom(Cow::Borrowed("test_isolation")),
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

/// Building a set *with* extras must not change what plain `mercury()` returns afterwards.
///
/// Before the fix both calls shared one `OnceBox` whose initializer captured the `extra`
/// argument. Whoever called first decided the contents for the entire process, so this
/// ordering handed the custom precompile to every ordinary EVM — `set_spec` calls `mercury()`
/// before every transaction.
#[test]
fn plain_mercury_does_not_inherit_another_callers_extras() {
    let with_extra = mercury_with_extra::<Ctx>(Some(extra_set())).0;
    assert!(
        with_extra.contains(&u64_to_address(CUSTOM_ADDRESS)),
        "precondition: extras are honored when requested"
    );

    let plain = mercury::<Ctx>().0;
    assert!(
        !plain.contains(&u64_to_address(CUSTOM_ADDRESS)),
        "mercury() must not serve another caller's extra precompiles"
    );

    // The stock set is still intact.
    assert!(
        plain.contains(&u64_to_address(101)),
        "mercury() must still contain the Seismic precompiles"
    );
}
