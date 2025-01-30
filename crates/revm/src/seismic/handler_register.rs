//! Handler related to Seismic chain

use super::precompiles::{
    aes::{aes_gcm_dec, aes_gcm_enc},
    ecdh_derive_sym_key, hkdf_derive_sym_key, rng,
};
use crate::{
    handler::register::EvmHandler, primitives::{db::Database, spec_to_generic, Spec, SpecId, EVMError}, Context, ContextPrecompiles
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

#[inline]
pub fn reset_seismic_rng<SPEC: Spec, EXT, DB: Database>(
     context: &mut Context<EXT, DB>,
    ) -> Result<(), EVMError<DB::Error>> {
     let secret_key = context.evm.kernel.get_eph_rng_keypair();
     context.evm.kernel.reset_root_rng(secret_key);
     Ok(())
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use revm_precompile::u64_to_address;

    use super::*;
    use crate::{
        db::{CacheDB, EmptyDBTyped},
        primitives::{
            hex, Address, U256
        }, Evm,
    };

    #[test]
    fn test_rng_resets() {
        // 1) Set up a fresh EVM with an empty DB for testing.
        let db: CacheDB::<EmptyDBTyped<Infallible>> = CacheDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .append_handler_register(seismic_handle_register)
            .build();

        // 2) Transaction #1 in block #1
        //    We'll call the precompile at address 0x64, with input specifying output size = 32.
        evm.context.evm.env.block.number = U256::from(1); 
        evm.context.evm.env.tx.caller = Address::ZERO;
        evm.context.evm.env.tx.transact_to = u64_to_address(100).into();
        evm.context.evm.env.tx.data = 32u32.to_be_bytes().to_vec().into(); 
        evm.context.evm.env.tx.value = U256::ZERO;
        evm.context.evm.env.tx.gas_limit = 1_000_000; 
        // Execute the transaction. Even though it's a "staticcall," in REVM we just do
        // a normal call with no writes (value=0, no state changes).
        let res1 = evm.transact_commit().expect("tx1 failed");
        let out1 = res1.output().unwrap();
        println!("Block #1 precompile output: 0x{}", hex::encode(&out1));

        // 3) Transaction #2 in block #2
        evm.context.evm.env.block.number = U256::from(2); // simulate second block
        // Keep same caller, same precompile address, same input
        evm.context.evm.env.tx.data = 32u32.to_be_bytes().to_vec().into();

        let res2 = evm.transact_commit().expect("tx2 failed");
        let out2 = res2.output().unwrap();
        println!("Block #2 precompile output: 0x{}", hex::encode(&out2));

        // 4) Check outputs. If your RNG depends on block number, they’ll likely differ:
        //    If you want them the same, tweak your precompile logic to ignore block context.
        assert_ne!(
            out1, out2,
            "Expected different RNG outputs across blocks, but got the same"
        );
    }
}
