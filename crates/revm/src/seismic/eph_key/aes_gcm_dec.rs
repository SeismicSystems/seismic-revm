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
use crate::primitives::hex;

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
    println!("input: {:?}", hex::encode(input));
    let aes_key = Key::<Aes256Gcm>::from_slice(&input[0..32]);
    println!("aes_key: {:?}", hex::encode(aes_key));
    let nonce_bytes: [u8; 8] = input[56..64].try_into().unwrap(); // Interpret bytes as a big-endian `u64`
    println!("nonce_bytes: {:?}", hex::encode(nonce_bytes));
    let nonce_be: u64 = u64::from_be_bytes(nonce_bytes);
    let ciphertext = input[64..].to_vec();
    println!("ciphertext: {:?}", hex::encode(ciphertext.clone()));
    // decrypt the ciphertext
    let plaintext = match aes_decrypt(&aes_key, &ciphertext, nonce_be) {
        Ok(result) => result,
        Err(e) => {
                let err_msg = e.to_string();
                println!("Error during decryption: {}", err_msg);
                return Err(PCError::Other(err_msg).into());
            }
        };

    println!("plaintext: {:?}", hex::encode(plaintext.clone()));
    // prepare the output
    let output: Bytes = Bytes::from(plaintext);
    println!("output: {:?}", hex::encode(output.clone()));
    Ok(PrecompileOutput::new(gas_limit, output))
} 
