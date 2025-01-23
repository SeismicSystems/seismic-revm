use crate::precompile::PrecompileError;
use crate::{
    primitives::{db::Database, Address, Bytes},
    ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext,
};
use std::sync::Arc;

use crate::precompile::Error as PCError;
use rand_core::RngCore;
use revm_precompile::{
    calc_linear_cost_u32, u64_to_address, Error as REVM_ERROR, PrecompileOutput, PrecompileResult,
};

use crate::seismic::rng::LeafRng;

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */
pub struct RngPrecompile;

// Register the RNG precompile at `0x64`.
pub const ADDRESS: Address = u64_to_address(100);

// The RNG precompile is a stateful precompile based on Merlin transcripts
// At each transaction in a block executes, the tx hash is appended to
// the transcript as domain seperation, causing identical transactions
// to produce different randomness
impl RngPrecompile {
    pub fn address_and_precompile<DB: Database>() -> (Address, ContextPrecompile<DB>) {
        (
            ADDRESS,
            ContextPrecompile::ContextStateful(Arc::new(RngPrecompile)),
        )
    }
}

/* --------------------------------------------------------------------------
Precompile Logic
-------------------------------------------------------------------------- */
/// # RNG Precompile
///
/// ## Overview
/// We interpret the input as a [u8] slice of bytes used as personalization
/// for the RNG entropy.
///
/// Using the pers bytes, the block rng transcript, and the block VRF key,
/// we produce a leaf RNG that impliments the RngCore interface and query
/// it for bytes.
///
/// ## Gas Cost
/// ### Pricing Fundamental Operations
/// The RNG precompile uses Merlin transcripts that rely on the Strobe128 hash function.
/// Strobe uses the keccak256 sponge, which has an evm opcode cost of
/// g=30+6×ceil(input size/32). We have a more complex initialization than SHA,
/// so we price a base cost of 100 gas. However, Strobe128 is designed for 128 bit security
/// insead of SHA3's 256 bit security, which allows it to work faster. The dominating cost
/// for the keccak256 sponges is the keccak256 permutation. For SHA3, you permute
/// once per 136 bytes of data absorbed. Ethereum simplifies this cost calculation as
/// 6 bytes per word absorbed, where a word is 32 bytes. Strobe128, on the other hand,
/// can absorb/sqeeze 166 bytes before it needs to run the keccak256 permutation.
/// 136 / 166 * 6 = 4.9, which we round up to 5 gas, instead of 6 gas per word.
///
/// The transcripts also use points on the Ristretto group for Curve25519, and require
/// scalar multiplications. Scalar multiplication is optimized through the use of the
/// Montgomery ladder for Curve25519, so this should be as fast or faster than
/// a Secp256k1 scalar multiplication. Benchmarks by XRLP support this:  https://xrpl.org/blog/2014/curves-with-a-twist
/// We bound the cost at that of ecrecover, which performs 3 scep256k1
/// scalar multiplications, a point addition, as well as some other computation.
/// Charging the same amount as ecrecover, i.e.3000 gas, very conservative,
/// but allows us to lower the cost later on.
///
/// ### Pricing RNG Operations
/// The cost of the initializing the leaf_rng comes from the following:
/// * The Root RNG initialization requires a running hash of the transcript. The Root RNG
/// is initialized by adding 13 bytes to the transcript and then keying the rng
/// (essentially hashing) using Strobe128.
/// * (optional) if personalization bytes are provided, the RNG is seeded with
/// those pers bytes
/// * Each leaf rng requires forking the root_rng, which involves adding
/// a 32 byte tx_hash and label 2 bytes per transaction. Then a seperate
/// VRF Hash function is used that performs a single EC scalar multiplication
/// * The leaf RNG is initialized, which involves keying the rng based on 32 random bytes
/// from the parent RNG.
///
/// Filling bytes once the rng is initialized.
/// * Filling bytes occurs by squeezing the keccak sponge. As described above,
/// take inspiration from ethereum and charge 5 bytes per word to account for the
/// cheaper Strobe parameters.
///
/// To calculate the base init cost of the RNG precompile, we get:
/// 100 gas from setting up Strobe128
/// (13 + len(pers) + 32 + 2 + 32) * 5 = 79*5 + len(pers) * 5  = 395 gas for hashing init root_rng bytes
/// 3000 gas for the EC scalar multiplication
/// We add a 50 percent buffer to our gas calculations, which may be lowered in the future
///
/// RNG_INIT_BASE = Round(100 + 395 + 3000) = 3500
/// fill_cost = ceil(fill_len/32)*5

const MIN_INPUT_LENGTH: usize = 2;
const RNG_INIT_BASE: u64 = 3500;
const STROBE128WORD: u64 = 5;

impl<DB: Database> ContextStatefulPrecompile<DB> for RngPrecompile {
    fn call(
        &self,
        input: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        validate_input_length(input.len(), MIN_INPUT_LENGTH.into())?;
        let (requested_output_len, pers) = parse_input(input)?;

        let gas_used = match evmctx.kernel.leaf_rng_mut_ref() {
            Some(_) => calculate_fill_cost(requested_output_len.into()),
            None => {
                calculate_init_cost(pers.len()) + calculate_fill_cost(requested_output_len.into())
            }
        };

        if gas_used > gas_limit {
            return Err(REVM_ERROR::OutOfGas.into());
        }

        // append to root_tx for domain separation
        evmctx.kernel.maybe_append_entropy();
        let tx_hash = evmctx.env().tx.tx_hash;
        let rng = evmctx.kernel.root_rng_mut_ref();
        rng.append_tx(&tx_hash);

        // if the leaf rng is not initialized, initialize it
        if evmctx.kernel.leaf_rng_mut_ref().is_none() {
            let leaf_rng =
                get_leaf_rng(&pers, evmctx).map_err(|e| PCError::Other(e.to_string()))?;
            evmctx.kernel.leaf_rng_mut_ref().replace(leaf_rng);
        }

        // Get the random bytes
        let leaf_rng = evmctx.kernel.leaf_rng_mut_ref().as_mut().unwrap();
        let mut rng_bytes = vec![0u8; requested_output_len.into()];
        leaf_rng.fill_bytes(&mut rng_bytes);
        let output = Bytes::from(rng_bytes);

        Ok(PrecompileOutput::new(gas_used, output))
    }
}

pub fn get_leaf_rng<DB: Database>(
    input: &Bytes,
    evmctx: &mut InnerEvmContext<DB>,
) -> Result<LeafRng, anyhow::Error> {
    let pers = input.as_ref(); // pers is the personalized entropy added by the caller
    let root_rng = &mut evmctx.kernel.root_rng_mut_ref();
    let leaf_rng = root_rng.fork(pers);
    Ok(leaf_rng)
}

pub(crate) fn calculate_init_cost(pers_len: usize) -> u64 {
    calc_linear_cost_u32(pers_len, RNG_INIT_BASE, STROBE128WORD)
}

pub(crate) fn calculate_fill_cost(fill_len: usize) -> u64 {
    calc_linear_cost_u32(fill_len, 0, STROBE128WORD)
}

pub(crate) fn parse_input(input: &Bytes) -> Result<(u16, Bytes), PrecompileError> {
    let output_len_bytes: [u8; 2] = input[0..2].try_into().unwrap();
    let requested_output_len: u16 = u16::from_be_bytes(output_len_bytes); // first 2 bytes are the output length
    let pers = Bytes::from(input[2..].to_vec());
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

    use super::*;
    use crate::db::EmptyDB;
    use crate::precompile::PrecompileError;
    use crate::precompile::PrecompileErrors;
    use alloy_primitives::B256;

    #[test]
    fn test_rng_init_no_pers() {
        let gas_limit = 6000;
        let input = Bytes::from(32u16.to_be_bytes()); // request 32 bytes, no pers
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        evmctx.env().tx.tx_hash = B256::from([0u8; 32]);
        let precompile = RngPrecompile;

        let result = precompile.call(&input, gas_limit, &mut evmctx);
        println!("{:?}", result);
        assert!(result.is_ok(), "Should succeed without personalization");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 3505, "Should consume exactly 3505 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_init_with_pers() {
        let gas_limit = 6000;
        let mut input = 32u16.to_be_bytes().to_vec();
        input.extend(vec![1, 2, 3, 4]); // use 4 pers bytes, gets rounted up to one word
        let input = Bytes::from(Bytes::from(input));
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        let precompile = RngPrecompile;

        let result = precompile.call(&input, gas_limit, &mut evmctx);
        assert!(result.is_ok(), "Should succeed with personalization");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 3510, "Should consume exactly 3510 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_already_initialized() {
        let gas_limit = 500;
        let input = Bytes::from(32u16.to_be_bytes());
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        let precompile = RngPrecompile;

        // call once to initialize the RNG
        let _ = precompile.call(&input, 6000, &mut evmctx);

        // make a second call with the leaf rng already initialized
        let result = precompile.call(&input, gas_limit, &mut evmctx);
        assert!(result.is_ok(), "Should succeed with initialized RNG");

        let output = result.unwrap();
        assert_eq!(output.gas_used, 5, "Should consume exactly 5 gas");
        assert!(output.bytes.len() == 32, "RNG output should be 32 bytes");
    }

    #[test]
    fn test_rng_out_of_gas_on_init() {
        let gas_limit = 2500; // less than the init cost
        let input = Bytes::from(32u16.to_be_bytes());
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        let precompile = RngPrecompile;

        let result = precompile.call(&input, gas_limit, &mut evmctx);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_rng_out_of_gas_on_fill() {
        let gas_limit = 100;
        let input = Bytes::from(6000u16.to_be_bytes()); // request too many bytes
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        let precompile = RngPrecompile;

        // call once to initialize the RNG
        let _ = precompile.call(&input, 6000, &mut evmctx);

        // make a second call with the leaf rng already initialized
        let result = precompile.call(&input, gas_limit, &mut evmctx);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }

    #[test]
    fn test_invalid_input_length() {
        let gas_limit = 6000;
        let input = Bytes::from(32u8.to_be_bytes());
        let mut evmctx = InnerEvmContext::new(EmptyDB::default());
        let precompile = RngPrecompile;

        let result = precompile.call(&input, gas_limit, &mut evmctx);
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
