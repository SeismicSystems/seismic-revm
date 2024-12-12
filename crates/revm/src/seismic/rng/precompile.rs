use super::domain_sep_rng::RootRng;
use super::rng_env::RngEnv;
use crate::primitives::{Bytes, Env, TxEnv};
use alloy_primitives::{keccak256, B256};
use alloy_rlp::encode;

use rand_core::RngCore;
use revm_precompile::{
    u64_to_address, Error as REVM_ERROR, Precompile, PrecompileError, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};

pub const RNG_PRECOMPILE: PrecompileWithAddress =
    PrecompileWithAddress(u64_to_address(100), Precompile::Env(run));

pub fn run(input: &Bytes, gas_limit: u64, env: &Env) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    let tx_env_hash = hash_tx_env(&env.tx);
    let rng_env = RngEnv::new(env.block.number, tx_env_hash);
    let pers = input.as_ref(); // pers is the personalized entropy added by the caller

    // Get the random bytes
    // TODO: Root rng goes in Env, fork, then append tx hash
    let root_rng = RootRng::new();
    let mut leaf_rng = match root_rng.fork(rng_env, pers.as_ref()) {
        Ok(rng) => rng,
        Err(_err) => {
            return Err(PrecompileError::Other("Rng fork failed".to_string()).into());
        }
    };

    let mut rng_bytes = [0u8; 32];
    leaf_rng.fill_bytes(&mut rng_bytes);
    let output = Bytes::from(rng_bytes);

    Ok(PrecompileOutput::new(gas_used, output))
}

// Computes the hash of the transaction fields
// This will not be equal to the hash of the transaction itself
// because the TxEnv does not contain the signature fields
fn hash_tx_env(tx_env: &TxEnv) -> B256 {
    // RLP encode the transaction fields and concatenate them
    let mut tx_bytes = Vec::new();
    tx_bytes.extend_from_slice(&encode(tx_env.caller));
    tx_bytes.extend_from_slice(&encode(tx_env.gas_limit));
    tx_bytes.extend_from_slice(&encode(tx_env.gas_price));
    tx_bytes.extend_from_slice(&encode(&tx_env.transact_to));
    tx_bytes.extend_from_slice(&encode(tx_env.value));
    tx_bytes.extend_from_slice(&encode(tx_env.data.clone()));

    // Compute Keccak-256 of the RLP-encoded bytes
    let hash = keccak256(&tx_bytes);

    // Convert to B256
    B256::from(hash)
}
