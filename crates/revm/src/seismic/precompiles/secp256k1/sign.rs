use crate::precompile::Error as PCError;
use crate::primitives::Bytes;
use crate::seismic::precompiles::SECP256K1_SIGN_ADDRESS;

use revm_precompile::{
    Precompile, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
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
    let gas_used = 3000;
    if gas_used > gas_limit {
        return Err(PCError::OutOfGas.into());
    }

    // input validation
    if input.len() != 64 {
        return Err(PCError::Other("Invalid input length".to_string()).into());
    }
    let key_bytes: [u8; 32] = input[0..32].try_into().unwrap();
    let digest_bytes: [u8; 32] = input[32..64].try_into().unwrap();
    let secret_key = secp256k1::SecretKey::from_slice(&key_bytes)
        .map_err(|e| PCError::Other(format!("Invalid secret key: {e}")))?;
    let message = secp256k1::Message::from_digest_slice(&digest_bytes)
        .map_err(|e| PCError::Other(format!("Invalid message: {e}")))?;

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
    use crate::precompile::secp256k1::ecrecover;
    use crate::primitives::{alloy_primitives::B512, keccak256, Bytes, B256};

    use revm_precompile::{PrecompileError, PrecompileErrors};
    use secp256k1::{
        ecdsa::Signature,
        Message,
    };

    #[test]
    fn test_verify() {
        let full_message = "1234567890abcdef1234567890abcdef";
        let message: [u8; 32] = keccak256(full_message.as_bytes()).into();
        let sk_bytes: [u8; 32] = [0x1; 32];
        let sk = secp256k1::SecretKey::from_slice(&sk_bytes).unwrap();

        let mut input = sk_bytes.to_vec();
        input.extend_from_slice(&message);
        let gas_limit = 4000;
        let output = secp256k1_sign_ecdsa_recoverable(&Bytes::from(input), gas_limit)
            .unwrap()
            .bytes;

        let sig: [u8; 64] = output[0..64].try_into().unwrap();
        let secp = Secp256k1::verification_only();
        let pk: secp256k1::PublicKey =
            secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);
        assert!(secp
            .verify_ecdsa(
                &Message::from_digest_slice(&message).unwrap(),
                &Signature::from_compact(&sig).unwrap(),
                &pk
            )
            .is_ok());
    }

    #[test]
    fn test_eth_ecrecover() {
        let full_message = "1234567890abcdef1234567890abcdef";
        let message: [u8; 32] = keccak256(full_message.as_bytes()).into();
        let sk_bytes: [u8; 32] = [0x1; 32];
        let sk = secp256k1::SecretKey::from_slice(&sk_bytes).unwrap();

        let pk: secp256k1::PublicKey =
            secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::signing_only(), &sk);
        let mut pk_addr = keccak256(&pk.serialize_uncompressed()[1..]);
        pk_addr[..12].fill(0);

        let mut input = sk_bytes.to_vec();
        input.extend_from_slice(&message);
        let gas_limit = 4000;
        let output = secp256k1_sign_ecdsa_recoverable(&Bytes::from(input), gas_limit)
            .unwrap()
            .bytes;

        let sig: B512 = output[0..64].try_into().unwrap();
        let recid: u8 = output[64];
        let msg: B256 = message.try_into().unwrap();
        let recovered_addr = ecrecover(&sig, recid, &msg).unwrap();
        assert_eq!(recovered_addr, pk_addr);
    }

    #[test]
    fn test_invalid_input_length() {
        let input = Bytes::from("short_bytes");
        let gas_limit = 4000;
        let result = secp256k1_sign_ecdsa_recoverable(&input, gas_limit);
        assert!(result.is_err());
        match result.err() {
            Some(PrecompileErrors::Error(PCError::Other(msg))) => {
                assert_eq!(msg, "Invalid input length");
            }
            other => panic!("Expected PCError::Other(Invalid input length), got: {other:?}"),
        }
    }

    #[test]
    fn test_invalid_secret_key() {
        let input = Bytes::from([0u8; 64]);
        let gas_limit = 4000;
        let result = secp256k1_sign_ecdsa_recoverable(&input, gas_limit);
        assert!(result.is_err());
        match result.err() {
            Some(PrecompileErrors::Error(PCError::Other(msg))) => {
                assert_eq!(msg, "Invalid secret key: malformed or out-of-range secret key");
            }
            other => panic!("Expected PCError::Other(Invalid secret key: malformed or out-of-range secret key), got: {other:?}"),
        }
    }

    #[test]
    fn test_out_of_gas() {
        let full_message = "1234567890abcdef1234567890abcdef";
        let message: [u8; 32] = keccak256(full_message.as_bytes()).into();
        let sk_bytes: [u8; 32] = [0x1; 32];

        let mut input = sk_bytes.to_vec();
        input.extend_from_slice(&message);
        let small_gas_limit = 500; // well below 1180

        let result = secp256k1_sign_ecdsa_recoverable(&Bytes::from(input), small_gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileErrors::Error(PrecompileError::OutOfGas)) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }
}
