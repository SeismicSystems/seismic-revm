use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit as _};
use hkdf::Hkdf;
use revm::precompile::PrecompileError;
use secp256k1::ecdh::SharedSecret;
use sha2::{
    digest::{consts::U12, generic_array::GenericArray},
    Sha256,
};

/// The below gas cost are very rough estimates.
/// Overhead cost for AES-GCM setup & finalization. We intentionally overprice to stay safe.
const AES_GCM_BASE: u64 = 1000;

/// Per 16-byte block cost. One AES encryption + one GHASH multiply per block, plus cushion.
const AES_GCM_PER_BLOCK: u64 = 30;

pub const AESGCM_NONCE_SIZE: usize = 12; // Size of AES-GCM nonce in bytes

/// The intermediate type to represent a nonce in the enclave
#[derive(Debug, Clone)]
pub struct Nonce(pub [u8; AESGCM_NONCE_SIZE]);

impl From<Nonce> for aes_gcm::Nonce<U12> {
    fn from(nonce: Nonce) -> Self {
        GenericArray::clone_from_slice(&nonce.0)
    }
}

impl From<[u8; AESGCM_NONCE_SIZE]> for Nonce {
    fn from(bytes: [u8; AESGCM_NONCE_SIZE]) -> Self {
        Nonce(bytes)
    }
}

pub(crate) fn validate_input_length(
    input_len: usize,
    min_input_length: usize,
) -> Result<(), PrecompileError> {
    if input_len < min_input_length {
        let err_msg = format!(
            "invalid input length: must be >= {min_input_length}, got {}",
            input_len
        );
        return Err(PrecompileError::Other(err_msg));
    }
    Ok(())
}

pub(crate) fn parse_aes_key(slice: &[u8]) -> Result<[u8; 32], PrecompileError> {
    slice
        .try_into()
        .map_err(|_| PrecompileError::Other("invalid key length (must be 32 bytes)".to_string()))
}

pub(crate) fn validate_nonce_length(slice: &[u8]) -> Result<(), PrecompileError> {
    if slice.len() != 12 {
        return Err(PrecompileError::Other(
            "Invalid nonce length: expected 12 bytes".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn calculate_cost(ciphertext_len: usize) -> u64 {
    calc_linear_cost(16, ciphertext_len, AES_GCM_BASE, AES_GCM_PER_BLOCK)
}

fn calc_linear_cost(bus: u64, len: usize, base: u64, word: u64) -> u64 {
    (len as u64).div_ceil(bus) * word + base
}

pub(crate) fn validate_gas_limit(cost: u64, gas_limit: u64) -> Result<(), PrecompileError> {
    if cost > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(())
}

/// Decrypts ciphertext using AES-256 GCM with a 92-bit nonce.
///
/// This function requires the nonce to be exactly 92 bits (12 bytes),
/// with no padding or truncation. The caller must pass a `Vec<u8>`
/// containing 12 bytes.
///
/// # Arguments
/// * `key` - The AES-256 GCM key used for decryption.
/// * `ciphertext` - A slice of bytes (`&[u8]`) representing the encrypted data.
/// * `nonce` - A `Nonce` containing exactly 12 bytes (92 bits).
///
/// # Returns
/// A `Vec<u8>` containing the bytes of the decrypted plaintext.
///
/// # Errors
/// Returns an error if the nonce size is incorrect or if decryption fails.
pub(crate) fn aes_decrypt(
    key: &Key<Aes256Gcm>,
    ciphertext: &[u8],
    nonce: impl Into<Nonce>,
) -> Result<Vec<u8>, PrecompileError> {
    let nonce_array: Nonce = nonce.into();
    let cipher = Aes256Gcm::new(key);

    cipher
        .decrypt(&nonce_array.into(), ciphertext)
        .map_err(|e| PrecompileError::Other(format!("Encryption failed: {e}")))
}

/// Encrypts plaintext using AES-256 GCM with a 92-bit nonce.
///
/// This function requires the nonce to be exactly 92 bits (12 bytes),
/// with no padding or truncation. The caller must pass a `Vec<u8>`
/// containing 12 bytes.
///
/// # Arguments
/// * `key` - The AES-256 GCM key used for encryption.
/// * `plaintext` - The slice of bytes to encrypt.
/// * `nonce` - A `Nonce` containing exactly 12 bytes (92 bits).
///
/// # Returns
/// A `Vec<u8>` containing the bytes of encrypted ciphertext.
///
/// # Errors
/// Returns an error if the nonce size is incorrect or if encryption fails.
pub(crate) fn aes_encrypt(
    key: &Key<Aes256Gcm>,
    plaintext: &[u8],
    nonce: impl Into<Nonce>,
) -> Result<Vec<u8>, PrecompileError> {
    let nonce_array: Nonce = nonce.into();
    let cipher = Aes256Gcm::new(key);
    cipher
        .encrypt(&nonce_array.into(), plaintext)
        .map_err(|e| PrecompileError::Other(format!("AES encryption failed: {:?}", e)))
}

/// Derives an AES key from a shared secret using HKDF and SHA-256.
///
/// This function takes a `SharedSecret` and derives a 256-bit AES key using
/// the HKDF (HMAC-based Extract-and-Expand Key Derivation Function) with SHA-256.
///
/// # Arguments
/// * `shared_secret` - The shared secret from which the AES key will be derived.
///
/// # Returns
/// A `Result` containing the derived AES key, or an error if key derivation fails.
pub fn derive_aes_key(shared_secret: &SharedSecret) -> Result<Key<Aes256Gcm>, hkdf::InvalidLength> {
    // Initialize HKDF with SHA-256
    let hk = Hkdf::<Sha256>::new(None, &shared_secret.secret_bytes());

    // Output a 32-byte key for AES-256
    let mut okm = [0u8; 32];
    hk.expand(b"aes-gcm key", &mut okm)?;
    Ok(*Key::<Aes256Gcm>::from_slice(&okm))
}
