use crate::primitives::{Address, Bytes};
use crate::precompile::Error as PCError;
use revm_precompile::{
    u64_to_address, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress, Precompile, Error as REVM_ERROR
};
use aes_gcm::{
    Aes256Gcm, 
    Key
};
use tee_service_api::aes_decrypt;

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(precompile_decrypt));
   

pub const ADDRESS: Address = u64_to_address(104);
pub const MIN_INPUT_LENGTH: usize = 64;

/// Decrypts a ciphertext using AES-256 GCM
/// The input is a concatenation of the AES key, nonce, and ciphertext
/// returns the decrypted plaintext
pub fn precompile_decrypt(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    let gas_used = 1; // TODO: refine this constant. Should scale with input size
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }
    // Process the input
    if input.len() <= MIN_INPUT_LENGTH {
        let err_msg = format!(
            "invalid input length. Must be at least {}, was {}",
            MIN_INPUT_LENGTH,
            input.len()
        );
        return Err(PCError::Other(err_msg).into());
    }

    let aes_key = Key::<Aes256Gcm>::from_slice(&input[0..32]);
    let nonce_bytes: [u8; 8] = input[32..40].try_into().unwrap(); // Interpret bytes as a big-endian `u64`
    let nonce_be: u64 = u64::from_be_bytes(nonce_bytes);
    let ciphertext = input[40..].to_vec();
    let plaintext = aes_decrypt(&aes_key, &ciphertext, nonce_be).map_err(|e| PCError::Other(e.to_string()))?;
    let output: Bytes = Bytes::from(plaintext);
    Ok(PrecompileOutput::new(gas_limit, output))
} 
