//! Custom precompiles installed on a [`SeismicPrecompiles`] must stay installed.
//!
//! `anvil` injects precompiles this way (see `crates/anvil/src/evm.rs` in seismic-foundry),
//! looping over a `PrecompileFactory`'s set and calling `apply_precompile` once per entry.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use revm::{
    database::EmptyDB,
    handler::PrecompileProvider,
    precompile::{
        u64_to_address, Precompile, PrecompileId, PrecompileOutput, PrecompileResult, Precompiles,
    },
    primitives::Bytes,
};
use seismic_revm::{precompiles::SeismicPrecompiles, SeismicContext, SeismicSpecId};
use std::borrow::Cow;

type Ctx = SeismicContext<EmptyDB>;

const CUSTOM_A_ADDRESS: u64 = 0x5001;
const CUSTOM_B_ADDRESS: u64 = 0x5002;
const CUSTOM_C_ADDRESS: u64 = 0x5003;

fn echo(input: &[u8], _gas_limit: u64) -> PrecompileResult {
    Ok(PrecompileOutput::new(0, Bytes::copy_from_slice(input)))
}

const CUSTOM_A: Precompile = Precompile::new(
    PrecompileId::Custom(Cow::Borrowed("test_custom_a")),
    u64_to_address(CUSTOM_A_ADDRESS),
    echo,
);

const CUSTOM_B: Precompile = Precompile::new(
    PrecompileId::Custom(Cow::Borrowed("test_custom_b")),
    u64_to_address(CUSTOM_B_ADDRESS),
    echo,
);

const CUSTOM_C: Precompile = Precompile::new(
    PrecompileId::Custom(Cow::Borrowed("test_custom_c")),
    u64_to_address(CUSTOM_C_ADDRESS),
    echo,
);

/// `apply_precompile` must install the precompile it is handed, every time.
///
/// Before the fix a process-wide `OnceLock` memoized the *first* merged set ever built, so
/// the second call in this loop re-applied `CUSTOM_A` and dropped `CUSTOM_B` on the floor.
#[test]
fn apply_precompile_installs_every_precompile() {
    let mut precompiles = SeismicPrecompiles::<Ctx>::new_with_spec(SeismicSpecId::MERCURY);

    for precompile in [CUSTOM_A, CUSTOM_B] {
        precompiles.apply_precompile(precompile);
    }

    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(CUSTOM_A_ADDRESS)),
        "first injected precompile must be present"
    );
    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(CUSTOM_B_ADDRESS)),
        "second injected precompile must be present, not silently replaced by the first"
    );
}

/// The stock Seismic precompiles must survive an injection.
#[test]
fn apply_precompile_keeps_the_builtin_set() {
    let mut precompiles = SeismicPrecompiles::<Ctx>::new_with_spec(SeismicSpecId::MERCURY);
    precompiles.apply_precompile(CUSTOM_A);

    // ECDH lives at 0x65 and is part of the Mercury set.
    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(101)),
        "injecting a precompile must not drop the Seismic precompiles"
    );
    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(1)),
        "injecting a precompile must not drop the Ethereum precompiles"
    );
}

/// `set_spec` runs before *every* transaction (`handler::pre_execution::load_accounts`).
/// An unchanged spec must therefore not rebuild the set, or injected precompiles would be
/// wiped by the first transaction the node executes.
#[test]
fn set_spec_preserves_injected_precompiles() {
    let mut precompiles = SeismicPrecompiles::<Ctx>::new_with_spec(SeismicSpecId::MERCURY);
    precompiles.apply_precompile(CUSTOM_C);
    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(CUSTOM_C_ADDRESS)),
        "precondition: the precompile is installed"
    );

    let changed = PrecompileProvider::<Ctx>::set_spec(&mut precompiles, SeismicSpecId::MERCURY);

    assert!(!changed, "an unchanged spec must report no change");
    assert!(
        PrecompileProvider::<Ctx>::contains(&precompiles, &u64_to_address(CUSTOM_C_ADDRESS)),
        "executing a transaction must not wipe injected precompiles"
    );
}

/// Sanity check that the fixtures above are not accidentally part of the stock set.
#[test]
fn custom_addresses_are_not_builtin() {
    let mercury = seismic_revm::precompiles::mercury::<Ctx>().0;
    for address in [CUSTOM_A_ADDRESS, CUSTOM_B_ADDRESS, CUSTOM_C_ADDRESS] {
        assert!(
            Precompiles::get(mercury, &u64_to_address(address)).is_none(),
            "test fixture address {address:#x} must not collide with a real precompile"
        );
    }
}
