//! Handler related to Seismic chain

use crate::{
    handler::register::EvmHandler,
    primitives::{
        db::Database, spec_to_generic, Spec, SpecId
    },
    ContextPrecompiles,
    seismic::rng::RNG,
};
use revm_precompile::{secp256r1, PrecompileSpecId};
use std::sync::Arc;

pub fn seismic_handle_register<DB: Database, EXT>(handler: &mut EvmHandler<'_, EXT, DB>) {
    spec_to_generic!(handler.cfg.spec_id, {
        handler.pre_execution.load_precompiles = Arc::new(load_precompiles::<SPEC, EXT, DB>);
    });
}

// Load precompiles for Seismic chain.
#[inline]
pub fn load_precompiles<SPEC: Spec, EXT, DB: Database>() -> ContextPrecompiles<DB> {
    let mut precompiles = ContextPrecompiles::new(PrecompileSpecId::from_spec_id(SPEC::SPEC_ID));

    if SPEC::enabled(SpecId::MERCURY) {
        precompiles.extend([
            // EIP-7212: secp256r1 P256verify
            secp256r1::P256VERIFY,
            RNG
        ])
    }
    precompiles
}

