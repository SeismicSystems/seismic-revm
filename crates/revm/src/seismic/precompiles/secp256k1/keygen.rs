use crate::primitives::Bytes;
use crate::seismic::precompiles::SECP256K1_VALIDATE_SK_ADDRESS;
use crate::precompile::Error as PCError;

use revm_precompile::{
    calc_linear_cost_u32, Precompile, PrecompileError, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};
use secp256k1::Secp256k1;

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
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(PCError::OutOfGas.into());
    }
    if input.len() != 32 {
        return Err(PCError::Other("Invalid input length".to_string()).into());
    }
    let key_bytes: [u8; 32] = input[0..32].try_into().unwrap();

    let secp = Secp256k1::new();
    let secret_key = secp256k1::SecretKey::from_slice(&key_bytes).map_err(
        |e| PCError::Other(format!("Invalid secret key: {e}")),
    )?;
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);


    let sk_bytes = bincode::serialize(&secret_key).unwrap();
    let pk_bytes = bincode::serialize(&public_key).unwrap();
    let output: [u8; 65] = [sk_bytes, pk_bytes].concat().try_into().unwrap();
    Ok(PrecompileOutput::new(gas_used, output.into()))
    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Bytes;
    use revm_precompile::Error as PrecompileError;

}
