use crate::primitives::{Address, Bytes};
use hkdf::Hkdf;
use revm_precompile::{
    calc_linear_cost_u32, u64_to_address, Precompile, PrecompileError, PrecompileOutput, PrecompileResult, PrecompileWithAddress
};
use sha2::Sha256;


/* --------------------------------------------------------------------------
   Precompile Wiring 
   -------------------------------------------------------------------------- */
pub const ADDRESS: Address = u64_to_address(105);

pub const PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(ADDRESS, Precompile::Standard(hkdf_derive_symmetric_key));

/* --------------------------------------------------------------------------
    Cost Constants
   -------------------------------------------------------------------------- */

/// Single SHA-256 precompile cost: 60 + 12 * (#words) as per the sha256 precompile.
/// - 60 is a base cost
/// - 12 is per 32-byte word
const SHA256_BASE: u64 = 60;
const SHA256_PER_WORD: u64 = 12;

/// HMAC-SHA256 = 2 passes of SHA-256 (inner + outer):
/// So each HMAC run costs about 2×(SHA256_BASE + SHA256_PER_WORD * (#words)).
fn calc_hmac_sha256_cost(input_len: usize) -> u64 {
    let cost_single_sha256 = calc_linear_cost_u32(input_len, SHA256_BASE, SHA256_PER_WORD);
    (2 * cost_single_sha256) as u64
}

/// For HKDF, we do:
///  1) EXTRACT → HMAC-SHA256( salt, input ) with variable-sized `input`. TODO: If no salt, is that
///     only one run SHA-256?
///  2) EXPAND  → HMAC-SHA256( PRK, info )  with a short, fixed-length `info`.
///
/// We'll treat `Expand` as another HMAC with small input => we approximate with a
/// constant cost derived from HMAC-SHA256 on a ~64-byte buffer, i.e. ~2 * single SHA-256 base.
const EXPAND_FIXED_COST: u64 = (2 * SHA256_BASE) as u64; 

/// This is the label used in the `expand(...)` step. 
const AES_GCM_KEY_INFO: &[u8] = b"Seismic: aes-gcm key";


/* --------------------------------------------------------------------------
    HKDF with Gas Calculation
   -------------------------------------------------------------------------- */

/// # HKDF-based AES symmetric key derivation
///
/// This precompile implements [HKDF](https://tools.ietf.org/html/rfc5869) with **SHA-256**.
/// It processes the input in two stages:
///
/// 1) **Extract**: uses an HMAC-SHA256 with `input` as key material to produce a pseudo-random key (PRK).
/// 2) **Expand**: uses a second HMAC-SHA256 to generate exactly 32 bytes for an AES-256 key.
///
/// Internally, **each HMAC** is two SHA-256 passes (inner + outer), like so:
/// ```
/// HMAC(K, M) = SHA256( (K ⊕ opad) || SHA256( (K ⊕ ipad) || M ) )
///
///   Where:
///     - (K ⊕ ipad) is the key XORed with a 64-byte 0x36 pattern
///     - (K ⊕ opad) is the same key XORed with a 64-byte 0x5C pattern
/// ```
///
/// That means for a single HMAC, we have two hashing passes. HKDF then does 2 HMAC calls:
/// one for Extract, one for Expand.
///
/// # Gas Cost
///
/// Let `len(input) = n`. We do:
/// - **Extract** cost ~ 2 × SHA-256 (each pass processes `n` bytes).
/// - **Expand** cost ~ 2 × SHA-256 (but for a short, fixed-size input).
///
/// We approximate the Expand step as a constant `EXPAND_FIXED_COST`.  
/// Overall:
/// ```
/// total_cost = HMAC_SHA256_EXTRACT(n) + EXPAND_FIXED_COST
///            = 2 × (60 + 12 * (#words)) + ~120
/// ```
/// where `#words = ceil(n / 32)`.
///
/// # Returns
///
/// A `PrecompileResult` whose `bytes` field is a 32-byte key for AES-256.
///
/// # Errors
///
/// - Returns `OutOfGas` if the total cost exceeds the available `gas_limit`.
/// - Returns `HKDF expansion error` if anything goes wrong in the internal HKDF call
///   (rare in practice).
pub fn hkdf_derive_symmetric_key(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    let extract_cost = calc_hmac_sha256_cost(input.len());
    let total_cost = extract_cost + EXPAND_FIXED_COST;

    if total_cost > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    let hkdf = Hkdf::<Sha256>::new(None, input);

    let mut okm = [0u8; 32];
    hkdf.expand(AES_GCM_KEY_INFO, &mut okm)
        .map_err(|_| PrecompileError::Other("HKDF expansion error".into()))?;

    Ok(PrecompileOutput::new(total_cost, okm.to_vec().into()))
}
