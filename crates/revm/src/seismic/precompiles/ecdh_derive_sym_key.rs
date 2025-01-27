use super::{hkdf_derive_sym_key::EXPAND_FIXED_COST, HDFK_ADDRESS};
use crate::primitives::Bytes;

use revm_precompile::{
    Precompile, PrecompileError, PrecompileOutput, PrecompileResult, PrecompileWithAddress,
};
use secp256k1::{ecdh::SharedSecret, PublicKey, SecretKey};

use tee_service_api::derive_aes_key;

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(HDFK_ADDRESS, Precompile::Standard(derive_symmetric_key));

/// Expected input layout:
/// - 32 bytes: secp256k1 secret key
/// - 33 bytes: secp256k1 compressed public key
pub const INPUT_LENGTH: usize = 65;

/* --------------------------------------------------------------------------
 Cost Model
-------------------------------------------------------------------------- */

/// We adopt an intentionally *high* (ceiling) gas price for ECDH + HKDF:
///
/// 1. **ECDH** cost is roughly one scalar multiplication on secp256k1,  
///    but we price it near or above `ECRecover` (~3000 gas) to be safe. ECRecover
///    has many scalar multiplications and additions compared to ECDH scalar mul.
/// 2. **HKDF** overhead is minor (see the HKDF precompile doc for details),
///    but we account for it anyways.
///
/// By setting a single constant (`DERIVE_SYM_KEY_COST`), we cover the entire
/// flow (uncompressing the secp256k1 public key, scalar-multiplying, plus
/// the HKDF extraction & expansion to produce the final AES key).
///
/// This ensures we don't underprice the operation, even though ECDH is
/// arguably simpler than a full ECDSA recover (which includes signature checks).
///
const SHARED_SECRET_COST: u64 = 3000;
const DERIVE_SYM_KEY_COST: u64 = SHARED_SECRET_COST + EXPAND_FIXED_COST;

/* --------------------------------------------------------------------------
 Precompile Logic
-------------------------------------------------------------------------- */

/// # Derive Symmetric Key (ECDH + HKDF-AES)
///
/// Accepts 65 bytes of input:
///  - `0..32`: `sk_bytes` (secp256k1 secret key)
///  - `32..65`: `pk_bytes` (compressed public key)
///
/// Steps:
/// 1) Compute ECDH shared secret using `pk * sk`.
/// 2) Derive a 32-byte AES key from the shared secret via `tee_service_api::derive_aes_key`,
///    which internally runs HKDF-SHA256 (see separate doc for gas breakdown).
///
/// Returns the 32-byte AES key if successful, or an error otherwise.
///
/// ## Gas:
/// *We apply a constant `DERIVE_SYM_KEY_COST` (3,800), ensuring we
/// overestimate in comparison to `ECRecover` and simpler HKDF ops.*  
///
/// If `gas_limit < DERIVE_SYM_KEY_COST`, we revert with `OutOfGas`.
pub fn derive_symmetric_key(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    if DERIVE_SYM_KEY_COST > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    if input.len() != INPUT_LENGTH {
        let err_msg = format!(
            "invalid input length: expected {INPUT_LENGTH} but got {}",
            input.len()
        );
        return Err(PrecompileError::Other(err_msg).into());
    }

    let sk_bytes = &input[..32];
    let pk_bytes = &input[32..];

    let secret_key: SecretKey = SecretKey::from_slice(sk_bytes)
        .map_err(|e| PrecompileError::Other(format!("secret key deser err: {e}")))?;

    let public_key: PublicKey = PublicKey::from_slice(pk_bytes)
        .map_err(|e| PrecompileError::Other(format!("public key deser err: {e}")))?;

    let shared_secret = SharedSecret::new(&public_key, &secret_key);

    let aes_key = derive_aes_key(&shared_secret)
        .map_err(|e| PrecompileError::Other(format!("aes derivation failed: {e}")))?;

    let output_32: [u8; 32] = aes_key.to_vec().try_into().expect("must be 32 bytes");

    Ok(PrecompileOutput::new(DERIVE_SYM_KEY_COST, output_32.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::hex;
    use revm_precompile::{PrecompileError, PrecompileErrors};

    /// 1) Tests normal usage with valid 65-byte input,
    ///    ensuring we get a 32-byte output and don't exceed gas.
    #[test]
    fn test_derive_key_success() {
        // 32 bytes of secret key + 33 bytes of compressed pubkey
        // These are dummy placeholders; in a real scenario you'd produce them
        // from a valid secp256k1 library or a known test vector.
        let sk1 = hex!("7e38022030c40773cc561c1cc9c0053e48b0be2cee33c13495f096942ea176ef");
        let pk1 = hex!("03f176e697b5b0c4799f1816f5fe114263d1c01a84ad296129f994278499f0842e");

        // Concatenate secret key + pubkey into 65 bytes
        let input_data = [sk1.as_ref(), pk1.as_ref()].concat();

        // Sufficient gas
        let gas_limit = 10_000;

        let result = derive_symmetric_key(&Bytes::from(input_data), gas_limit);
        assert!(result.is_ok(), "Expected successful derivation");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 3120, "Cost must match our constant");
        assert_eq!(output.bytes.len(), 32, "Should produce exactly 32-byte key");
    }

    /// 2) Tests an out-of-gas scenario by providing smaller gas than 3,800.
    #[test]
    fn test_out_of_gas() {
        let mut input_data = vec![0u8; 65];
        input_data[32] = 0x02; // compressed format marker

        let tiny_gas_limit = 1_000; // Less than 3,800
        let result = derive_symmetric_key(&Bytes::from(input_data), tiny_gas_limit);
        assert!(result.is_err(), "Should fail due to out of gas");

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    /// 3) Tests incorrect input length (e.g. 64 bytes),
    ///    ensuring it fails with an "invalid length" error.
    #[test]
    fn test_invalid_input_length() {
        // 64 instead of 65
        let input_data = vec![0u8; 64];
        let gas_limit = 10_000;

        let result = derive_symmetric_key(&Bytes::from(input_data), gas_limit);
        assert!(result.is_err());

        // We check it's not `OutOfGas`, but a parse error
        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::Other(msg))) => {
                assert!(
                    msg.contains("invalid input length"),
                    "Expected length error"
                );
            }
            other => panic!("Expected invalid length error, got {:?}", other),
        }
    }

    /// 4) Tests failure in deserialization (unparsable secret/public key),
    ///    e.g. passing random garbage that can't be decoded as a valid key.
    #[test]
    fn test_deserialize_failure() {
        let mut input_data = vec![0xFF; 65]; // garbage
        input_data[32] = 0x02; // pretend it's compressed

        let gas_limit = 10_000;
        let result = derive_symmetric_key(&Bytes::from(input_data), gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::Other(msg))) => {
                assert!(
                    msg.contains("deser err"),
                    "Should mention deserialization error"
                );
            }
            other => panic!("Expected deserialization failure, got {:?}", other),
        }
    }

    /// 5) (Optional) Tests that derivation for a specific input is reproducible.
    ///    i.e., same input => same derived key.
    ///    This checks there's no hidden randomness in the precompile logic.
    #[test]
    fn test_reproducibility() {
        let mut input_data = vec![0xAB; 65];
        input_data[32] = 0x03;

        let gas_limit = 10_000;

        let out1 = derive_symmetric_key(&Bytes::from(input_data.clone()), gas_limit)
            .unwrap()
            .bytes;
        let out2 = derive_symmetric_key(&Bytes::from(input_data), gas_limit)
            .unwrap()
            .bytes;

        assert_eq!(
            out1, out2,
            "Derivation must be deterministic for identical input"
        );
    }

    /// 6) Test that swapping the private/public keys from two pairs
    ///    yields the same shared secret (ECDH property).
    #[test]
    fn test_reproducibility_swapped_keys() {
        // Keys #1
        let sk1 = hex!("7e38022030c40773cc561c1cc9c0053e48b0be2cee33c13495f096942ea176ef");
        let pk1 = hex!("03f176e697b5b0c4799f1816f5fe114263d1c01a84ad296129f994278499f0842e");

        // Keys #2
        let sk2 = hex!("adbed354135e517bc881d55fa60c455737d1ba98d446c0866cec3837e13d9906");
        let pk2 = hex!("02555d7b94d8afc4afdf5a03e9da73a408b6d19c865036bae833864d2353e85a25");

        // ECDH property: sk1 * pk2 == sk2 * pk1 (shared secret is identical).
        let input_1 = [sk1.as_ref(), pk2.as_ref()].concat(); // (sk1, pk2)
        let input_2 = [sk2.as_ref(), pk1.as_ref()].concat(); // (sk2, pk1)

        let gas_limit = 10_000;

        let out1 = derive_symmetric_key(&Bytes::from(input_1), gas_limit)
            .unwrap()
            .bytes;
        let out2 = derive_symmetric_key(&Bytes::from(input_2), gas_limit)
            .unwrap()
            .bytes;

        assert_eq!(
            out1, out2,
            "Swapped key pairs must derive the same shared secret"
        );
    }
}
