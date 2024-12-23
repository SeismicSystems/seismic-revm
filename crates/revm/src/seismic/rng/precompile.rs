use crate::{
    primitives::{db::Database, Address, Bytes},
    ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext,
};
use std::sync::Arc;
use anyhow::anyhow;

use rand_core::RngCore;
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, PrecompileOutput, PrecompileResult,
};
use crate::precompile::Error as PCError;

use super::{domain_sep_rng::LeafRng, env_hash::hash_tx_env};

pub struct RngPrecompile;

impl RngPrecompile {
    pub fn address_and_precompile<DB: Database>() -> (Address, ContextPrecompile<DB>) {
        (
            ADDRESS,
            ContextPrecompile::ContextStateful(Arc::new(RngPrecompile)),
        )
    }
}

pub const ADDRESS: Address = u64_to_address(100);

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

        // Get the random bytes
        // TODO: evaluate if this is good, ex if the tx_hash is correct
        let mut leaf_rng = get_leaf_rng(input, evmctx).map_err(|e| PCError::Other(e.to_string()))?;

        let mut rng_bytes = [0u8; 32];
        leaf_rng.fill_bytes(&mut rng_bytes);
        let output = Bytes::from(rng_bytes);

        Ok(PrecompileOutput::new(gas_used, output))
    }
}

pub fn get_leaf_rng<DB: Database>(
    input: &Bytes,
    evmctx: &mut InnerEvmContext<DB>,
) -> Result<LeafRng, anyhow::Error> {
    let pers = input.as_ref(); // pers is the personalized entropy added by the caller
    let env = evmctx.env().clone();
    let tx_hash = hash_tx_env(&env.tx);
    let root_rng = &mut evmctx.kernel.root_rng;
    root_rng.append_tx(tx_hash);
    let leaf_rng = match root_rng.fork(&env, pers) {
        Ok(rng) => rng,
        Err(_err) => {
            return Err(anyhow!("Rng fork failed".to_string()));
        }
    };
    Ok(leaf_rng)
}