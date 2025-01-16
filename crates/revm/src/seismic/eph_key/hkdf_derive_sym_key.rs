use crate::primitives::{Address, Bytes};
use hkdf::Hkdf;
use revm_precompile::{
    u64_to_address, Precompile, PrecompileOutput, PrecompileResult, PrecompileWithAddress, PrecompileError
};
use sha2::Sha256;

/// The deployed address of this precompile.
pub const ADDRESS: Address = u64_to_address(105);

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(hkdf_derive_symmetric_key));


/// Ceiling gas limit at 100k, to be reviewed. 
const HKDF_DERIVE_SYM_KEY_GAS: u64 = 100_000;

/// HKDF-based AES symmetric key derivation.
/// This function uses HKDF-SHA256 on the provided input (which should be a high-entropy
/// seed) to derive a 32-byte key suitable for AES-256 use.
///
/// # Gas
/// No specific gas checks are done here; the returned `gas_used` equals whatever gas limit
/// the caller passed in.
///
/// # Errors
/// Returns an error if HKDF expansion fails (unlikely, but we handle it anyway).
///
/// # Arguments
/// * `input`: The raw bytes used as HKDF input material.
/// * `gas_limit`: The gas limit provided by the EVM. This is returned as-is in the output.
///
/// # Returns
/// A `PrecompileResult` whose `bytes` field is a 32-byte HKDF expansion.
pub fn hkdf_derive_symmetric_key(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    if HKDF_DERIVE_SYM_KEY_GAS > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    // TODO: linear gas cost increases with input length

    // TODO: Use similar construct in opther precompile (extract and expand here for ex, or XOR, etc) to derive the cost!
    
    let hkdf = Hkdf::<Sha256>::new(None, input);

    let mut okm = [0u8; 32];
    hkdf.expand(b"Seismic: aes-gcm key", &mut okm)
        .map_err(|_| PrecompileError::Other("HKDF expansion error".into()))?;

    Ok(PrecompileOutput::new(gas_limit, okm.to_vec().into()))
}
