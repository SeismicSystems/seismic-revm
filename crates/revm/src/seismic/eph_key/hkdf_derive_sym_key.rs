use crate::primitives::{Address, Bytes};
use hkdf::Hkdf;
use revm_precompile::{
    u64_to_address, Precompile, PrecompileOutput, PrecompileResult, PrecompileWithAddress,
};
use sha2::Sha256;

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(hkdf_derive_symmetric_key));

pub const ADDRESS: Address = u64_to_address(105);
pub const INPUT_LENGTH: usize = 65;

/// Derives an Bytes for a AES symmetric key from a
/// HMAC-based key derivation function (HKDF)
/// The input should be a high entropy string to ensure the key is not predictable.
/// The output is 32 bytes
pub fn hkdf_derive_symmetric_key(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    // Initialize HKDF with SHA-256
    let hk = Hkdf::<Sha256>::new(None, &input);

    // Output a 32-byte key for AES-256
    let mut okm: [u8; 32] = [0u8; 32];
    hk.expand(b"aes-gcm key", &mut okm).unwrap(); // TODO: error handling

    Ok(PrecompileOutput::new(gas_limit, okm.into()))
}
