use crate:: primitives::{Address, Bytes};
use secp256k1::{
    SecretKey,
    PublicKey,
    ecdh::SharedSecret,
};
use revm_precompile::{
    u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress, Precompile, Error as REVM_ERROR,
};
use tee_service_api::derive_aes_key;
use crate::precompile::Error as PCError;

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(derive_symmetric_key));

pub const ADDRESS: Address = u64_to_address(102);
pub const INPUT_LENGTH: usize = 64;

/// Derives an AES symmetric key from a secp256k1 secret key and a secp256k1 public key.
/// The input is a concatenation of the secret key and the public key.
/// The output is the AES key.
pub fn derive_symmetric_key(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    // Process the input
    if input.len() != INPUT_LENGTH {
        let err_msg = format!(
            "invalid input length. Should be {}, was {}",
            INPUT_LENGTH,
            input.len()
        );
        return Err(PCError::Other(err_msg).into());
    }

    let sk_bytes = &input[0..32];
    let pk_bytes = &input[32..64];
    let sk = SecretKey::from_slice(sk_bytes).map_err(|_| PrecompileError::Other("invalid sk".to_string()))?;
    let pk = PublicKey::from_slice(pk_bytes).map_err(|_| PrecompileError::Other("invalid pk".to_string()))?;

    // derive the shared secret
    let shared_secret = SharedSecret::new( &pk, &sk);
    // derive the AES key
    let aes_key = derive_aes_key(&shared_secret).unwrap(); // TODO: no unwraps 
    let output = aes_key.to_vec(); // TODO: coerce this to be a specific size
    println!("output len {}", output.len());

    Ok(PrecompileOutput::new(gas_limit, output.into()))
}