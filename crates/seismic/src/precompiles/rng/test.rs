use super::*;
use domain_sep_rng::RootRng;
use rand_core::RngCore;
use revm::primitives::B256;
use schnorrkel::{keys::Keypair as SchnorrkelKeypair, ExpansionMode};
use std::str::FromStr;

fn hex_to_hash_bytes(input: &str) -> B256 {
    B256::from_str(input).unwrap()
}

#[test]
fn test_rng_basic() {
    let root_rng = RootRng::test_default();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1_1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1_1);

    assert_ne!(bytes1, bytes1_1, "rng should apply domain separation");

    // Create second root RNG using the same context so the ephemeral key is shared.
    let root_rng = RootRng::test_default();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes2 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes2);

    assert_eq!(bytes1, bytes2, "rng should be deterministic");

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes2_1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes2_1);

    assert_ne!(bytes2, bytes2_1, "rng should apply domain separation");
    assert_eq!(bytes1_1, bytes2_1, "rng should be deterministic");

    // Create third root RNG using the same context, but with different personalization.
    let root_rng = RootRng::test_default();

    let mut leaf_rng = root_rng.fork(b"domsep");
    let mut bytes3 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes3);

    assert_ne!(bytes2, bytes3, "rng should apply domain separation");

    // Create another root RNG using the same context, but with different history.
    let root_rng = RootRng::test_default();
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes4 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes4);

    assert_ne!(bytes2, bytes4, "rng should apply domain separation");

    // Create another root RNG using the same context, but with different history.
    let root_rng = RootRng::test_default();
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes5 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes5);

    assert_ne!(bytes4, bytes5, "rng should apply domain separation");

    // Create another root RNG using the same context, but with same history as four.
    let root_rng = RootRng::test_default();
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes6 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes6);

    assert_eq!(bytes4, bytes6, "rng should be deterministic");

    // Create another root RNG using the same context, but with different history.
    let root_rng = RootRng::test_default();
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes7 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes7);

    assert_ne!(bytes4, bytes7, "rng should apply domain separation");

    // Create another root RNG using the same context, but with different init point.
    let root_rng = RootRng::test_default();
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000001",
    ));
    let _ = root_rng.fork(&[]); // Force init.
    root_rng.append_tx(&hex_to_hash_bytes(
        "0000000000000000000000000000000000000000000000000000000000000002",
    ));

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes8 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes8);

    assert_ne!(bytes7, bytes8, "rng should apply domain separation");
    assert_ne!(bytes6, bytes8, "rng should apply domain separation");
}

#[test]
fn test_rng_local_entropy() {
    let eph_rng_keypair: SchnorrkelKeypair = schnorrkel::MiniSecretKey::generate()
        .expand(ExpansionMode::Uniform)
        .into();
    let root_rng = RootRng::new(eph_rng_keypair.clone());

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    // Create second root RNG using the same context, but mix in local entropy.
    let root_rng = RootRng::test_default();
    root_rng.append_local_entropy();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes2 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes2);

    assert_ne!(bytes1, bytes2, "rng should apply domain separation");
}

#[test]
fn test_rng_parent_fork_propagation() {
    let eph_rng_keypair: SchnorrkelKeypair = schnorrkel::MiniSecretKey::generate()
        .expand(ExpansionMode::Uniform)
        .into();
    let root_rng = RootRng::new(eph_rng_keypair.clone());

    let mut leaf_rng = root_rng.fork(b"a");
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng = root_rng.fork(b"a");
    let mut bytes1_1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1_1);

    // Create second root RNG.
    let root_rng = RootRng::test_default();

    let mut leaf_rng = root_rng.fork(b"b");
    let mut bytes2 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes2);

    let mut leaf_rng = root_rng.fork(b"a");
    let mut bytes2_1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes2_1);

    assert_ne!(
        bytes1_1, bytes2_1,
        "forks should propagate domain separator to parent"
    );
}

#[test]
fn test_clone_rng_before_init() {
    let root_rng = RootRng::test_default();

    // clone and test leaves are the same
    let root_rng_2 = root_rng.clone();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng_2 = root_rng_2.fork(&[]);
    let mut bytes2 = [0u8; 32];
    leaf_rng_2.fill_bytes(&mut bytes2);

    assert_eq!(bytes1, bytes2, "rng should be deterministic");
}

#[test]
fn test_clone_rng_after_init() {
    let root_rng = RootRng::test_default();

    // fork
    root_rng.append_tx(&B256::from([1u8; 32]));
    let _ = root_rng.fork(&[]);

    // clone and test rng is same
    let root_rng_2 = root_rng.clone();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng_2 = root_rng_2.fork(&[]);
    let mut bytes2 = [0u8; 32];
    leaf_rng_2.fill_bytes(&mut bytes2);

    assert_eq!(bytes1, bytes2, "rng should be deterministic");

    let root_rng_3 = root_rng.clone();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng_3 = root_rng_3.fork(&[]);
    let mut bytes3 = [0u8; 32];
    leaf_rng_3.fill_bytes(&mut bytes3);

    assert_eq!(bytes1, bytes3, "rng should be deterministic");
}

#[test]
fn test_clone_after_local_entropy() {
    let eph_rng_keypair: SchnorrkelKeypair = schnorrkel::MiniSecretKey::generate()
        .expand(ExpansionMode::Uniform)
        .into();
    let root_rng = RootRng::new(eph_rng_keypair.clone());

    // simulate some initial transactions with local entropy
    let _ = root_rng.fork(&[]);
    root_rng.append_local_entropy();
    let _ = root_rng.fork(&[]);
    root_rng.append_local_entropy();

    // clone and test rng is same
    let root_rng_2 = root_rng.clone();

    let mut leaf_rng = root_rng.fork(&[]);
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);

    let mut leaf_rng_2 = root_rng_2.fork(&[]);
    let mut bytes2 = [0u8; 32];
    leaf_rng_2.fill_bytes(&mut bytes2);
}
