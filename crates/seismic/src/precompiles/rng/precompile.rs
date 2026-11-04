use revm::{
    precompile::{u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult},
    primitives::Bytes,
};

use crate::{
    api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress,
    transaction::abstraction::SeismicTxTr,
};

use super::domain_sep_rng::{
    CHUNK_COUNTER_LEN, HKDF_OUTPUT_BLOCK_SIZE, MAX_HKDF_OUTPUT, RNG_INFO_PREFIX_LEN,
};

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

// The RNG precompile derives random bytes via HKDF-SHA256 from the node's rng key.
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

/// Base cost for HKDF-Extract on the fixed-size key material and setup overhead.
const RNG_BASE_COST: u64 = 3500;
/// Per 32-byte word copied into the personalization info buffer.
const RNG_COPY_WORD_COST: u64 = 5;
/// Approximate HMAC-SHA256 pricing, matching the HKDF precompile's two-pass model.
/// These are gas-schedule coefficients, not measured CPU cycle counts.
const RNG_ROUND_BASE_COST: u64 = 2 * 60;
const RNG_HASH_WORD_COST: u64 = 2 * 12;

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
/// | `pers`                   | n     | Remaining bytes used as personalization data (may be empty)          |
///
/// ## Overview
/// We interpret the input as a `[u8]` slice of bytes used as personalization
/// for the RNG derivation.
///
/// Using HKDF-SHA256 with the node's rng key as input key material,
/// and domain separation data (tx_hash, gas_left) plus personalization as the
/// HKDF info parameter, we derive the requested number of random bytes.
///
/// ## Gas Cost
///
/// Each 32-byte output block requires hashing the entire HKDF info again:
/// ```text
/// rounds = ceil(output_len / 32)
/// info_len = 121 + pers_len + (if output_len > 8160 { 4 } else { 0 })
/// cost = 3500 + 5 * ceil(pers_len / 32)
///             + rounds * (120 + 24 * ceil((info_len + 33) / 32))
/// ```
///
/// The 33 bytes conservatively cover a previous 32-byte output block and the
/// one-byte HKDF round counter, including the first round of each expansion.
/// The four-byte chunk counter is included in every round of chunked output.
/// All cost arithmetic saturates, and gas is checked before derivation.
fn rng<CTX: SeismicContextTr>(evmctx: &mut CTX, input: &Bytes, gas_limit: u64) -> PrecompileResult {
    // Validate input and extract parameters.
    validate_input_length(input.len(), MIN_INPUT_LENGTH)?;
    let (requested_output_len, pers) = parse_input(input)?;
    let requested_output_len = requested_output_len as usize;

    // Compute the gas cost.
    let gas_used = calculate_gas_cost(pers.len(), requested_output_len);
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

/// Charge for setup and for re-hashing the full info in every HKDF output round.
pub(crate) fn calculate_gas_cost(pers_len: usize, output_len: usize) -> u64 {
    calculate_init_cost(pers_len).saturating_add(calculate_fill_cost(pers_len, output_len))
}

fn calculate_init_cost(pers_len: usize) -> u64 {
    (pers_len as u64)
        .div_ceil(32)
        .saturating_mul(RNG_COPY_WORD_COST)
        .saturating_add(RNG_BASE_COST)
}

fn calculate_fill_cost(pers_len: usize, output_len: usize) -> u64 {
    let chunk_counter_len = if output_len > MAX_HKDF_OUTPUT {
        CHUNK_COUNTER_LEN as u64
    } else {
        0
    };
    let round_input_len = (pers_len as u64)
        .saturating_add(RNG_INFO_PREFIX_LEN as u64)
        .saturating_add(chunk_counter_len)
        .saturating_add(HKDF_OUTPUT_BLOCK_SIZE as u64)
        .saturating_add(1);
    let round_cost = round_input_len
        .div_ceil(32)
        .saturating_mul(RNG_HASH_WORD_COST)
        .saturating_add(RNG_ROUND_BASE_COST);
    (output_len as u64)
        .div_ceil(HKDF_OUTPUT_BLOCK_SIZE as u64)
        .saturating_mul(round_cost)
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
        // cost = 3500 + 120 + 24 * ceil((121 + 33) / 32) = 3740
        assert_eq!(output.gas_used, 3740, "Should consume exactly 3740 gas");
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
        // cost = 3500 + 5 + 120 + 24 * ceil((121 + 4 + 33) / 32) = 3745
        assert_eq!(
            output_with_pers.gas_used, 3745,
            "Should consume exactly 3745 gas"
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
            output_no_pers.gas_used, 3740,
            "Should consume exactly 3740 gas"
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
        // cost = 3500 + 5 + 120 + 24 * ceil((121 + 4 + 33) / 32) = 3745
        assert_eq!(output.gas_used, 3745, "Should consume exactly 3745 gas");
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
            output.gas_used, 3745,
            "Should consume exactly 3745 gas (full cost, no caching)"
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

        // cost = 3500 + 5 + 188 * (120 + 24 * 5) = 48625
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
    fn test_rng_gas_exact_thresholds() {
        // Fixed expected values guard word rounding, zero output, and the chunk
        // counter's effect on every round at the 8160/8161-byte boundary.
        for (pers_len, output_len, expected_gas) in [
            (0, 0, 3500),
            (1, 0, 3505),
            (32, 0, 3505),
            (33, 0, 3510),
            (0, 1, 3740),
            (0, 31, 3740),
            (0, 32, 3740),
            (0, 33, 3980),
            (6, 32, 3745),
            (7, 32, 3769),
            (32, 32, 3769),
            (33, 32, 3774),
            (64, 320, 6390),
            (3, 8160, 64705),
            (3, 8161, 71089),
            (3, 16320, 138145),
            (3, 16321, 138409),
        ] {
            assert_eq!(calculate_gas_cost(pers_len, output_len), expected_gas);
            let (_, input, mut context, precompile) =
                setup_rng_test(output_len as u32, Some(vec![0xAA; pers_len]));
            assert_eq!(
                precompile.1(&mut context, &input, expected_gas - 1),
                Err(PrecompileError::OutOfGas),
                "must fail one gas below the threshold for ({pers_len}, {output_len})"
            );
            let output = precompile.1(&mut context, &input, expected_gas).unwrap();
            assert_eq!(output.gas_used, expected_gas);
            assert_eq!(output.bytes.len(), output_len);
        }
    }

    #[test]
    fn test_rng_personalization_charged_for_every_round() {
        for rounds in [1, 2, 10, 255, 256, 511] {
            // Adding 32 bytes increases the copy cost by 5 and the hashing cost
            // by 24 for each output round, including chunked expansions.
            let output_len = rounds * HKDF_OUTPUT_BLOCK_SIZE;
            assert_eq!(
                calculate_gas_cost(64, output_len) - calculate_gas_cost(32, output_len),
                5 + 24 * rounds as u64
            );
        }
    }

    #[test]
    fn test_rng_rejects_old_underpriced_budget() {
        let pers_len = 65_536;
        let output_len = 65_536;
        let old_gas_cost = 23_980;
        let (_, input, mut context, precompile) =
            setup_rng_test(output_len, Some(vec![0; pers_len]));
        assert_eq!(
            precompile.1(&mut context, &input, old_gas_cost),
            Err(PrecompileError::OutOfGas)
        );
    }

    #[test]
    fn test_rng_rejects_huge_output_before_derivation() {
        // This would reserve almost 4 GiB if the gas check did not run first.
        let (_, input, mut context, precompile) = setup_rng_test(u32::MAX, None);
        assert_eq!(
            precompile.1(&mut context, &input, 6000),
            Err(PrecompileError::OutOfGas)
        );
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
    fn test_rng_cost_monotonic() {
        for pers_len in [0, 3, 6, 7, 32, 1024] {
            let mut prev = calculate_gas_cost(pers_len, 0);
            for output_len in 1..=2 * MAX_HKDF_OUTPUT + 1 {
                let cost = calculate_gas_cost(pers_len, output_len);
                assert!(cost >= prev, "cost decreased at ({pers_len}, {output_len})");
                prev = cost;
            }
        }
        for output_len in [0, 1, 32, 33, MAX_HKDF_OUTPUT, MAX_HKDF_OUTPUT + 1] {
            let mut prev = calculate_gas_cost(0, output_len);
            for pers_len in 1..=1024 {
                let cost = calculate_gas_cost(pers_len, output_len);
                assert!(cost >= prev, "cost decreased at ({pers_len}, {output_len})");
                prev = cost;
            }
        }
    }

    #[test]
    fn test_rng_cost_no_overflow() {
        let init = calculate_init_cost(usize::MAX);
        assert!(
            init >= calculate_init_cost(0),
            "Extreme pers length must not wrap below base cost"
        );

        let fill = calculate_fill_cost(0, usize::MAX);
        assert!(
            fill >= calculate_fill_cost(0, 0),
            "Extreme fill length must not wrap below base cost"
        );
        assert!(calculate_gas_cost(usize::MAX, 0) >= RNG_BASE_COST);
        assert!(calculate_gas_cost(usize::MAX, 1) >= calculate_gas_cost(1024, 1));
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(calculate_gas_cost(usize::MAX, 64), u64::MAX);
            assert_eq!(calculate_gas_cost(0, usize::MAX), u64::MAX);
            assert_eq!(calculate_gas_cost(usize::MAX, usize::MAX), u64::MAX);
        }
    }
}
