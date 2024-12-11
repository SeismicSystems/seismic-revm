use revm_precompile::{u64_to_address,  Precompile, PrecompileOutput, PrecompileResult, PrecompileWithAddress};
use crate::primitives::{Env, Bytes};

pub const RNG: PrecompileWithAddress=
    PrecompileWithAddress(u64_to_address(100), Precompile::Env(run));

pub fn run(input: &Bytes, gas_limit: u64, env: &Env) -> PrecompileResult {
    Ok(PrecompileOutput::new(10, Bytes::new()))
}
