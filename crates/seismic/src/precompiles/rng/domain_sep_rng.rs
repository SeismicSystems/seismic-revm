//! Domain-separated RNG using HKDF-SHA256.
//!
//! For each precompile call, random bytes are derived via:
//! ```text
//! HKDF-SHA256(
//!   ikm  = rng_ikm,  // the node's 64-byte rng key material
//!   salt = b"seismic rng context",
//!   info = domain_data || b"pers" || pers
//! ) → output bytes
//! ```
//!
//! Each call constructs a fresh `RootRng`, appends tx_hash + gas_left,
//! then derives output. There is no persistent state between calls.
use hkdf::Hkdf;
use revm::primitives::B256;
use seismic_crypto::get_unsecure_sample_schnorrkel_keypair;
use sha2::Sha256;

/// RNG domain separation salt.
const RNG_SALT: &[u8] = b"seismic rng context";

/// A stateless RNG that derives output bytes via HKDF-SHA256.
///
/// Constructed fresh for each precompile call. Domain separation data
/// (tx hash, gas left) is appended before derivation.
pub struct RootRng {
    /// The 64-byte HKDF input key material.
    key_bytes: [u8; 64],
    /// Accumulated domain separation info (tx hashes, gas values, etc.).
    domain_data: Vec<u8>,
}

impl RootRng {
    /// Create a new root RNG from 64 bytes of HKDF input key material.
    pub fn new(rng_ikm: [u8; 64]) -> Self {
        Self {
            key_bytes: rng_ikm,
            domain_data: Vec::new(),
        }
    }

    /// A default RNG for testing that loads a sample key.
    /// We do not implement the Default trait because
    /// it might be misleading or error-prone.
    pub fn test_default() -> Self {
        Self::new(get_unsecure_sample_schnorrkel_keypair().secret.to_bytes())
    }

    /// Append the parent block hash to the domain separation data.
    pub fn append_parent_block_hash(&mut self, hash: &B256) {
        self.domain_data.extend_from_slice(b"block");
        self.domain_data.extend_from_slice(hash.as_ref());
    }

    /// Append the transaction hash accumulator to the domain separation data.
    pub fn append_tx_hash_accumulator(&mut self, acc: &B256) {
        self.domain_data.extend_from_slice(b"acc");
        self.domain_data.extend_from_slice(acc.as_ref());
    }

    /// Append a transaction hash to the domain separation data.
    pub fn append_tx(&mut self, tx_hash: &B256) {
        self.domain_data.extend_from_slice(b"tx");
        self.domain_data.extend_from_slice(tx_hash.as_ref());
    }

    /// Append the remaining gas to the domain separation data.
    pub fn append_gas_left(&mut self, gas_left: u64) {
        self.domain_data.extend_from_slice(b"gas");
        self.domain_data.extend_from_slice(&gas_left.to_le_bytes());
    }

    /// Derive `len` random bytes using HKDF-SHA256 with the given personalization.
    ///
    /// The HKDF info parameter is: `domain_data || b"pers" || pers`.
    /// This is a stateless operation — same inputs always produce the same output.
    ///
    /// For outputs larger than 255 * 32 = 8160 bytes (the HKDF-SHA256 limit),
    /// this uses counter-mode chunking internally.
    pub fn derive_bytes(&self, pers: &[u8], len: usize) -> Vec<u8> {
        let hkdf = Hkdf::<Sha256>::new(Some(RNG_SALT), &self.key_bytes);

        // Build info: domain_data || b"pers" || pers
        let mut info = Vec::with_capacity(self.domain_data.len() + 4 + pers.len());
        info.extend_from_slice(&self.domain_data);
        info.extend_from_slice(b"pers");
        info.extend_from_slice(pers);

        // HKDF-Expand has a max output of 255 * HashLen (8160 bytes for SHA-256).
        // For larger outputs, use counter-mode chunking.
        const MAX_HKDF_OUTPUT: usize = 255 * 32;

        if len <= MAX_HKDF_OUTPUT {
            let mut output = vec![0u8; len];
            // SAFETY: len <= MAX_HKDF_OUTPUT so expand cannot fail
            #[allow(clippy::expect_used)]
            hkdf.expand(&info, &mut output)
                .expect("HKDF expand cannot fail for len <= 8160");
            output
        } else {
            // Counter-mode: chunk the output into MAX_HKDF_OUTPUT-sized pieces,
            // each with a unique counter suffix in info.
            let mut output = Vec::with_capacity(len);
            let mut chunk_idx: u32 = 0;
            while output.len() < len {
                let remaining = len - output.len();
                let chunk_len = remaining.min(MAX_HKDF_OUTPUT);
                let mut chunk = vec![0u8; chunk_len];

                // The counter is prepended, not appended. `pers` is caller-controlled
                // and is the tail of `info`, so appending the counter would make a
                // chunked derivation with personalization `P` produce the same HKDF
                // info as a single-shot derivation with `P || counter` — i.e. the
                // caller could obtain a chunked call's first 8160 bytes from a
                // separate short call. Prepending keeps the two disjoint, because
                // `info` always starts with the fixed-length domain data.
                let mut chunk_info = Vec::with_capacity(4 /* counter */ + info.len());
                chunk_info.extend_from_slice(&chunk_idx.to_le_bytes());
                chunk_info.extend_from_slice(&info);

                // SAFETY: chunk_len <= MAX_HKDF_OUTPUT
                #[allow(clippy::expect_used)]
                hkdf.expand(&chunk_info, &mut chunk)
                    .expect("HKDF expand cannot fail for chunk_len <= 8160");

                output.extend_from_slice(&chunk);
                chunk_idx += 1;
            }
            output.truncate(len);
            output
        }
    }
}
