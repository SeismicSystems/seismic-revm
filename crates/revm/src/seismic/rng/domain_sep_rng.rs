use super::rng_env::RngEnv;

use anyhow::{anyhow, Error};
use merlin::{Transcript, TranscriptRng};
use rand_core::{CryptoRng, OsRng, RngCore};
use schnorrkel::keys::{ExpansionMode, Keypair, MiniSecretKey};
use std::cell::RefCell;

// // TODO: Replace with reth tx Hash type
pub type Hash = [u8; 32];

/// RNG domain separation context.
const RNG_CONTEXT: &[u8] = b"seismic rng context";

// // TODO: remove this?
// /// Per-block root VRF key domain separation context.
// const VRF_KEY_CONTEXT: &[u8] = b"seismic vrf key context";

/// A root RNG that can be used to derive domain-separated leaf RNGs.
pub struct RootRng {
    inner: RefCell<Inner>,
}

struct Inner {
    /// Merlin transcript for initializing the RNG.
    transcript: Transcript,
    /// A transcript-based RNG (when initialized).
    rng: Option<TranscriptRng>,
}

impl RootRng {
    /// Create a new root RNG.
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(Inner {
                transcript: Transcript::new(RNG_CONTEXT),
                rng: None,
            }),
        }
    }

    fn derive_root_vrf_key(env: RngEnv) -> Result<Keypair, Error> {
        // Hash all the relevant data to get bytes for the VRF secret key
        let env_hash = env.hash();

        // "expanded" form to use with schnorrkel
        let kp = MiniSecretKey::from_bytes(&env_hash)
            .map_err(|err| anyhow!("schnorrkel conversion error: {}", err))?
            .expand_to_keypair(ExpansionMode::Uniform);

        Ok(kp)
    }

    /// Append local entropy to the root RNG.
    ///
    /// # Non-determinism
    ///
    /// Using this method will result in the RNG being non-deterministic.
    pub fn append_local_entropy(&self) {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);

        let mut inner = self.inner.borrow_mut();
        inner.transcript.append_message(b"local-rng", &bytes);
    }

    /// Append an observed transaction hash to RNG transcript.
    pub fn append_tx(&self, tx_hash: Hash) {
        let mut inner = self.inner.borrow_mut();
        inner.transcript.append_message(b"tx", tx_hash.as_ref());
    }

    /// Append an observed subcontext to RNG transcript.
    pub fn append_subcontext(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.transcript.append_message(b"subctx", &[]);
    }

    /// Create an independent leaf RNG using this RNG as its parent.
    pub fn fork(&self, rng_env: RngEnv, pers: &[u8]) -> Result<LeafRng, Error> {
        let mut inner = self.inner.borrow_mut();

        // Ensure the RNG is initialized and initialize it if not.
        if inner.rng.is_none() {
            // Derive the root VRF key for the current block.
            let root_vrf_key = Self::derive_root_vrf_key(rng_env)?;

            // Initialize the root RNG.
            let rng = root_vrf_key
                .vrf_create_hash(&mut inner.transcript)
                .make_merlin_rng(&[]);
            inner.rng = Some(rng);
        }

        // Generate the leaf RNG.
        inner.transcript.append_message(b"fork", pers);

        let rng_builder = inner.transcript.build_rng();
        let parent_rng = inner.rng.as_mut().expect("rng must be initialized");
        let rng = rng_builder.finalize(parent_rng);

        Ok(LeafRng(rng))
    }
}

/// A leaf RNG.
pub struct LeafRng(TranscriptRng);

impl RngCore for LeafRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.0.try_fill_bytes(dest)
    }
}

impl CryptoRng for LeafRng {}