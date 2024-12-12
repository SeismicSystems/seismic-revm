// TODO: evaluate if these hashing methods are well designed
use crate::primitives::{TxEnv, BlockEnv};
use alloy_primitives::{keccak256, B256};
use alloy_rlp::encode;

// Computes the hash of the transaction fields
// This will not be equal to the hash of the transaction itself
// because the TxEnv does not contain the signature fields
pub fn hash_tx_env(tx_env: &TxEnv) -> B256 {
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

pub fn hash_block_env(block_env: &BlockEnv) -> B256 {
    let mut block_bytes = Vec::new();
    block_bytes.extend_from_slice(&encode(block_env.number));
    block_bytes.extend_from_slice(&encode(block_env.coinbase));
    block_bytes.extend_from_slice(&encode(block_env.timestamp));
    block_bytes.extend_from_slice(&encode(block_env.gas_limit));
    block_bytes.extend_from_slice(&encode(&block_env.basefee));
    block_bytes.extend_from_slice(&encode(block_env.difficulty));
    if let Some(ref prevrandao) = block_env.prevrandao {
        block_bytes.extend_from_slice(&encode(prevrandao));
    }

    // Compute Keccak-256 of the RLP-encoded bytes
    keccak256(&block_bytes)
}