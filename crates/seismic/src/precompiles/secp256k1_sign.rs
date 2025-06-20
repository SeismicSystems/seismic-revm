use revm::precompile::{
    u64_to_address, PrecompileError, PrecompileOutput, PrecompileResult, PrecompileWithAddress,
};

use secp256k1::Secp256k1;

/* --------------------------------------------------------------------------
Precompile Wiring
-------------------------------------------------------------------------- */
/// Address of SECP256K1 sign precompile.
pub const SECP256K1_SIGN_ADDRESS: u64 = 105;

/// Returns the ecdh precompile with its address.
pub fn precompiles() -> impl Iterator<Item = PrecompileWithAddress> {
    [SECP256K1_SIGN].into_iter()
}

pub const SECP256K1_SIGN: PrecompileWithAddress = PrecompileWithAddress(
    u64_to_address(SECP256K1_SIGN_ADDRESS),
    secp256k1_sign_ecdsa_recoverable,
);

const BASE_GAS: u64 = 3000;

/* --------------------------------------------------------------------------
Precompile Logic and Gas Calculation
-------------------------------------------------------------------------- */
/// We give signing the same gas cost as Ethereum's ecrecover precompile,
/// which we expect is slightly conservative.
///
/// Recovering a public key from a signature requires one scalar inverse, an ecmult, a field square root,
/// two scalar mul, and some other operations that take very little time relatively. Ecrecover must
/// recover the public key, plus do some keccak hashing to recover the address.
/// In comparision, signing is one scalar inverse, an ecmult, two scalar mul, and some other operations
/// Notably, it does not require any square roots or keccak hashing, so it should be slight less gas-expensive.
pub fn secp256k1_sign_ecdsa_recoverable(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let gas_used = BASE_GAS;
    if gas_used > gas_limit {
        return Err(PrecompileError::OutOfGas.into());
    }

    // input validation
    if input.len() != 64 {
        return Err(PrecompileError::Other("Invalid input length".to_string()).into());
    }
    let key_bytes: [u8; 32] = input[0..32].try_into().unwrap();
    let digest_bytes: [u8; 32] = input[32..64].try_into().unwrap();
    let secret_key = secp256k1::SecretKey::from_slice(&key_bytes)
        .map_err(|e| PrecompileError::Other(format!("Invalid secret key: {e}")))?;
    let message = secp256k1::Message::from_digest_slice(&digest_bytes)
        .map_err(|e| PrecompileError::Other(format!("Invalid message: {e}")))?;

    // sign
    let secp = Secp256k1::new();
    let sig = secp.sign_ecdsa_recoverable(&message, &secret_key);

    // serialize the output
    let (recid, sig) = sig.serialize_compact();
    let mut output = sig.to_vec();
    output.push(recid as u8);

    Ok(PrecompileOutput::new(gas_used, output.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::precompile::secp256k1::ecrecover;
    use revm::primitives::{alloy_primitives::B512, keccak256, Bytes, B256};

    use revm::precompile::PrecompileError;
    use secp256k1::{ecdsa::Signature, Message};

    // test using the secp256k1 crate's verify function
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

    // test using Ethereum'secrecover precompile
    // this is a common on-chain workflow for verifying signatures,
    // first recovering an eth address from the signature, then
    // comparing the resulting address with the expected one
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
            Some(PrecompileError::Other(msg)) => {
                assert_eq!(msg, "Invalid input length");
            }
            other => {
                panic!("Expected PrecompileError::Other(Invalid input length), got: {other:?}")
            }
        }
    }

    #[test]
    fn test_invalid_secret_key() {
        let input = Bytes::from([0u8; 64]);
        let gas_limit = 4000;
        let result = secp256k1_sign_ecdsa_recoverable(&input, gas_limit);
        assert!(result.is_err());
        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert_eq!(msg, "Invalid secret key: malformed or out-of-range secret key");
            }
            other => panic!("Expected PrecompileError::Other(Invalid secret key: malformed or out-of-range secret key), got: {other:?}"),
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
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }
}
