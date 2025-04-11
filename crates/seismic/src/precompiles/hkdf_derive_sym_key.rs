use revm::{
    primitives::Bytes,
    precompile::{PrecompileWithAddress, PrecompileResult, PrecompileError, PrecompileOutput, calc_linear_cost_u32, u64_to_address},
};

use hkdf::Hkdf;
use sha2::Sha256;

/* --------------------------------------------------------------------------
Precompile Wiring
-------------------------------------------------------------------------- */
/// Address of ECDH precompile.
pub const HKDF_ADDRESS: u64 = 104; 

/// Returns the ecdh precompile with its address.
pub fn precompiles() -> impl Iterator<Item = PrecompileWithAddress> {
    [HKDF].into_iter()
}

pub const HKDF: PrecompileWithAddress =
    PrecompileWithAddress(u64_to_address(HKDF_ADDRESS), hkdf_derive_symmetric_key);

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
    2 * cost_single_sha256
}

/// For HKDF, we do:
///  1) EXTRACT → HMAC-SHA256( salt, input ) with variable-sized `input`. TODO: If no salt, is that
///     only one run SHA-256?
///  2) EXPAND  → HMAC-SHA256( PRK, info )  with a short, fixed-length `info`.
///
/// We'll treat `Expand` as another HMAC with small input => we approximate with a
/// constant cost derived from HMAC-SHA256 on a ~64-byte buffer, i.e. ~2 * single SHA-256 base.
pub const EXPAND_FIXED_COST: u64 = 2 * SHA256_BASE;

/// This is the label used in the `expand(...)` step.
const APPLICATION_INFO_BYTES: &[u8] = b"seismic_hkdf_105";

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
/// HMAC(K, M) = SHA256( (K ⊕ opad) || SHA256( (K ⊕ ipad) || M ) )
///
///   Where:
///     - (K ⊕ ipad) is the key XORed with a 64-byte 0x36 pattern
///     - (K ⊕ opad) is the same key XORed with a 64-byte 0x5C pattern
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
/// total_cost = HMAC_SHA256_EXTRACT(n) + EXPAND_FIXED_COST
///            = 2 × (60 + 12 * (#words)) + ~120
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
    hkdf.expand(APPLICATION_INFO_BYTES, &mut okm)
        .map_err(|_| PrecompileError::Other("HKDF expansion error".into()))?;

    Ok(PrecompileOutput::new(total_cost, okm.to_vec().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::primitives::Bytes;
    use revm::precompile::PrecompileError;
    use sha2::Sha256;

    /// 1) **Test Basic Derivation**  
    /// Ensures normal usage works, verifying we get a 32-byte key
    /// and that gas is properly accounted for.
    #[test]
    fn test_hkdf_derive_basic() {
        // Suppose we have a random 64-byte input
        let input = vec![42u8; 64];
        // Some comfortable gas limit
        let gas_limit = 80_000;

        // cf gas computation comments to understand the below line
        let gas_theoretically_spent = 2 * calc_linear_cost_u32(input.len(), 60, 12) + 2 * 60;

        let result = hkdf_derive_symmetric_key(&Bytes::from(input), gas_limit);
        assert!(result.is_ok(), "Expected success on normal input");
        let output = result.unwrap();

        // Ensure gas used is not zero and is within limit
        assert!(output.gas_used > 0, "Gas used should be > 0");
        assert!(
            output.gas_used <= gas_limit,
            "Should not exceed the provided gas"
        );
        assert!(
            output.gas_used == gas_theoretically_spent,
            "Gas spent should be equal to theoretical gas"
        );

        // We produce exactly 32 bytes
        assert_eq!(output.bytes.len(), 32, "HKDF output must be 32 bytes");
    }

    /// 2) **Test Empty Input**  
    /// HKDF generally supports empty input. Check that it succeeds
    /// and yields 32 bytes. We also verify that gas is deducted properly.
    #[test]
    fn test_hkdf_derive_empty_input() {
        let input = Bytes::new(); // empty
        let gas_limit = 50_000;

        let result = hkdf_derive_symmetric_key(&input, gas_limit);
        assert!(
            result.is_ok(),
            "Empty input should still produce a valid key"
        );
        let output = result.unwrap();

        // 32-byte output
        assert_eq!(
            output.bytes.len(),
            32,
            "Output should be 32 bytes for AES-256"
        );
        // Gas check
        assert!(output.gas_used > 0);
        assert!(output.gas_used <= gas_limit);
    }

    /// 3) **Test Out of Gas**  
    /// Force a scenario where input length is so large that the computed cost
    /// surpasses the provided gas limit. We expect the function to fail.
    #[test]
    fn test_hkdf_derive_out_of_gas() {
        // Very large input to drive cost high
        let input = vec![0u8; 10_000];
        let small_gas_limit = 1_000; // artificially small

        let result = hkdf_derive_symmetric_key(&Bytes::from(input), small_gas_limit);
        assert!(result.is_err(), "Should fail due to out of gas");
        assert_eq!(
            result.err(),
            Some(
                PrecompileError::OutOfGas
            ),
            "Expected OutOfGas error"
        );
    }

    /// 4) **Test Gas Exactly at Threshold**  
    /// Checks an edge case: we set gas such that it's exactly enough
    /// to cover the cost. We want to confirm it barely succeeds.
    #[test]
    fn test_hkdf_gas_exact_threshold() {
        // Choose input length that triggers a known cost
        // For instance, 32 bytes is 1 word; cost ~ 2*(60 + 12*1) for Extract + a small Expand cost.
        let input = vec![1u8; 32];

        // We'll guess a cost from the logic:
        //   extract = 2*(60 + 12*1) = 2*(72) = 144
        //   expand ~ 120
        //   total ~ 144 + 120 = 264
        let exact_gas = 264;

        let result = hkdf_derive_symmetric_key(&Bytes::from(input), exact_gas);
        assert!(result.is_ok(), "Should succeed exactly at the threshold");
        let output = result.unwrap();
        assert_eq!(output.gas_used, exact_gas, "Gas used should match exactly");
        assert_eq!(output.bytes.len(), 32);
    }

    /// 5) **Test Known HKDF Result**  
    /// Verify correctness by replicating HKDF in test and comparing outputs.
    /// We do a small known input and compare the precompile's output with
    /// direct Rust HKDF usage to confirm cryptographic equivalence.
    #[test]
    fn test_hkdf_known_result() {
        let input = b"HelloHKDF";
        let gas_limit = 10_000;

        // Precompile version
        let precompile_res = hkdf_derive_symmetric_key(&Bytes::from(input.as_ref()), gas_limit)
            .expect("Should succeed");
        let precompile_okm = precompile_res.bytes.clone();

        // Direct library usage (mirror the same HKDF calls).
        use hkdf::Hkdf;
        const AES_GCM_KEY_INFO: &[u8] = b"seismic_hkdf_105";
        let hkdf = Hkdf::<Sha256>::new(None, input.as_ref());
        let mut direct_okm = [0u8; 32];
        hkdf.expand(AES_GCM_KEY_INFO, &mut direct_okm)
            .expect("expand should never fail on 32-byte output");

        assert_eq!(
            precompile_okm.as_ref(),
            direct_okm,
            "Precompile output should match direct HKDF library usage"
        );
    }

    /// 6) **Test Reproducibility**  
    /// Same input => same derived key. Confirm no randomness is introduced,
    /// as HKDF is purely deterministic.
    #[test]
    fn test_hkdf_reproducibility() {
        let input = vec![123u8; 128];
        let gas_limit = 50_000;

        let out1 = hkdf_derive_symmetric_key(&Bytes::from(input.clone()), gas_limit)
            .unwrap()
            .bytes;
        let out2 = hkdf_derive_symmetric_key(&Bytes::from(input), gas_limit)
            .unwrap()
            .bytes;
        assert_eq!(
            out1, out2,
            "HKDF must produce the same key for identical input"
        );
    }
}
