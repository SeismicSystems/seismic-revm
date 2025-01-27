use crate::primitives::Bytes;
use crate::seismic::precompiles::AES_GCM_DEC_ADDRESS;

use revm_precompile::{
    Precompile, PrecompileError, PrecompileOutput, PrecompileResult, PrecompileWithAddress,
};
use tee_service_api::aes_decrypt;

use super::common::{
    calculate_cost, parse_aes_key, validate_gas_limit, validate_input_length, validate_nonce_length,
};

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

pub const PRECOMPILE: PrecompileWithAddress = PrecompileWithAddress(
    AES_GCM_DEC_ADDRESS,
    Precompile::Standard(precompile_decrypt),
);

/// Minimal input size for AES-GCM (32-byte key + 12-byte nonce + 16-byte tag).
pub const MIN_INPUT_LENGTH: usize = 60;

/* --------------------------------------------------------------------------
Precompile Logic
-------------------------------------------------------------------------- */

/// # AES-256-GCM Decryption Precompile
///
/// **Input Layout** (mirrors encryption):
/// ```text
/// [0..32]   :  AES-256 key  (32 bytes)
/// [32..44]  :  12-byte nonce
/// [44.. ]   :  ciphertext + tag
/// ```
/// We decrypt `[44..]` using the key & nonce.
/// If the tag doesn't match, decryption fails with an error.
///
/// **Gas Model**:
/// Refer to the encryption file for further discussion.
pub fn precompile_decrypt(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    validate_input_length(input.len(), MIN_INPUT_LENGTH)?;

    let aes_key = parse_aes_key(&input[0..32])?;
    validate_nonce_length(&input[32..44])?;
    let nonce = (input[32..44]).to_vec();

    let ciphertext = &input[44..];

    let cost = calculate_cost(ciphertext.len());
    validate_gas_limit(cost, gas_limit)?;

    let plaintext = aes_decrypt(&aes_key.into(), ciphertext, nonce)
        .map_err(|e| PrecompileError::Other(format!("Decryption failed: {e}")))?;

    Ok(PrecompileOutput::new(cost, plaintext.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{hex, Bytes};
    use revm_precompile::{PrecompileError, PrecompileErrors};

    /// 1) Test the smallest possible cyphertext:
    ///    - 32-byte key + 12-byte nonce + 16-byte ciphertext (one block)
    ///    - Gas should match `1000 + 30 * 1 = 1030`.
    #[test]
    fn test_decrypt_ciphertext_from_empty_plaintext() {
        // output of empty plaintext encryption. Can look at aes_gcm_enc.rs for details.
        let mut input = vec![0u8; 44];
        let output_enc = hex!("530f8afbc74536b9a963b4f1c4cb738b");
        input.extend_from_slice(&output_enc);

        let gas_limit = 2000;
        let result = precompile_decrypt(&Bytes::from(input), gas_limit);

        assert!(result.is_ok(), "Should succeed for one-block ciphertext");
        let output = result.unwrap();
        assert_eq!(output.gas_used, 1000 + 30, "Must match cost formula = 1030");
        assert!(output.bytes.is_empty(), "Should produce empty plaintext");
    }

    /// 2) Test out-of-gas with large ciphertext:
    ///    Suppose 6 blocks => cost = 1000 + 6*30 = 1180, but we provide only 500 gas.
    #[test]
    fn test_decrypt_out_of_gas() {
        let input = vec![0u8; 44 + 96];
        let small_gas_limit = 500; // well below 1180 needed

        let result = precompile_decrypt(&Bytes::from(input), small_gas_limit);
        assert!(result.is_err());
        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    /// 3) Test invalid input length: fewer than 60 bytes => immediate error
    #[test]
    fn test_decrypt_invalid_input_length() {
        let input = vec![0u8; 20];
        let gas_limit = 2000;

        let result = precompile_decrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::Other(msg))) => {
                assert!(
                    msg.contains("invalid input length"),
                    "Should mention invalid input length"
                );
            }
            other => panic!("Expected invalid length error, got {:?}", other),
        }
    }
}
