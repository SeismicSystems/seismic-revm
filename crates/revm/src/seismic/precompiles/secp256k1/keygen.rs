use crate::primitives::Bytes;
use crate::seismic::precompiles::SECP256K1_VALIDATE_SK_ADDRESS;


use revm_precompile::{
    calc_linear_cost_u32, Precompile, PrecompileError, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};


/* --------------------------------------------------------------------------
Precompile Wiring
-------------------------------------------------------------------------- */

pub const PRECOMPILE: PrecompileWithAddress = PrecompileWithAddress(
    SECP256K1_VALIDATE_SK_ADDRESS,
    Precompile::Standard(secp256k1_validate_sk),
);

/* --------------------------------------------------------------------------
 HKDF with Gas Calculation
-------------------------------------------------------------------------- */
/// TODO: explain gas reasoning

// validates the secret key is non-zero and within the curve order
pub fn secp256k1_validate_sk(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Bytes;
    use revm_precompile::Error as PrecompileError;

}
