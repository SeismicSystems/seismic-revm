use crate::precompiles::rng::domain_sep_rng::RootRng;
use revm::{
    precompile::PrecompileError,
    primitives::{Bytes, B256},
};
use schnorrkel::ExpansionMode;

/// Derives random bytes for the RNG precompile.
///
/// Each call is fully stateless: a fresh `RootRng` is constructed from the
/// provided key, domain separation data (tx_hash, gas_left) is appended,
/// and bytes are derived via HKDF-SHA256.
///
/// - **Execution mode** (`live_key = Some(key)`): uses the enclave-provided key.
///   Deterministic for the same (key, tx_hash, gas_left, pers).
/// - **Simulation mode** (`live_key = None`): generates a random key via `OsRng`.
///   Non-deterministic by design (each call gets a fresh random key).
pub fn derive_rng_output(
    pers: &[u8],
    requested_output_len: usize,
    tx_hash: &B256,
    live_key: Option<schnorrkel::Keypair>,
    total_gas_remaining: u64,
) -> Result<Bytes, PrecompileError> {
    let key = live_key.unwrap_or_else(|| {
        schnorrkel::MiniSecretKey::generate()
            .expand(ExpansionMode::Uniform)
            .into()
    });

    let mut rng = RootRng::new(key);
    rng.append_tx(tx_hash);
    rng.append_gas_left(total_gas_remaining);
    let rng_bytes = rng.derive_bytes(pers, requested_output_len);
    Ok(Bytes::from(rng_bytes))
}
