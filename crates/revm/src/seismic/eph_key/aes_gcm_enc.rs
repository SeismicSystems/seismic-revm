use crate:: primitives::{Address, Bytes};
use crate::precompile::Error as PCError;
use revm_precompile::{
    u64_to_address, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress, Precompile, Error as REVM_ERROR
};
use aes_gcm::{
    Aes256Gcm, 
    Key
};
use tee_service_api::aes_encrypt;

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(precompile_encrypt));
   

pub const ADDRESS: Address = u64_to_address(103);
pub const MIN_INPUT_LENGTH: usize = 40;


pub fn precompile_encrypt(input: &Bytes, gas_limit: u64) -> PrecompileResult {
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
    let nonce_bytes: [u8; 8] = input[32..40].try_into().unwrap();     // Interpret bytes as a big-endian `u64`
    let nonce_be: u64 = u64::from_be_bytes(nonce_bytes);   // TODO: nonce comes from the contract instead of the input
    let plaintext = input[40..].to_vec();

    // encrypt the plaintext
    let ciphertext = aes_encrypt(&aes_key, &plaintext, nonce_be).unwrap(); // TODO: no unwraps

    // prepare the output: (nonce, ciphertext + authtag)
    let output: Bytes = Bytes::from(nonce_bytes.to_vec().into_iter().chain(ciphertext.into_iter()).collect::<Vec<u8>>());
    println!("output: {:?}", output);

    Ok(PrecompileOutput::new(gas_limit, output))
}