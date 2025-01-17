use aes_gcm::{Aes256Gcm, Key};
use revm_precompile::{
    calc_linear_cost, u64_to_address, PrecompileError, Precompile, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};
use crate::precompile::Error as PCError;
use crate::primitives::{Address, Bytes};
use tee_service_api::aes_encrypt;

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
/// - 8 bytes for the nonce,
/// - 0+ bytes for plaintext (we allow empty plaintext).
/// => at least 40 if you want to allow zero-length plaintext.
pub const MIN_INPUT_LENGTH: usize = 40;

/// The below gas cost are very rough estimates.
/// Overhead cost for AES-GCM setup & finalization. We intentionally overprice to stay safe.
const AES_GCM_BASE: u64 = 1000;

/// Per 16-byte block cost. One AES encryption + one GHASH multiply per block, plus cushion.
const AES_GCM_PER_BLOCK: u64 = 30;

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
/// ┌───────────────────── 8 bytes (nonce in big-endian) ────────────────────┐
/// │   [32..40]:  64-bit nonce                                            │
/// └────────────────────────────────────────────────────────────────────────┘
/// ┌────────────────────────────────────────────────────────────────────────┐
/// │   [40..] :  Plaintext bytes                                          │
/// └────────────────────────────────────────────────────────────────────────┘
///
/// We encrypt `[40..]` using AES-256 in CTR mode (via `aes_encrypt()`),
/// and produce a GCM authentication tag. The output is `[ciphertext + tag]`.
///
/// ## Gas Model
/// let num_blocks = (plaintext_len + 15) / 16;
/// let cost = AES_GCM_BASE + AES_GCM_PER_BLOCK * num_blocks;
/// If `cost > gas_limit`, we revert with `OutOfGas`.
///
/// We set the final `gas_used` = `cost`.
pub fn precompile_encrypt(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    if input.len() < MIN_INPUT_LENGTH {
        let err_msg = format!(
            "invalid input length: must be >= {MIN_INPUT_LENGTH}, got {}",
            input.len()
        );
        return Err(PrecompileError::Other(err_msg).into());
    }

    let aes_key = Key::<Aes256Gcm>::from_slice(&input[0..32]);
    let nonce_be = u64::from_be_bytes(
        input[32..40]
            .try_into()
            .map_err(|e| PCError::Other(format!("nonce parse error: {e}")))?
    );

    let plaintext = &input[40..];

    let plaintext_len = plaintext.len();
    let cost = calc_linear_cost(16, plaintext_len, AES_GCM_BASE, AES_GCM_PER_BLOCK);

    if cost > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    let ciphertext = aes_encrypt(aes_key, plaintext, nonce_be)
        .map_err(|e| PCError::Other(e.to_string()))?;

    Ok(PrecompileOutput::new(cost, ciphertext.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm_precompile::PrecompileErrors;
    use crate::primitives::Bytes;

    /// 1) Test a normal case: a small non-empty plaintext,
    ///    verifying correct gas usage and successful encryption.
    #[test]
    fn test_encrypt_small_plaintext() {
        // Prepare input:
        //   [0..32]: AES key
        //   [32..40]: 8-byte nonce
        //   [40..]: small plaintext (16 bytes => exactly 1 block)
        let mut input = vec![0u8; 40 + 16];
        // Key can be any random 32 bytes; here all zero for test
        // Nonce next 8 bytes = also zero
        // Plaintext next 16 bytes => we do [40..56]
        input[40..56].copy_from_slice(&[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16]);

        // The cost formula is:
        //   cost = 1000 (AES_GCM_BASE) + 30 (AES_GCM_PER_BLOCK) * 1 block => 1030
        let gas_limit = 2_000; // well above 1030

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Should succeed for small plaintext");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 1000 + 30, "Should consume exactly 1030 gas");
        // Output is ciphertext + tag, must be non-empty
        assert!(!output.bytes.is_empty(), "Encryption output shouldn't be empty");
    }

    /// 2) Test an empty plaintext scenario:
    ///    i.e. 32-byte key + 8-byte nonce + 0 plaintext => exactly 40 bytes.
    #[test]
    fn test_encrypt_empty_plaintext() {
        let input = vec![0u8; 40];
        // cost = 1000 + 30 * 0 = 1000
        let gas_limit = 2_000;

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Empty plaintext should be valid");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 1000, "Cost must be base only (no blocks)");
        // Typically GCM produces just the 16-byte tag if plaintext is empty
        assert!(!output.bytes.is_empty(), "Should still produce a tag");
    }

    /// 3) Test insufficient gas: large plaintext but too little gas
    ///    We expect an OutOfGas error.
    #[test]
    fn test_out_of_gas() {
        // 32 + 8 + 96 => 6 blocks (since 96 / 16 = 6)
        // cost = 1000 + 6*30 = 1180
        // We'll give less than that
        let mut input = vec![0u8; 40 + 96];
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
        let input = vec![0u8; 20]; // definitely less than 40
        let gas_limit = 2_000;

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_err());

        // We expect a PCError::Other complaining about input length
        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::Other(msg))) => {
                assert!(msg.contains("invalid input length"),
                    "Should mention invalid input length"
                );
            }
            other => panic!("Expected PCError::Other with length msg, got {:?}", other),
        }
    }

    /// 5) (Optional) Test large input that *does* fit the gas limit.
    ///    This ensures we handle big data properly.
    #[test]
    fn test_large_input_enough_gas() {
        // Suppose we have 32 + 8 + 512 => 512 bytes of plaintext => 512/16=32 blocks
        // cost = 1000 + 30*32 = 1000 + 960 = 1960
        let mut input = vec![0u8; 40 + 512];
        let gas_limit = 3_000; // above 1960

        let result = precompile_encrypt(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Should succeed with large input if gas is enough");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 1960, "Should match cost formula for 32 blocks");
        assert!(!output.bytes.is_empty(), "Should produce ciphertext + tag");
    }
}

