use crate::{
    primitives::{db::Database, Address, Bytes}, seismic::rng, ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext
};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message, SECP256K1,
    generate_keypair,
    SecretKey,
    PublicKey,
    ecdh::SharedSecret,
};
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, PrecompileError, PrecompileOutput, PrecompileResult,
};
use crate::seismic::rng::precompile::get_leaf_rng;
use crate::precompile::Error as PCError;
use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, AeadCore, KeyInit, OsRng as AesRng},
    Aes256Gcm, 
    Key
};
use tee_service_api::{aes_decrypt, aes_encrypt, derive_aes_key};




pub fn encrypt_event(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    // Process the input
    // if input.len() != INPUT_LENGTH {
    //     return Err(Error::Blake2WrongLength.into());
    // }
    let aes_key = Key::<Aes256Gcm>::from_slice(&input[0..32]);
    // Interpret bytes as a big-endian `u64`
    let nonce_bytes: [u8; 8] = input[0..32].try_into().unwrap();
    let nonce_be: u64 = u64::from_be_bytes(nonce_bytes);
    let plaintext = input[40..].to_vec();
 
    // encrypt the plaintext
    let ciphertext = aes_encrypt(&aes_key, &plaintext, nonce_be).unwrap(); // TODO: no unwraps
    
    // prepare the output: (nonce, ciphertext + authtag)
    
    Ok(PrecompileOutput::new(gas_limit, ciphertext.into()))
}