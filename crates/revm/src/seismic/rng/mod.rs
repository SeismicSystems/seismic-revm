use revm_precompile::{u64_to_address, Error as REVM_ERROR, Precompile, PrecompileOutput, PrecompileResult, PrecompileWithAddress};
use crate::primitives::{Env, Bytes, U256};

pub mod helpers;
use helpers::get_key_pair_id;
pub mod context;
use context::Context;
use std::cell::RefCell;

use anyhow::anyhow;
use merlin::{Transcript, TranscriptRng};
use rand_core::{CryptoRng, OsRng, RngCore};
use schnorrkel::keys::{ExpansionMode, Keypair, MiniSecretKey};

use anyhow::Error;
use helpers::Hash;

/// RNG domain separation context.
const RNG_CONTEXT: &[u8] = b"seismic rng context";
/// Per-block root VRF key domain separation context.
const VRF_KEY_CONTEXT: &[u8] = b"seismic vrf key context";

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

    fn derive_root_vrf_key(ctx: Context) -> Result<Keypair, Error> {
        // Hash all the relevant data to get bytes for the VRF secret key
        let key_id = get_key_pair_id([VRF_KEY_CONTEXT, ctx.header.as_slice()]);
        
       // "expanded" form to use with schnorrkel
        let kp = MiniSecretKey::from_bytes(&key_id)
            .map_err(|err| {
                anyhow!("schnorrkel conversion error: {}", err)
            })?
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
    pub fn fork(&self, ctx: Context, pers: &[u8]) -> Result<LeafRng, Error> {
        let mut inner = self.inner.borrow_mut();

        // Ensure the RNG is initialized and initialize it if not.
        if inner.rng.is_none() {
            // Derive the root VRF key for the current block.
            let root_vrf_key = Self::derive_root_vrf_key(ctx)?;

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

const HEADER: [u8; 32] = [
    0xc8, 0xb2, 0x24, 0xc5, 0x80, 0x03, 0xa7, 0x97, 
    0xc0, 0x06, 0x46, 0x97, 0xdf, 0x57, 0xa4, 0x20, 
    0x9b, 0x2b, 0x9c, 0xb5, 0x21, 0x22, 0x86, 0xa9, 
    0xb1, 0xb1, 0x83, 0x17, 0x63, 0x75, 0x25, 0x16
];


pub const RNG: PrecompileWithAddress=
    PrecompileWithAddress(u64_to_address(100), Precompile::Env(run));

// to refine: gas usage.
// use actual block randomness
pub fn run(input: &Bytes, gas_limit: u64, _env: &Env) -> PrecompileResult {
    let gas_used = 100;
    if gas_used > gas_limit {
        return Err(REVM_ERROR::OutOfGas.into());
    }

    let context = Context::new(HEADER);

    // Create first root RNG.
    let root_rng = RootRng::new();

    let mut leaf_rng = root_rng.fork(context,  &[]).expect("rng fork should work");
    let mut bytes1 = [0u8; 32];
    leaf_rng.fill_bytes(&mut bytes1);
    let value = U256::from(1);

    // Convert U256 to a byte array
    let bytes = Bytes::from(value.to_be_bytes::<32>().to_vec());
    println!("bytes: {:?}", Bytes::from(HEADER));
    Ok(PrecompileOutput::new(gas_used, Bytes::from(HEADER)))
}
