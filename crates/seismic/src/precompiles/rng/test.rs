#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use super::*;
use domain_sep_rng::RootRng;
use rand::RngCore;
use revm::primitives::B256;
use seismic_crypto::well_known_rng_ikm;
use std::str::FromStr;

fn hex_to_hash_bytes(input: &str) -> B256 {
    B256::from_str(input).unwrap()
}

#[test]
fn test_rng_basic() {
    // First derivation with empty pers
    let root_rng = RootRng::new(well_known_rng_ikm());
    let bytes1 = root_rng.derive_bytes(&[], 32);

    // Same RNG, same pers — should produce the same output (stateless derivation)
    let bytes1_again = root_rng.derive_bytes(&[], 32);
    assert_eq!(
        bytes1, bytes1_again,
        "same inputs should produce same output"
    );

    // Create second root RNG using the same keypair — should produce the same output
    let root_rng2 = RootRng::new(well_known_rng_ikm());
    let bytes2 = root_rng2.derive_bytes(&[], 32);
    assert_eq!(
        bytes1, bytes2,
        "rng should be deterministic across instances"
    );

    // Different personalization should produce different output
    let bytes3 = root_rng.derive_bytes(b"domsep", 32);
    assert_ne!(
        bytes1, bytes3,
        "different pers should produce different output"
    );

    // Appending a tx hash should change the output
    let mut root_rng3 = RootRng::new(well_known_rng_ikm());
    root_rng3.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    let bytes4 = root_rng3.derive_bytes(&[], 32);
    assert_ne!(bytes1, bytes4, "tx hash should change output");

    // Different tx hash should produce different output
    let mut root_rng4 = RootRng::new(well_known_rng_ikm());
    root_rng4.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));
    let bytes5 = root_rng4.derive_bytes(&[], 32);
    assert_ne!(
        bytes4, bytes5,
        "different tx hash should produce different output"
    );

    // Same tx hash as root_rng3 should produce the same output
    let mut root_rng5 = RootRng::new(well_known_rng_ikm());
    root_rng5.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    let bytes6 = root_rng5.derive_bytes(&[], 32);
    assert_eq!(bytes4, bytes6, "same tx hash should be deterministic");

    // Multiple tx hashes should produce different output than single
    let mut root_rng6 = RootRng::new(well_known_rng_ikm());
    root_rng6.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    root_rng6.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));
    let bytes7 = root_rng6.derive_bytes(&[], 32);
    assert_ne!(bytes4, bytes7, "multiple tx hashes should change output");
}

#[test]
fn test_rng_gas_domain_separation() {
    let mut root_rng1 = RootRng::new(well_known_rng_ikm());
    root_rng1.append_tx(&B256::from([1u8; 32]));
    root_rng1.append_gas_left(1000);
    let bytes1 = root_rng1.derive_bytes(&[], 32);

    let mut root_rng2 = RootRng::new(well_known_rng_ikm());
    root_rng2.append_tx(&B256::from([1u8; 32]));
    root_rng2.append_gas_left(2000);
    let bytes2 = root_rng2.derive_bytes(&[], 32);

    assert_ne!(
        bytes1, bytes2,
        "different gas_left should produce different output"
    );
}

#[test]
fn test_rng_different_keys_different_output() {
    let mut ikm1 = [0u8; 64];
    rand::rng().fill_bytes(&mut ikm1);
    let mut ikm2 = [0u8; 64];
    rand::rng().fill_bytes(&mut ikm2);

    let root_rng1 = RootRng::new(ikm1);
    let root_rng2 = RootRng::new(ikm2);

    let bytes1 = root_rng1.derive_bytes(&[], 32);
    let bytes2 = root_rng2.derive_bytes(&[], 32);

    assert_ne!(
        bytes1, bytes2,
        "different keys should produce different output"
    );
}

#[test]
fn test_precompile_info_layout_matches_gas_schedule() {
    use crate::chain::rng_container::derive_rng_output;
    use domain_sep_rng::{CHUNK_COUNTER_LEN, MAX_HKDF_OUTPUT, RNG_INFO_PREFIX_LEN};
    use hkdf::Hkdf;
    use sha2::Sha256;

    let key = [0x11; 64];
    let parent = B256::from([0x22; 32]);
    let accumulator = B256::from([0x33; 32]);
    let tx = B256::from([0x44; 32]);
    let gas_left = 6000u64;
    let pers = b"personalization";

    // Construct the original byte layout independently of RootRng's helpers.
    let mut info = Vec::new();
    info.extend_from_slice(b"block");
    info.extend_from_slice(parent.as_ref());
    info.extend_from_slice(b"acc");
    info.extend_from_slice(accumulator.as_ref());
    info.extend_from_slice(b"tx");
    info.extend_from_slice(tx.as_ref());
    info.extend_from_slice(b"gas");
    info.extend_from_slice(&gas_left.to_le_bytes());
    info.extend_from_slice(b"pers");
    assert_eq!(info.len(), RNG_INFO_PREFIX_LEN);
    assert_eq!(info.len(), 121);
    info.extend_from_slice(pers);

    let hkdf = Hkdf::<Sha256>::new(Some(b"seismic rng context"), &key);
    for len in [
        0,
        32,
        MAX_HKDF_OUTPUT,
        MAX_HKDF_OUTPUT + 1,
        2 * MAX_HKDF_OUTPUT + 1,
    ] {
        let mut expected = vec![0; len];
        if len <= MAX_HKDF_OUTPUT {
            hkdf.expand(&info, &mut expected).unwrap();
        } else {
            for (index, chunk) in expected.chunks_mut(MAX_HKDF_OUTPUT).enumerate() {
                let mut chunk_info = info.clone();
                chunk_info.extend_from_slice(&(index as u32).to_le_bytes());
                assert_eq!(chunk_info.len(), info.len() + CHUNK_COUNTER_LEN);
                hkdf.expand(&chunk_info, chunk).unwrap();
            }
        }
        let actual =
            derive_rng_output(pers, len, &tx, key, &parent, &accumulator, gas_left).unwrap();
        assert_eq!(actual.as_ref(), expected.as_slice());
    }
}

#[test]
fn test_large_output() {
    let root_rng = RootRng::new(well_known_rng_ikm());

    // Request more than the HKDF single-expand limit (8160 bytes)
    let large_output = root_rng.derive_bytes(b"large", 10000);
    assert_eq!(large_output.len(), 10000);

    // Verify determinism for large outputs
    let large_output_2 = root_rng.derive_bytes(b"large", 10000);
    assert_eq!(
        large_output, large_output_2,
        "large output should be deterministic"
    );
}
