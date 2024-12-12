use super::domain_sep_rng::RootRng;
use crate::primitives::{Bytes, Env};

use rand_core::RngCore;
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, Precompile, PrecompileError, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};

pub const RNG_PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(u64_to_address(100), Precompile::Env(run));

pub fn run(input: &Bytes, gas_limit: u64, env: &Env) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    // Get the random bytes
    // TODO: Root rng goes in Env. Appending the TxEnv hash happens at some point
    let root_rng = RootRng::new();
    let pers = input.as_ref(); // pers is the personalized entropy added by the caller
    let mut leaf_rng = match root_rng.fork(env, pers) {
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

