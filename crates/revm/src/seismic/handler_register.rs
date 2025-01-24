//! Handler related to Seismic chain

use super::precompiles::{
    aes::{aes_gcm_dec, aes_gcm_enc},
    ecdh_derive_sym_key, hkdf_derive_sym_key, rng
};
use crate::{
    handler::register::EvmHandler,
    primitives::{db::Database, spec_to_generic, Spec, SpecId},
    ContextPrecompiles
};
use revm_precompile::{secp256r1, PrecompileSpecId};
use std::sync::Arc;

pub fn seismic_handle_register<DB: Database, EXT>(handler: &mut EvmHandler<'_, EXT, DB>) {
    spec_to_generic!(handler.cfg.spec_id, {
        handler.pre_execution.load_precompiles = Arc::new(load_precompiles::<SPEC, EXT, DB>);
    });
}

#[inline]
pub fn load_precompiles<SPEC: Spec, EXT, DB: Database>() -> ContextPrecompiles<DB> {
    let mut precompiles = ContextPrecompiles::new(PrecompileSpecId::from_spec_id(SPEC::SPEC_ID));

    if SPEC::enabled(SpecId::MERCURY) {
        // extend with PrecompileWithAddress
        precompiles.extend([
            secp256r1::P256VERIFY,
            ecdh_derive_sym_key::PRECOMPILE,
            hkdf_derive_sym_key::PRECOMPILE,
            aes_gcm_enc::PRECOMPILE,
            aes_gcm_dec::PRECOMPILE,
        ]);
        // extend with ContextPrecompile<DB>
        precompiles.extend([rng::RngPrecompile::address_and_precompile::<DB>()]);
    }
    precompiles
}
