use revm::{
    context::ContextTr, precompile::{calc_linear_cost_u32, u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult}, primitives::Bytes
};

use crate::{api::exec::SeismicContextTr, precompiles::stateful_precompile::StatefulPrecompileWithAddress, transaction::abstraction::SeismicTxTr};

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

// The RNG precompile is a stateful precompile based on Merlin transcripts
// At each transaction in a block executes, the tx hash is appended to
// the transcript as domain seperation, causing identical transactions
// to produce different randomness
pub const RNG_ADDRESS: u64 = 100; // Hex address `0x64`.

pub fn rng_precompile_iter<CTX: SeismicContextTr>() -> impl Iterator<Item = StatefulPrecompileWithAddress<CTX>> {
    [rng_precompile::<CTX>()].into_iter()
}

pub fn rng_precompile<CTX: SeismicContextTr>() -> StatefulPrecompileWithAddress<CTX> {
    StatefulPrecompileWithAddress(u64_to_address(RNG_ADDRESS), rng::<CTX>)
}

const MIN_INPUT_LENGTH: usize = 2;
const RNG_INIT_BASE: u64 = 3500;
const STROBE128WORD: u64 = 5;

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
/// for the RNG entropy.
///
/// Using the pers bytes, the block rng transcript, and the block VRF key,
/// we produce a leaf RNG that implements the `RngCore` interface and query
/// it for bytes.
///
/// ## Gas Cost
///
/// ### Pricing Fundamental Operations
/// The RNG precompile uses Merlin transcripts that rely on the Strobe128 hash function.
/// Strobe uses the keccak256 sponge, which has an EVM opcode cost of
/// `g=30+6×ceil(input size/32)`. We have a more complex initialization than SHA,
/// so we price a base cost of 100 gas. However, Strobe128 is designed for 128-bit security
/// (instead of SHA3's 256-bit security), which allows it to work faster. The dominating cost
/// for the keccak256 sponges is the keccak256 permutation. For SHA3, you permute
/// once per 136 bytes of data absorbed. Ethereum simplifies this cost calculation as
/// 6 gas per 32-byte word absorbed. Strobe128, on the other hand,
/// can absorb/squeeze 166 bytes before it needs to run the keccak256 permutation.
/// `136 / 166 * 6 ≈ 4.9`, which we round up to 5 gas, instead of 6 gas per word.
///
/// The transcripts also use points on the Ristretto group for Curve25519 and require
/// scalar multiplications. Scalar multiplication is optimized through the use of the
/// Montgomery ladder for Curve25519, so this should be as fast or faster than
/// a Secp256k1 scalar multiplication. Benchmarks by XRLP support this:
/// <https://xrpl.org/blog/2014/curves-with-a-twist>
/// We bound the cost at that of ecrecover, which performs 3 secp256k1
/// scalar multiplications, a point addition, plus some other computation.
/// Charging the same amount as ecrecover (3000 gas) is very conservative
/// but allows us to lower the cost later on.
///
/// ### Pricing RNG Operations
/// The cost of initializing the `leaf_rng` comes from:
///
/// * The Root RNG initialization requires a running hash of the transcript. The Root RNG  
///   is initialized by adding 13 bytes to the transcript and then keying the rng  
///   (essentially hashing) using Strobe128.
///
/// * (optional) If personalization bytes are provided, the RNG is seeded with  
///   those pers bytes
///
/// * Each leaf RNG requires forking the `root_rng`, which involves adding  
///   a 32-byte `tx_hash` and label (2 bytes) per transaction. Then a separate  
///   VRF hash function is used that performs a single EC scalar multiplication
///
/// * The leaf RNG is initialized, which involves keying the RNG based on 32 random bytes  
///   from the parent RNG.
///
/// **Filling bytes** once the RNG is initialized:
///
/// * Filling bytes occurs by squeezing the keccak sponge. As described above,  
///   take inspiration from Ethereum and charge 5 gas per 32-byte word to account for the  
///   cheaper Strobe parameters.
///
/// To calculate the base init cost of the RNG precompile, we get:
/// - 100 gas from setting up Strobe128  
/// - `(13 + len(pers) + 32 + 2 + 32) * 5 = 395 + 5 * len(pers)` gas for hashing init bytes  
/// - 3000 gas for the EC scalar multiplication  
///
/// We add a 50% buffer to our gas calculations (which may be lowered in the future).
///
/// ```text
/// RNG_INIT_BASE = round(100 + 395 + 3000) = 3500
/// fill_cost     = ceil(fill_len / 32) * 5
/// ```
fn rng<CTX: SeismicContextTr>(
    evmctx: &mut CTX,
    input: &Bytes,
    gas_limit: u64,
) -> PrecompileResult {
    // Validate input and extract parameters.
    validate_input_length(input.len(), MIN_INPUT_LENGTH)?;
    let (requested_output_len, pers) = parse_input(input)?;
    let requested_output_len = requested_output_len as usize;
    
    // Compute the gas cost.
    let gas_used = evmctx
        .chain()
        .calculate_gas_cost(&pers, requested_output_len);
    if gas_used > gas_limit {
        return Err(PrecompileError::OutOfGas); // Changed REVM_ERROR to PrecompileError
    }
    
    // Obtain kernel mode and transaction hash.
    let kernel_mode = evmctx.tx().rng_mode();
    let tx_hash = evmctx.tx().tx_hash();
    
    // Let the container update its state and produce the random bytes.
    let output = evmctx
        .chain()
        .process_rng(&pers, requested_output_len, kernel_mode, &tx_hash)
        .map_err(|e| PrecompileError::Other(e.to_string()))?; // Changed PCError to PrecompileError
    
    Ok(PrecompileOutput::new(gas_used, output))
}

pub(crate) fn calculate_init_cost(pers_len: usize) -> u64 {
    calc_linear_cost_u32(pers_len, RNG_INIT_BASE, STROBE128WORD)
}

pub(crate) fn calculate_fill_cost(fill_len: usize) -> u64 {
    calc_linear_cost_u32(fill_len, 0, STROBE128WORD)
}

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
    use std::vec;
    use crate::transaction::abstraction::SeismicTransaction;
    use crate::{SeismicContext, DefaultSeismic};

    use super::*;
    use revm::database::EmptyDB;
    use revm::primitives::{B256, Bytes};
    use revm::precompile::PrecompileError;
    use revm::Context;

    fn setup_rng_test(bytes_requested: u32, personalization: Option<Vec<u8>>) -> (u64, Bytes, SeismicContext<EmptyDB>, StatefulPrecompileWithAddress<SeismicContext<EmptyDB>>) {
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
        let context = Context::seismic().with_tx(tx);
        
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
        assert_eq!(output.gas_used, 3505, "Should consume exactly 3505 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_init_00_pers_different_than_no_pers() {
        // Test with explicit zero personalization
        let empty_pers = vec![0, 0, 0, 0]; // U32::ZERO.to_be_bytes_vec()
        let (gas_limit, input_with_pers, mut context_with_pers, precompile) = setup_rng_test(32, Some(empty_pers));

        let result_with_pers = precompile.1(&mut context_with_pers, &input_with_pers.into(), gas_limit);
        assert!(
            result_with_pers.is_ok(),
            "Should succeed with default personalization"
        );

        let output_with_pers = result_with_pers.unwrap();
        assert_eq!(output_with_pers.gas_used, 3510, "Should consume exactly 3510 gas");
        assert!(output_with_pers.bytes.len() == 32, "RNG output should be 32 bytes");

        // Test without personalization
        let (gas_limit, input_no_pers, mut context_no_pers, precompile) = setup_rng_test(32, None);

        let result_no_pers = precompile.1(&mut context_no_pers, &input_no_pers.into(), gas_limit);
        assert!(
            result_no_pers.is_ok(),
            "Should succeed without default personalization"
        );

        let output_no_pers = result_no_pers.unwrap();
        assert_eq!(output_no_pers.gas_used, 3505, "Should consume exactly 3505 gas");
        assert!(output_no_pers.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_init_with_pers() {
        let personalization = vec![1, 2, 3, 4]; // use 4 pers bytes, gets rounded up to one word
        let (gas_limit, input, mut context, precompile) = setup_rng_test(32, Some(personalization));

        let result = precompile.1(&mut context, &input.into(), gas_limit);
        assert!(result.is_ok(), "Should succeed with personalization");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 3510, "Should consume exactly 3510 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_already_initialized() {
        let empty_pers = vec![0, 0, 0, 0]; // U32::ZERO.to_be_bytes_vec()
        let (_, input, mut context, precompile) = setup_rng_test(32, Some(empty_pers));

        // Call once to initialize the RNG
        let _ = precompile.1(&mut context, &input.clone().into(), 6000);

        // Make a second call with the leaf rng already initialized
        let reduced_gas_limit = 500;
        let result = precompile.1(&mut context, &input.into(), reduced_gas_limit);
        assert!(result.is_ok(), "Should succeed with initialized RNG");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 5, "Should consume exactly 5 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_out_of_gas_on_init() {
        let empty_pers = vec![0, 0, 0, 0]; // U32::ZERO.to_be_bytes_vec()
        let (_, input, mut context, precompile) = setup_rng_test(16, Some(empty_pers));
        
        let insufficient_gas = 2500; // less than the init cost
        let result = precompile.1(&mut context, &input.into(), insufficient_gas);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_rng_out_of_gas_on_fill() {
        let empty_pers = vec![0, 0, 0, 0]; // U32::ZERO.to_be_bytes_vec()
        let (_, input, mut context, precompile) = setup_rng_test(6000, Some(empty_pers));
        
        // Call once to initialize the RNG
        let _ = precompile.1(&mut context, &input.clone().into(), 6000);

        // Make a second call with the leaf rng already initialized
        let insufficient_gas = 100;
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
        
        // Use our setup function to get the context and precompile
        // We can use dummy values for bytes_requested and personalization since we'll override the input
        let (gas_limit, _, mut context, precompile) = setup_rng_test(0, None);
        
        let result = precompile.1(&mut context, &input.into(), gas_limit);
        assert!(result.is_err());

        // We expect a PCError::Other complaining about input length
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
}
