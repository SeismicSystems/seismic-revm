//! Handler related to Seismic chain

use super::precompiles::{
    aes::{aes_gcm_dec, aes_gcm_enc},
    ecdh_derive_sym_key, hkdf_derive_sym_key, rng, secp256k1_sign,
};
use crate::{
    handler::register::EvmHandler,
    primitives::{db::Database, spec_to_generic, EVMError, Spec, SpecId},
    Context, ContextPrecompiles,
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
            secp256k1_sign::PRECOMPILE,
        ]);
        // extend with ContextPrecompile<DB>
        precompiles.extend([rng::RngPrecompile::address_and_precompile::<DB>()]);
    }
    precompiles
}

#[inline]
pub fn reset_seismic_rng<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
) -> Result<(), EVMError<DB::Error>> {
    context.evm.kernel.reset_rng();
    Ok(())
}

#[cfg(test)]
mod tests {
    use revm_precompile::u64_to_address;
    use std::convert::Infallible;

    use super::*;
    use crate::{
        db::{CacheDB, EmptyDBTyped},
        primitives::{Address, U256},
        Evm,
    };

    #[test]
    fn test_rng_resets() {
        let db: CacheDB<EmptyDBTyped<Infallible>> = CacheDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .append_handler_register(seismic_handle_register)
            .build();

        evm.context.evm.env.block.number = U256::from(1);
        evm.context.evm.env.tx.caller = Address::ZERO;
        evm.context.evm.env.tx.transact_to = u64_to_address(100).into();
        evm.context.evm.env.tx.data = 32u32.to_be_bytes().to_vec().into();
        evm.context.evm.env.tx.value = U256::ZERO;
        evm.context.evm.env.tx.gas_limit = 1_000_000;
        let res1 = evm.transact_commit().expect("tx1 failed");
        let out1 = res1.output().unwrap();

        evm.context.evm.env.block.number = U256::from(2); // simulate second block
        evm.context.evm.env.tx.data = 32u32.to_be_bytes().to_vec().into();
        let res2 = evm.transact_commit().expect("tx2 failed");
        let out2 = res2.output().unwrap();

        assert_eq!(
            out1, out2,
            "Given equal transaction hash and entropy, should get same output"
        );
    }
}
