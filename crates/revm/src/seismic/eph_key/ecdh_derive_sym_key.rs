use bincode;
use revm_precompile::{
    u64_to_address, PrecompileError, Precompile, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
};
use secp256k1::{ecdh::SharedSecret, PublicKey, SecretKey};

use crate::primitives::{Address, Bytes};

use tee_service_api::derive_aes_key;

use super::hkdf_derive_sym_key::EXPAND_FIXED_COST;

/// On-chain address for the ECDH-based AES derivation precompile.
pub const ADDRESS: Address = u64_to_address(102);

/// Registration of this precompile with its address and logic entrypoint.
pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(derive_symmetric_key));

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
/// You can tune this value to your chain's performance profile.
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

    let secret_key: SecretKey = bincode::deserialize(sk_bytes)
        .map_err(|e| PrecompileError::Other(format!("secret key deser err: {e}")))?;

    let public_key: PublicKey = bincode::deserialize(pk_bytes)
        .map_err(|e| PrecompileError::Other(format!("public key deser err: {e}")))?;

    let shared_secret = SharedSecret::new(&public_key, &secret_key);

    let aes_key = derive_aes_key(&shared_secret)
        .map_err(|e| PrecompileError::Other(format!("aes derivation failed: {e}")))?;

    let output_32: [u8; 32] = aes_key.to_vec().try_into().expect("must be 32 bytes");

    Ok(PrecompileOutput::new(DERIVE_SYM_KEY_COST, output_32.into()))
}

