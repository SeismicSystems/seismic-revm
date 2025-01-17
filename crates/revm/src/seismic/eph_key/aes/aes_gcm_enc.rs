use crate::primitives::{Address, Bytes};
use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, KeyInit},
    Aes256Gcm, Key,
};
use revm_precompile::{
    u64_to_address, Precompile, PrecompileError, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
};
use sha2::digest::consts::U12;

use super::common::{
    calculate_cost, parse_aes_key, parse_nonce, validate_gas_limit, validate_input_length,
    validate_nonce_length,
};

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

/// On-chain address for the AES-256-GCM precompile. Adjust as desired.
pub const ADDRESS: Address = u64_to_address(103);

/// Register the AES encryption precompile at `0x103`.
pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(precompile_encrypt));

/// Minimal input size:
/// - 32 bytes for the AES key,
/// + 12 bytes for the nonce,
/// + 0+ bytes for plaintext (we allow empty plaintext).
/// = at least 40 if you want to allow zero-length plaintext.
pub const MIN_INPUT_LENGTH: usize = 44;

/* --------------------------------------------------------------------------
Precompile Logic
-------------------------------------------------------------------------- */

/// # AES-256-GCM Encryption Precompile
///
/// ## Overview
/// We interpret the input as:
/// ┌───────────────────── 32 bytes (AES Key, 256 bits) ─────────────────────┐
/// │    [0..32]:  Aes256Gcm key                                           │
/// └────────────────────────────────────────────────────────────────────────┘
/// ┌───────────────────── 12 bytes (nonce in big-endian) ────────────────────┐
/// │   [32..44]:  96-bit nonce                                            │
/// └────────────────────────────────────────────────────────────────────────┘
/// ┌────────────────────────────────────────────────────────────────────────┐
/// │   [44..] :  Plaintext bytes                                          │
/// └────────────────────────────────────────────────────────────────────────┘
///
/// We encrypt `[44..]` using AES-256 in CTR mode (via `aes_encrypt()`),
/// and produce a GCM authentication tag. The output is `[ciphertext + tag]`.
///
/// ## Gas Model
/// let num_blocks = (plaintext_len + 15) / 16;
/// let cost = AES_GCM_BASE + AES_GCM_PER_BLOCK * num_blocks;
/// If `cost > gas_limit`, we revert with `OutOfGas`.
///
/// We set the final `gas_used` = `cost`.
pub fn precompile_encrypt(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    validate_input_length(input.len(), MIN_INPUT_LENGTH)?;

    let aes_key = parse_aes_key(&input[0..32])?;
    validate_nonce_length(&input[32..44])?;

    let nonce = parse_nonce(&input[32..44]);
    let plaintext = &input[44..];

    let cost = calculate_cost(plaintext.len());
    validate_gas_limit(cost, gas_limit)?;

    let ciphertext = perform_encryption(aes_key, nonce, plaintext)?;

    Ok(PrecompileOutput::new(cost, ciphertext.into()))
}

fn perform_encryption(
    aes_key: Key<Aes256Gcm>,
    nonce: GenericArray<u8, U12>,
    plaintext: &[u8],
) -> Result<Vec<u8>, PrecompileError> {
    let cipher = Aes256Gcm::new(&aes_key);
    cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| PrecompileError::Other(format!("Encryption failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Bytes;
    use revm_precompile::PrecompileErrors;

    /// 1) Test a normal case: a small non-empty plaintext,
    ///    verifying correct gas usage and successful encryption.
    #[test]
    fn test_encrypt_small_plaintext() {
        // Prepare input:
        //   [0..32]: AES key
        //   [32..44]: 12-byte nonce
        //   [44..]: small plaintext (16 bytes => exactly 1 block)
        let mut input = vec![0u8; 44 + 16];
        // Key can be any random 32 bytes; here all zero for test
        // Nonce next 8 bytes = also zero
        // Plaintext next 16 bytes => we do [40..56]
        input[44..60].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);

        // The cost formula is:
        //   cost = 1000 (AES_GCM_BASE) + 30 (AES_GCM_PER_BLOCK) * 1 block => 1030
        let gas_limit = 2_000; // well above 1030

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Should succeed for small plaintext");

        let output = result.unwrap();
        assert_eq!(
            output.gas_used,
            1000 + 30,
            "Should consume exactly 1030 gas"
        );
        assert!(
            !output.bytes.is_empty(),
            "Encryption output shouldn't be empty"
        );
    }

    /// 2) Test an empty plaintext scenario:
    ///    i.e. 32-byte key + 12-byte nonce + 0 plaintext => exactly 44 bytes.
    #[test]
    fn test_encrypt_empty_plaintext() {
        let input = vec![0u8; 44];
        // cost = 1000 + 30 * 0 = 1000
        let gas_limit = 2_000;
        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Empty plaintext should be valid");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 1000, "Cost must be base only (no blocks)");
        assert!(!output.bytes.is_empty(), "Should still produce a tag");
    }

    /// 3) Test insufficient gas: large plaintext but too little gas
    ///    We expect an OutOfGas error.
    #[test]
    fn test_out_of_gas() {
        // 32 + 8 + 96 => 6 blocks (since 96 / 16 = 6)
        // cost = 1000 + 6*30 = 1180
        // We'll give less than that
        let input = vec![0u8; 44 + 96];
        // Just fill with zeros
        let small_gas_limit = 500; // well below 1180

        let result = precompile_encrypt(&Bytes::from(input), small_gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    /// 4) Test invalid input length: fewer than 40 bytes.
    ///    Must fail with "invalid input length".
    #[test]
    fn test_invalid_input_length() {
        let input = vec![0u8; 20];
        let gas_limit = 2_000;

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_err());

        // We expect a PCError::Other complaining about input length
        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::Other(msg))) => {
                assert!(
                    msg.contains("invalid input length"),
                    "Should mention invalid input length"
                );
            }
            other => panic!("Expected PrecompileError with length msg, got {:?}", other),
        }
    }
}
