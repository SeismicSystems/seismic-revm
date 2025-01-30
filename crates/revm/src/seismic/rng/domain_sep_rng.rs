//! This module provides a domain separation RNG for the Seismic chain.
//! It uses the Merlin transcript to generate a root RNG that is used to derive
//! a leaf RNG for each transaction.
//! The Merlin transcript is initialized with a hash of the block environment.
//! The Merlin transcript is then forked for each transaction
//! The leaf RNG is then used to generate random bytes.
//!
//! This module is heavily inspired Oasis Network's RNG implementation.
use crate::primitives::B256;
use merlin::{Transcript, TranscriptRng};
use rand_core::{CryptoRng, OsRng, RngCore};
pub use schnorrkel::keys::Keypair as SchnorrkelKeypair;
use tee_service_api::get_sample_schnorrkel_keypair;
use std::{cell::RefCell, rc::Rc};

/// RNG domain separation context.
const RNG_CONTEXT: &[u8] = b"seismic rng context";

/// A root RNG that can be used to derive domain-separated leaf RNGs.
pub struct RootRng {
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    /// The VRF key for the block
    root_vrf_key: SchnorrkelKeypair,
    /// Merlin transcript for initializing the RNG.
    transcript: Transcript,
    /// A transcript-based RNG (when initialized).
    rng: Option<TranscriptRng>,
    /// the transcript used to initialize the rng, saved for cloning
    cloning_transcript: Option<Transcript>,
    /// number of forks, saved for cloning
    num_forks: u64,
}

impl Clone for RootRng {
    fn clone(&self) -> Self {
        let inner = self.inner.borrow_mut();
        let rng_copy: Option<TranscriptRng>;
        let root_vrf = inner.root_vrf_key.clone();
        if inner.rng.is_some() {
            // make a new rng with the same transcript and vrf key
            let cloning_transcript = inner.cloning_transcript.as_ref().unwrap().clone();

            let mut rng = root_vrf
                .vrf_create_hash(cloning_transcript)
                .make_merlin_rng(&[]);

            // fast foward the rng to the same point as the original
            // By assumption, fork() is the only place root TranscriptRng is used
            for _ in 0..inner.num_forks {
                let mut bytes = [0u8; 32];
                rng.fill_bytes(&mut bytes);
            }

            rng_copy = Some(rng);
        } else {
            rng_copy = None;
        }

        let new_inner = Inner {
            root_vrf_key: root_vrf,
            transcript: inner.transcript.clone(),
            rng: rng_copy,
            cloning_transcript: inner.cloning_transcript.clone(),
            num_forks: inner.num_forks,
        };

        Self {
            inner: Rc::new(RefCell::new(new_inner)),
        }
    }
}

impl RootRng {
    /// Create a new root RNG.
    pub fn new(root_vrf_key: SchnorrkelKeypair) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner {
                root_vrf_key,
                transcript: Transcript::new(RNG_CONTEXT),
                rng: None,
                cloning_transcript: None,
                num_forks: 0,
            })),
        }
    }

    /// A default rng for testing that loads a sample keypair.
    /// We do not implement the Default trait becuase
    /// it might be misleading or error-prone.
    pub fn test_default() -> Self {
        Self::new(get_sample_schnorrkel_keypair())
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
    pub fn append_tx(&self, tx_hash: &B256) {
        let mut inner = self.inner.borrow_mut();
        inner.transcript.append_message(b"tx", tx_hash.as_ref());
    }

    /// Append an observed subcontext to RNG transcript.
    pub fn append_subcontext(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.transcript.append_message(b"subctx", &[]);
    }

    /// Create an independent leaf RNG using this RNG as its parent.
    pub fn fork(&self, pers: &[u8]) -> LeafRng {
        let mut inner = self.inner.borrow_mut();

        // Ensure the RNG is initialized and initialize it if not.
        if inner.rng.is_none() {
            // Initialize the root RNG.
            inner.cloning_transcript = Some(inner.transcript.clone());
            let root_key = inner.root_vrf_key.clone();

            let rng = root_key
                .vrf_create_hash(&mut inner.transcript)
                .make_merlin_rng(&[]);

            inner.rng = Some(rng);
        }

        // Generate the leaf RNG.
        inner.transcript.append_message(b"fork", pers);

        let rng_builder = inner.transcript.build_rng();
        let parent_rng = inner.rng.as_mut().expect("rng must be initialized");
        let rng = rng_builder.finalize(parent_rng);

        // Increment the number of forks
        inner.num_forks += 1;

        LeafRng(rng)
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
