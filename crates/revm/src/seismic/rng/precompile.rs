use crate::{
    primitives::{db::Database, Address, Bytes},
    ContextPrecompile, ContextStatefulPrecompile, InnerEvmContext,
};
use std::sync::Arc;

use crate::precompile::Error as PCError;
use rand_core::RngCore;
use revm_precompile::{u64_to_address, Error as REVM_ERROR, PrecompileOutput, PrecompileResult};

use super::domain_sep_rng::LeafRng;

/* --------------------------------------------------------------------------
Constants & Setup
-------------------------------------------------------------------------- */

/// On-chain address for the RNG precompile. Adjust as desired.
pub const ADDRESS: Address = u64_to_address(100);

pub struct RngPrecompile;

// Register the RNG precompile at `0x100`.
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
/// g=30+6×ceil(input size/32). Strobe128 has a more complex initialization than SHA,
/// so we price a base cost of 100 gas plus 6×ceil(input size/32). 
/// The transcripts also use points on the Ristretto group for Curve25519, and require
/// scalar multiplications. These EC operations are 2-3x more efficient than BLS12-381 
/// operations, so we price it at g = 6000, half the cost of a BLS12-381 G1 operation.
/// 
/// ### Pricing RNG Operations
/// The cost of the RNG comes from the following:
/// * The RNG initialization requires a running hash of the transcript using strobe128.
/// where a 32 byte tx_hash and label 2 bytes are added per transaction. 
/// * A seperate VRF Hash function that performs a single EC scalar multiplication 
/// is used whenever the RNG is forked as domain seperation
/// * The Root rng is initialized, which involves adding 13 bytes to the transcript 
/// and then keying the rng (essentially hashing)
/// * The leaf RNG is initialized, which involves keying the rng based on 32 random bytes
/// from the parent RNG. 
/// * Filling bytes once the rng is initialized. This requires the squeeze operation,
/// which is just copying bytes since we currently restring the rng request to 32 bytes
/// 
/// This is 100 gas from setting up Strobe128
/// 79 bytes of hashing to initialize the RNG. 79 * 6 = 474 gas
/// 6000 gas for the EC scalar multiplication
/// We add a 50 percent buffer to our gas calculations, which may be lowered in the future
/// (100 + 474 + 6000) * 1.25 = 9800 gas
/// 
/// TODO: add cost of pers bytes. Perhaps force this to be a bytes32 if used?
/// TODO: add a way to request a longer output than 32 bytes for efficiency
/// TODO: TBD if root rng needs to be initialized for every transaction
impl<DB: Database> ContextStatefulPrecompile<DB> for RngPrecompile {
    fn call(
        &self,
        input: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<DB>,
    ) -> PrecompileResult {
        let gas_used = 9800;
        if gas_used > gas_limit {
            return Err(REVM_ERROR::OutOfGas.into());
        }

        // Get the random bytes
        let mut leaf_rng =
            get_leaf_rng(input, evmctx).map_err(|e| PCError::Other(e.to_string()))?;
        let mut rng_bytes = [0u8; 32];
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
    let eph_rng_keypair = evmctx.kernel.get_eph_rng_keypair();
    let root_rng = &mut evmctx.kernel.rng_mut_ref();
    let leaf_rng = root_rng.fork(&eph_rng_keypair, pers);
    Ok(leaf_rng)
}
