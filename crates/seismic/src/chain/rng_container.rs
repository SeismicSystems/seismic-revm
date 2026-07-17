use crate::precompiles::rng::domain_sep_rng::RootRng;
use revm::{
    precompile::PrecompileError,
    primitives::{Bytes, B256},
};

/// Derives random bytes for the RNG precompile.
///
/// Each call is fully stateless: a fresh `RootRng` is constructed from the
/// provided key, domain separation data (parent_block_hash, tx_hash_accumulator,
/// tx_hash, gas_left) is appended, and bytes are derived via HKDF-SHA256.
pub fn derive_rng_output(
    pers: &[u8],
    requested_output_len: usize,
    tx_hash: &B256,
    live_key: [u8; 64],
    parent_block_hash: &B256,
    tx_hash_accumulator: &B256,
    total_gas_remaining: u64,
) -> Result<Bytes, PrecompileError> {
    let mut rng = RootRng::new(live_key);
    rng.append_parent_block_hash(parent_block_hash);
    rng.append_tx_hash_accumulator(tx_hash_accumulator);
    rng.append_tx(tx_hash);
    rng.append_gas_left(total_gas_remaining);
    let rng_bytes = rng.derive_bytes(pers, requested_output_len);
    Ok(Bytes::from(rng_bytes))
}
