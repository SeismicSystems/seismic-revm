use crate::{
    primitives::{db::Database, Address, Bytes},
    ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext,
};
use core::hash;
use std::sync::Arc;

use rand_core::RngCore;
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, PrecompileError, PrecompileOutput, PrecompileResult,
};

use super::env_hash::hash_tx_env;

pub struct RngPrecompile;

impl RngPrecompile {
    pub fn address_and_precompile<DB: Database>() -> (Address, ContextPrecompile<DB>) {
        (
            u64_to_address(100),
            ContextPrecompile::ContextStateful(Arc::new(RngPrecompile)),
        )
    }
}

impl<DB: Database> ContextStatefulPrecompile<DB> for RngPrecompile {
    fn call(
        &self,
        input: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        let gas_used = 100; // TODO: refine this constant
        if gas_used > gas_limit {
            return Err(REVM_ERROR::OutOfGas.into());
        }

        let pers = input.as_ref(); // pers is the personalized entropy added by the caller

        // Get the random bytes
        // TODO: evaluate if this is good, ex if the tx_hash is correct
        let env = evmctx.env().clone();
        let root_rng = &mut evmctx.kernel.root_rng;
        let tx_hash = hash_tx_env(&env.tx);
        root_rng.append_tx(tx_hash);
        let mut leaf_rng = match root_rng.fork(&env, pers) {
            Ok(rng) => rng,
            Err(_err) => {
                return Err(PrecompileError::Other("Rng fork failed".to_string()).into());
            }
        };

        let mut rng_bytes = [0u8; 32];
        leaf_rng.fill_bytes(&mut rng_bytes);
        let output = Bytes::from(rng_bytes);

        Ok(PrecompileOutput::new(gas_used, output))
    }
}
