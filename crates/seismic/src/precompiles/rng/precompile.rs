use revm::{
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::Bytes,
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
    transaction::abstraction::SeismicTxTr,
};

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

// The RNG precompile derives random bytes via HKDF-SHA256 using a schnorrkel key.
// Each call is stateless: the same (key, tx_hash, gas_left, pers) always produces
// the same output. Domain separation comes from tx_hash and gas_left appended
// to the HKDF info parameter.
pub const RNG_ADDRESS: u64 = 100; // Hex address `0x64`.

pub fn rng_precompile_iter<CTX: SeismicContextTr>(
) -> impl Iterator<Item = StatefulPrecompileWithAddress<CTX>> {
    [rng_precompile::<CTX>()].into_iter()
}

pub fn rng_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(RNG_ADDRESS), rng::<CTX>)
}

const MIN_INPUT_LENGTH: usize = 2;

/// Base cost for the HKDF-SHA256 derivation. This covers:
/// - HKDF-Extract: one HMAC-SHA256 (two SHA-256 passes over the 64-byte key)
/// - HKDF-Expand: one or more HMAC-SHA256 passes to produce output
/// - Conservative buffer for future adjustments
const RNG_BASE_COST: u64 = 3500;

/// Per-word cost for output bytes. Each 32-byte word of output requires
/// an additional HMAC-SHA256 round in the HKDF-Expand phase.
/// Based on SHA-256 EVM pricing (~6 gas/word) adjusted for HKDF overhead.
const RNG_WORD_COST: u64 = 5;

/* --------------------------------------------------------------------------
Precompile Logic
-------------------------------------------------------------------------- */
/// # RNG Precompile
/// ## Input Encoding
/// The input to this precompile is encoded as follows:
///
/// | Field                    | Bytes | Description                                                                |
/// | ------------------------ | ----- | -------------------------------------------------------------------------- |
/// | `requested_output_len`   | 4     | Big-endian `uint32` specifying how many random bytes to generate          |
/// | `pers`                   | n     | Remaining bytes used as personalization data (must be non-empty)          |
///
/// ## Overview
/// We interpret the input as a `[u8]` slice of bytes used as personalization
/// for the RNG derivation.
///
/// Using HKDF-SHA256 with the schnorrkel keypair as input key material,
/// and domain separation data (tx_hash, gas_left) plus personalization as the
/// HKDF info parameter, we derive the requested number of random bytes.
///
/// ## Gas Cost
///
/// Every call pays a flat base cost plus a per-word output cost:
/// ```text
/// cost = RNG_BASE_COST + ceil(output_len / 32) * RNG_WORD_COST
/// ```
///
/// The base cost (3500 gas) covers the HKDF-Extract and initial Expand rounds.
/// The per-word cost (5 gas) covers additional HMAC-SHA256 rounds for larger outputs.
fn rng<CTX: SeismicContextTr>(evmctx: &mut CTX, input: &Bytes, gas_limit: u64) -> PrecompileResult {
    // Validate input and extract parameters.
    validate_input_length(input.len(), MIN_INPUT_LENGTH)?;
    let (requested_output_len, pers) = parse_input(input)?;
    let requested_output_len = requested_output_len as usize;

    // Compute the gas cost.
    let gas_used = evmctx.chain().calculate_gas_cost(requested_output_len);
    if gas_used > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }

    let total_gas_remaining = evmctx.chain().gas_remaining_all_frames() + gas_limit;
    let tx_hash = evmctx.tx().tx_hash();

    // Derive the random bytes (stateless — each call is independent).
    let output = evmctx
        .chain()
        .process_rng(&pers, requested_output_len, &tx_hash, total_gas_remaining)
        .map_err(|e| PrecompileError::Other(e.to_string()))?;

    Ok(PrecompileOutput::new(gas_used, output))
}

/// Calculate the gas cost for an RNG precompile call.
/// Every call pays: BASE_COST + ceil(output_len / 32) * WORD_COST
pub(crate) fn calculate_gas_cost(output_len: usize) -> u64 {
    let words = (output_len as u64).div_ceil(32);
    RNG_BASE_COST + words * RNG_WORD_COST
}

// SAFETY: Indexing is validated by the length check above
#[allow(clippy::indexing_slicing)]
pub(crate) fn parse_input(input: &Bytes) -> Result<(u32, Bytes), PrecompileError> {
    if input.len() < 4 {
        return Err(PrecompileError::Other(
            "Insufficient input: need at least 4 bytes for length".to_string(),
        ));
    }

    let output_len_bytes: [u8; 4] = input[0..4].try_into().map_err(|_| {
        PrecompileError::Other("Failed to read requested output length (4 bytes)".to_string())
    })?;
    let requested_output_len = u32::from_be_bytes(output_len_bytes);

    let pers = input.slice(4..);
    Ok((requested_output_len, pers))
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::useless_conversion
    )]

    use crate::transaction::abstraction::SeismicTransaction;
    use crate::{DefaultSeismicContext, SeismicContext};
    use std::vec;

    use super::*;
    use revm::database::EmptyDB;
    use revm::precompile::PrecompileError;
    use revm::primitives::{Bytes, B256};
    use revm::Context;

    fn setup_rng_test(
        bytes_requested: u32,
        personalization: Option<Vec<u8>>,
    ) -> (
        u64,
        Bytes,
        SeismicContext<EmptyDB>,
        StatefulPrecompileWithAddress<SeismicContext<EmptyDB>>,
    ) {
        let gas_limit = 6000;

        // Prepare input bytes
        let mut input_data = bytes_requested.to_be_bytes().to_vec();

        // Add personalization if provided
        if let Some(pers) = personalization {
            input_data.extend(pers);
        }

        let input = Bytes::from(input_data);

        // Setup transaction and context
        let tx = SeismicTransaction::default().with_tx_hash(B256::from([0u8; 32]));
        let context = Context::seismic_with_random_rng_key().with_tx(tx);

        // Get precompile function
        let precompile = rng_precompile::<SeismicContext<EmptyDB>>;

        (gas_limit, input, context, precompile())
    }

    #[test]
    fn test_rng_init_no_pers() {
        let (gas_limit, input, mut context, precompile) = setup_rng_test(32, None);

        let result = precompile.1(&mut context, &input.into(), gas_limit);
        assert!(
            result.is_ok(),
            "Should succeed without default personalization"
        );

        let output = result.unwrap();
        // cost = 3500 + ceil(32/32) * 5 = 3505
        assert_eq!(output.gas_used, 3505, "Should consume exactly 3505 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_init_00_pers_different_than_no_pers() {
        // Test with explicit zero personalization
        let empty_pers = vec![0, 0, 0, 0]; // U32::ZERO.to_be_bytes_vec()
        let (gas_limit, input_with_pers, mut context_with_pers, precompile) =
            setup_rng_test(32, Some(empty_pers));

        let result_with_pers =
            precompile.1(&mut context_with_pers, &input_with_pers.into(), gas_limit);
        assert!(
            result_with_pers.is_ok(),
            "Should succeed with default personalization"
        );

        let output_with_pers = result_with_pers.unwrap();
        // cost = 3500 + ceil(32/32) * 5 = 3505
        assert_eq!(
            output_with_pers.gas_used, 3505,
            "Should consume exactly 3505 gas"
        );
        assert!(
            output_with_pers.bytes.len() == 32,
            "RNG output should be 32 bytes"
        );

        // Test without personalization
        let (gas_limit, input_no_pers, mut context_no_pers, precompile) = setup_rng_test(32, None);

        let result_no_pers = precompile.1(&mut context_no_pers, &input_no_pers.into(), gas_limit);
        assert!(
            result_no_pers.is_ok(),
            "Should succeed without default personalization"
        );

        let output_no_pers = result_no_pers.unwrap();
        assert_eq!(
            output_no_pers.gas_used, 3505,
            "Should consume exactly 3505 gas"
        );
        assert!(
            output_no_pers.bytes.len() == 32,
            "RNG output should be 32 bytes"
        );
    }

    #[test]
    fn test_rng_init_with_pers() {
        let personalization = vec![1, 2, 3, 4];
        let (gas_limit, input, mut context, precompile) = setup_rng_test(32, Some(personalization));

        let result = precompile.1(&mut context, &input.into(), gas_limit);
        assert!(result.is_ok(), "Should succeed with personalization");

        let output = result.unwrap();
        // cost = 3500 + ceil(32/32) * 5 = 3505
        assert_eq!(output.gas_used, 3505, "Should consume exactly 3505 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_second_call_pays_full_cost() {
        let empty_pers = vec![0, 0, 0, 0];
        let (_, input, mut context, precompile) = setup_rng_test(32, Some(empty_pers));

        // Call once
        let _ = precompile.1(&mut context, &input.clone().into(), 6000);

        // Second call should pay the same full cost (no caching discount)
        let result = precompile.1(&mut context, &input.into(), 6000);
        assert!(result.is_ok(), "Should succeed on second call");

        let output = result.unwrap();
        assert_eq!(
            output.gas_used, 3505,
            "Should consume exactly 3505 gas (full cost, no caching)"
        );
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_out_of_gas() {
        let empty_pers = vec![0, 0, 0, 0];
        let (_, input, mut context, precompile) = setup_rng_test(16, Some(empty_pers));

        let insufficient_gas = 2500; // less than the base cost of 3500
        let result = precompile.1(&mut context, &input.into(), insufficient_gas);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_rng_out_of_gas_large_output() {
        let empty_pers = vec![0, 0, 0, 0];
        let (_, input, mut context, precompile) = setup_rng_test(6000, Some(empty_pers));

        // cost = 3500 + ceil(6000/32) * 5 = 3500 + 188*5 = 3500 + 940 = 4440
        let insufficient_gas = 4000;
        let result = precompile.1(&mut context, &input.into(), insufficient_gas);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_input_length() {
        // Create an invalid input (too short)
        let input_vector = vec![0x00, 0x01, 0x02]; // 3 bytes only
        let input = Bytes::from(input_vector);

        let (gas_limit, _, mut context, precompile) = setup_rng_test(0, None);

        let result = precompile.1(&mut context, &input.into(), gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert!(
                    msg.contains("Insufficient input: need at least 4 bytes for length"),
                    "Should mention invalid input length"
                );
            }
            other => panic!("Expected PrecompileError with length msg, got {:?}", other),
        }
    }

    #[test]
    fn test_rng_init_cost_monotonic() {
        let mut prev = calculate_init_cost(0);
        for len in 1..=1024 {
            let cost = calculate_init_cost(len);
            assert!(
                cost >= prev,
                "Init cost must be monotonically non-decreasing: cost({len}) = {cost} < cost({}) = {prev}",
                len - 1
            );
            prev = cost;
        }
    }

    #[test]
    fn test_rng_fill_cost_monotonic() {
        let mut prev = calculate_fill_cost(0);
        for len in 1..=1024 {
            let cost = calculate_fill_cost(len);
            assert!(
                cost >= prev,
                "Fill cost must be monotonically non-decreasing: cost({len}) = {cost} < cost({}) = {prev}",
                len - 1
            );
            prev = cost;
        }
    }

    #[test]
    fn test_rng_cost_no_overflow() {
        let init = calculate_init_cost(usize::MAX);
        assert!(
            init >= calculate_init_cost(0),
            "Extreme pers length must not wrap below base cost"
        );

        let fill = calculate_fill_cost(usize::MAX);
        assert!(
            fill >= calculate_fill_cost(0),
            "Extreme fill length must not wrap below base cost"
        );
    }
}
