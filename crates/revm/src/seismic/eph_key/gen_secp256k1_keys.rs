use crate::precompile::Error as PCError;
use crate::seismic::rng::precompile::get_leaf_rng;
use crate::{
    primitives::{db::Database, Address, Bytes},
    ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext,
};
use revm_precompile::{u64_to_address, Error as REVM_ERROR, PrecompileOutput, PrecompileResult};
use secp256k1::generate_keypair;
use std::sync::Arc;

pub struct GenSecp256k1KeysPrecompile;

impl GenSecp256k1KeysPrecompile {
    pub fn address_and_precompile<DB: Database>() -> (Address, ContextPrecompile<DB>) {
        (
            ADDRESS,
            ContextPrecompile::ContextStateful(Arc::new(GenSecp256k1KeysPrecompile)),
        )
    }
}

pub const ADDRESS: Address = u64_to_address(101);


//TODO: Below has non-deterministic gas cost. Need to fix before added to evm-specs.
impl<DB: Database> ContextStatefulPrecompile<DB> for GenSecp256k1KeysPrecompile {
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

        let mut leaf_rng =
            get_leaf_rng(input, evmctx).map_err(|e| PCError::Other(e.to_string()))?;

        let (secret_key, public_key) = generate_keypair(&mut leaf_rng);
        let sk_bytes = bincode::serialize(&secret_key).unwrap();
        let pk_bytes = bincode::serialize(&public_key).unwrap();
        let output: [u8; 65] = [sk_bytes, pk_bytes].concat().try_into().unwrap();
        Ok(PrecompileOutput::new(gas_used, output.into()))
    }
}
