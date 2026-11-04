use revm::precompile::PrecompileError;

/// The below gas cost are very rough estimates.
/// Overhead cost for AES-GCM setup & finalization. We intentionally overprice to stay safe.
const AES_GCM_BASE: u64 = 1000;

/// Per 16-byte block cost. One AES encryption + one GHASH multiply per block, plus cushion.
const AES_GCM_PER_BLOCK: u64 = 30;

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

pub(crate) fn parse_aes_key(slice: &[u8]) -> Result<[u8; 32], PrecompileError> {
    slice
        .try_into()
        .map_err(|_| PrecompileError::Other("invalid key length (must be 32 bytes)".to_string()))
}

pub(crate) fn validate_nonce_length(slice: &[u8]) -> Result<(), PrecompileError> {
    if slice.len() != 12 {
        return Err(PrecompileError::Other(
            "Invalid nonce length: expected 12 bytes".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn calculate_cost(ciphertext_len: usize) -> u64 {
    calc_linear_cost(16, ciphertext_len, AES_GCM_BASE, AES_GCM_PER_BLOCK)
}

fn calc_linear_cost(bus: u64, len: usize, base: u64, word: u64) -> u64 {
    (len as u64).div_ceil(bus) * word + base
}

pub(crate) fn validate_gas_limit(cost: u64, gas_limit: u64) -> Result<(), PrecompileError> {
    if cost > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }
    Ok(())
}
