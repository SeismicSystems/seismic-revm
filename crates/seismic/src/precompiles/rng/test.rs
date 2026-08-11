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
use std::str::FromStr;

fn hex_to_hash_bytes(input: &str) -> B256 {
    B256::from_str(input).unwrap()
}

#[test]
fn test_rng_basic() {
    // First derivation with empty pers
    let root_rng = RootRng::test_default();
    let bytes1 = root_rng.derive_bytes(&[], 32);

    // Same RNG, same pers — should produce the same output (stateless derivation)
    let bytes1_again = root_rng.derive_bytes(&[], 32);
    assert_eq!(
        bytes1, bytes1_again,
        "same inputs should produce same output"
    );

    // Create second root RNG using the same keypair — should produce the same output
    let root_rng2 = RootRng::test_default();
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
    let mut root_rng3 = RootRng::test_default();
    root_rng3.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    let bytes4 = root_rng3.derive_bytes(&[], 32);
    assert_ne!(bytes1, bytes4, "tx hash should change output");

    // Different tx hash should produce different output
    let mut root_rng4 = RootRng::test_default();
    root_rng4.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));
    let bytes5 = root_rng4.derive_bytes(&[], 32);
    assert_ne!(
        bytes4, bytes5,
        "different tx hash should produce different output"
    );

    // Same tx hash as root_rng3 should produce the same output
    let mut root_rng5 = RootRng::test_default();
    root_rng5.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    let bytes6 = root_rng5.derive_bytes(&[], 32);
    assert_eq!(bytes4, bytes6, "same tx hash should be deterministic");

    // Multiple tx hashes should produce different output than single
    let mut root_rng6 = RootRng::test_default();
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
    let mut root_rng1 = RootRng::test_default();
    root_rng1.append_tx(&B256::from([1u8; 32]));
    root_rng1.append_gas_left(1000);
    let bytes1 = root_rng1.derive_bytes(&[], 32);

    let mut root_rng2 = RootRng::test_default();
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
fn test_large_output() {
    let root_rng = RootRng::test_default();

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

/// The HKDF-SHA256 single-expand limit; above this, `derive_bytes` chunks.
const MAX_HKDF_OUTPUT: usize = 255 * 32;

/// A chunked derivation must not be reproducible through a short derivation.
///
/// `pers` is caller-controlled and forms the tail of the HKDF `info`. If the
/// chunk counter were appended, a chunked call with personalization `P` would
/// share its info with a single-shot call using `P || counter`, handing the
/// caller the chunked call's first 8160 bytes for a fraction of the gas.
#[test]
fn test_chunked_output_does_not_collide_with_short_call() {
    let root_rng = RootRng::test_default();
    let pers = b"collide";

    let mut pers_with_counter = pers.to_vec();
    pers_with_counter.extend_from_slice(&0u32.to_le_bytes());

    let chunked = root_rng.derive_bytes(pers, MAX_HKDF_OUTPUT + 1);
    let short = root_rng.derive_bytes(&pers_with_counter, 32);

    assert_ne!(
        &chunked[..32],
        &short[..],
        "a chunked derivation must not be reproducible by a short call whose \
         pers carries the chunk counter"
    );
}

/// Every chunk boundary must be domain-separated, not just the first.
#[test]
fn test_each_chunk_is_domain_separated() {
    let root_rng = RootRng::test_default();
    let pers = b"chunks";

    let out = root_rng.derive_bytes(pers, MAX_HKDF_OUTPUT * 2 + 64);
    assert_eq!(out.len(), MAX_HKDF_OUTPUT * 2 + 64);

    let chunk0 = &out[..MAX_HKDF_OUTPUT];
    let chunk1 = &out[MAX_HKDF_OUTPUT..MAX_HKDF_OUTPUT * 2];
    assert_ne!(chunk0, chunk1, "consecutive chunks must differ");

    for idx in 0..3u32 {
        let mut spoof = pers.to_vec();
        spoof.extend_from_slice(&idx.to_le_bytes());
        let short = root_rng.derive_bytes(&spoof, 32);
        assert_ne!(
            &out[..32],
            &short[..],
            "chunk 0 must not be reproducible via pers || {idx}"
        );
    }
}

/// Requesting exactly the single-expand limit must not take the chunked path,
/// and asking for one more byte must change the output rather than extend it.
#[test]
fn test_chunk_boundary_changes_derivation() {
    let root_rng = RootRng::test_default();
    let pers = b"boundary";

    let at_limit = root_rng.derive_bytes(pers, MAX_HKDF_OUTPUT);
    let over_limit = root_rng.derive_bytes(pers, MAX_HKDF_OUTPUT + 1);

    assert_eq!(at_limit.len(), MAX_HKDF_OUTPUT);
    assert_eq!(over_limit.len(), MAX_HKDF_OUTPUT + 1);
    assert_ne!(
        &at_limit[..32],
        &over_limit[..32],
        "crossing the chunk boundary must re-derive, not extend"
    );
}
