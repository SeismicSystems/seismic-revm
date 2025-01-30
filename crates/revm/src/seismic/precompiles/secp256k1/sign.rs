use crate::primitives::Bytes;
use crate::seismic::precompiles::SECP256K1_SIGN_ADDRESS;
use crate::precompile::Error as PCError;

use revm_precompile::{
    calc_linear_cost_u32, Precompile, PrecompileError, PrecompileOutput,
    PrecompileResult, PrecompileWithAddress,
};
use secp256k1::Secp256k1;


/* --------------------------------------------------------------------------
Precompile Wiring
-------------------------------------------------------------------------- */

pub const PRECOMPILE: PrecompileWithAddress = PrecompileWithAddress(
    SECP256K1_SIGN_ADDRESS,
    Precompile::Standard(secp256k1_sign_ecdsa_recoverable),
);

/* --------------------------------------------------------------------------
Precompile Logic and Gas Calculation
-------------------------------------------------------------------------- */
/// TODO: explain gas reasoning

// takes in a secret key and a digest and returns a signature
pub fn secp256k1_sign_ecdsa_recoverable(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    let gas_used = 100; // TODO: refine this constant
    if gas_used > gas_limit {
        return Err(PCError::OutOfGas.into());
    }

    // input validation
    if input.len() != 64 {
        return Err(PCError::Other("Invalid input length".to_string()).into());
    }
    let key_bytes: [u8; 32] = input[0..32].try_into().unwrap();
    let digest_bytes: [u8; 32] = input[32..64].try_into().unwrap();
    let secret_key = secp256k1::SecretKey::from_slice(&key_bytes).map_err(
        |e| PCError::Other(format!("Invalid secret key: {e}")),
    )?;
    let message = secp256k1::Message::from_digest_slice(&digest_bytes).map_err(
        |e| PCError::Other(format!("Invalid message: {e}")),
    )?;
    
    // sign
    let secp = Secp256k1::new();
    let sig = secp.sign_ecdsa_recoverable(&message, &secret_key);
    
    // serialize the output
    let (recid, sig) = sig.serialize_compact();
    let mut output = sig.to_vec();
    output.push(recid.to_i32() as u8);

    Ok(PrecompileOutput::new(gas_used, output.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{alloy_primitives::B512, Bytes, B256, keccak256};
    use crate::precompile::secp256k1::ecrecover;

    use revm_precompile::{PrecompileError, PrecompileErrors};
    use sha2::{Digest, Sha256};
    
    #[test]
    fn test_ecrecover_on_sig() {
        let full_message = "1234567890abcdef1234567890abcdef";
        let message: [u8; 32] = keccak256(full_message.as_bytes()).into();
        let sk_bytes: [u8; 32] = [0x1; 32];
        let sk = secp256k1::SecretKey::from_slice(&sk_bytes).unwrap();
        println!("sk created successfully");

        let pk: secp256k1::PublicKey = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);
        let mut hash = keccak256(&pk.serialize_uncompressed()[1..]);
        hash[..12].fill(0);

        let mut input = sk_bytes.to_vec();
        input.extend_from_slice(&message);
        let gas_limit = 2000;
        let output = secp256k1_sign_ecdsa_recoverable(&Bytes::from(input), gas_limit).unwrap().bytes;

        let sig: B512 = output[0..64].try_into().unwrap();
        let recid: u8 = output[64];
        let msg: B256 = message.try_into().unwrap();
        let recovered_key = ecrecover(&sig, recid, &msg).unwrap();
        assert_eq!(recovered_key, hash);
    }
}