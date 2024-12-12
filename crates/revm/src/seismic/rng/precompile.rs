use super::{context::Context, domain_sep_rng::RootRng};
use crate::primitives::{Bytes, Env};
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, Precompile, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
};
use rand_core::RngCore;

pub const RNG: PrecompileWithAddress =
    PrecompileWithAddress(u64_to_address(100), Precompile::Env(run));

pub fn run(input: &Bytes, gas_limit: u64, _env: &Env) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    const HEADER: [u8; 32] = [
        0xc8, 0xb2, 0x24, 0xc5, 0x80, 0x03, 0xa7, 0x97, 0xc0, 0x06, 0x46, 0x97, 0xdf, 0x57, 0xa4,
        0x20, 0x9b, 0x2b, 0x9c, 0xb5, 0x21, 0x22, 0x86, 0xa9, 0xb1, 0xb1, 0x83, 0x17, 0x63, 0x75,
        0x25, 0x16,
    ];
    let context = Context::new(HEADER); // TODO: recieve context from somewhere else
    let pers = input.as_ref(); // pers is the personalized entropy added by the caller

    // Get the random bytes
    // TODO: Root rng passed in?
    // TODO: Better error handling for fork
    let root_rng = RootRng::new();
    let mut leaf_rng = root_rng
        .fork(context, pers.as_ref())
        .expect("rng fork should work");
    let mut rng_bytes = [0u8; 32];
    leaf_rng.fill_bytes(&mut rng_bytes);
    let output = Bytes::from(rng_bytes);

    Ok(PrecompileOutput::new(gas_used, output))
}