use revm::precompile::{
    u64_to_address, Precompile, PrecompileError, PrecompileId, PrecompileOutput, PrecompileResult,
};

use secp256k1::Secp256k1;

/* --------------------------------------------------------------------------
Precompile Wiring
-------------------------------------------------------------------------- */
/// Address of the SECP256K1 pubkey-derivation precompile.
///
/// The issue that requested this precompile (SeismicSystems/seismic-revm#207)
/// proposed `0x6A`, the next sequential address after `SECP256K1_SIGN`
/// (`0x69`). That address is claimed by the in-flight tx-type precompile
/// (SeismicSystems/seismic-revm#227, not yet merged at the time of writing),
/// so this uses `0x6B` instead to avoid a collision once one of the two
/// lands. Update if `#227` merges with a different address, or if this PR
/// lands first and `0x6B` should move to `0x6A`.
pub const SECP256K1_PUBKEY_ADDRESS: u64 = 107;

/// Returns the secp256k1 pubkey-derivation precompile with its address.
pub fn precompiles() -> impl Iterator<Item = Precompile> {
    [SECP256K1_PUBKEY].into_iter()
}

pub const SECP256K1_PUBKEY: Precompile = Precompile::new(
    PrecompileId::Custom(std::borrow::Cow::Borrowed("Secp256K1_pubkey")),
    u64_to_address(SECP256K1_PUBKEY_ADDRESS),
    secp256k1_derive_pubkey,
);

/// Same cost as [`SECP256K1_SIGN`](super::secp256k1_sign::SECP256K1_SIGN) and
/// the ECDH shared-secret step (`SHARED_SECRET_COST` in
/// `ecdh_derive_sym_key.rs`): all three are dominated by a single secp256k1
/// scalar multiplication, so they're priced identically rather than
/// introducing a fourth constant for the same underlying operation.
const BASE_GAS: u64 = 3000;

/* --------------------------------------------------------------------------
Precompile Logic and Gas Calculation
-------------------------------------------------------------------------- */
/// Derives the compressed secp256k1 public key `sk * G` from a 32-byte
/// private key, returning the 33-byte SEC1 compressed encoding.
///
/// Lets contracts generate a keypair entirely on-chain (e.g. `contractPrivateKey
/// = sbytes32(rng256()); contractPublicKey = secp256k1_pubkey(contractPrivateKey);`)
/// instead of requiring off-chain key generation before deployment, and
/// enables on-chain verification that a public key corresponds to a given
/// private key.
pub fn secp256k1_derive_pubkey(input: &[u8], gas_limit: u64) -> PrecompileResult {
    let gas_used = BASE_GAS;
    if gas_used > gas_limit {
        return Err(PrecompileError::OutOfGas);
    }

    // input validation
    if input.len() != 32 {
        return Err(PrecompileError::Other("Invalid input length".to_string()));
    }
    // SAFETY: Length validated above - input is exactly 32 bytes
    #[allow(clippy::expect_used, clippy::indexing_slicing)]
    let key_bytes: [u8; 32] =
        input[0..32].try_into().expect("input length already validated as 32 bytes");
    let secret_key = secp256k1::SecretKey::from_byte_array(key_bytes)
        .map_err(|e| PrecompileError::Other(format!("Invalid secret key: {e}")))?;

    // derive
    let secp = Secp256k1::new();
    let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);

    // serialize the compressed (33-byte) output
    let output = public_key.serialize().to_vec();

    Ok(PrecompileOutput::new(gas_used, output.into()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

    use super::*;
    use revm::precompile::PrecompileError;
    use revm::primitives::Bytes;

    #[test]
    fn test_derives_expected_pubkey() {
        let sk_bytes: [u8; 32] = [0x1; 32];
        let sk = secp256k1::SecretKey::from_byte_array(sk_bytes).unwrap();
        let secp = Secp256k1::new();
        let expected = secp256k1::PublicKey::from_secret_key(&secp, &sk);

        let gas_limit = 4000;
        let output = secp256k1_derive_pubkey(&Bytes::from(sk_bytes.to_vec()), gas_limit)
            .unwrap()
            .bytes;

        assert_eq!(output.len(), 33, "output should be the 33-byte compressed encoding");
        assert_eq!(&output[..], &expected.serialize()[..]);
    }

    #[test]
    fn test_same_key_is_deterministic() {
        let sk_bytes: [u8; 32] = [0x42; 32];
        let gas_limit = 4000;

        let out1 = secp256k1_derive_pubkey(&Bytes::from(sk_bytes.to_vec()), gas_limit)
            .unwrap()
            .bytes;
        let out2 = secp256k1_derive_pubkey(&Bytes::from(sk_bytes.to_vec()), gas_limit)
            .unwrap()
            .bytes;

        assert_eq!(out1, out2, "same private key must derive the same public key every time");
    }

    #[test]
    fn test_different_keys_derive_different_pubkeys() {
        let gas_limit = 4000;
        let out1 = secp256k1_derive_pubkey(&Bytes::from(vec![0x1; 32]), gas_limit).unwrap().bytes;
        let out2 = secp256k1_derive_pubkey(&Bytes::from(vec![0x2; 32]), gas_limit).unwrap().bytes;

        assert_ne!(out1, out2);
    }

    #[test]
    fn test_invalid_input_length() {
        let input = Bytes::from("short_bytes");
        let gas_limit = 4000;
        let result = secp256k1_derive_pubkey(&input, gas_limit);
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
        // All-zero scalar is not a valid secp256k1 secret key.
        let input = Bytes::from([0u8; 32]);
        let gas_limit = 4000;
        let result = secp256k1_derive_pubkey(&input, gas_limit);
        assert!(result.is_err());
        match result.err() {
            Some(PrecompileError::Other(msg)) => {
                assert_eq!(msg, "Invalid secret key: malformed or out-of-range secret key");
            }
            other => panic!(
                "Expected PrecompileError::Other(Invalid secret key: malformed or out-of-range secret key), got: {other:?}"
            ),
        }
    }

    #[test]
    fn test_out_of_gas() {
        let sk_bytes: [u8; 32] = [0x1; 32];
        let small_gas_limit = 500; // well below BASE_GAS (3000)

        let result = secp256k1_derive_pubkey(&Bytes::from(sk_bytes.to_vec()), small_gas_limit);
        assert!(result.is_err());

        match result.err() {
            Some(PrecompileError::OutOfGas) => {}
            other => panic!("Expected OutOfGas, got {:?}", other),
        }
    }
}
