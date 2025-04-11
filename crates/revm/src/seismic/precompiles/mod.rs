//! Precompiles Added for the Seismic Chain
//!
//! This module provides several precompiles enabled by the Seismic Chain
//! features. Below is an overview of the available precompiles and their
//! functionalities:
//!
//! # Modules
//!
//! - [`aes`]: Provides AES-GCM encryption and decryption functionalities.
//! - [`ecdh_derive_sym_key`]: Implements symmetric key derivation using
//!     Elliptic Curve Diffie-Hellman (ECDH) with the secp256k1 curve,
//!     producing an AES-compatible key.
//! - [`hkdf_derive_sym_key`]: Implements key derivation using the HKDF
//!     (HMAC-based Key Derivation Function) algorithm to derive AES-compatible
//!     symmetric keys from raw input bytes.
//! - [`rng`]: Generates cryptographically secure random bytes. The randomness
//!     is based on a secret Verifiable Random Function (VRF) key and the
//!     block's transcript.

pub mod aes;
pub mod ecdh_derive_sym_key;
pub mod hkdf_derive_sym_key;
pub mod rng;
pub mod secp256k1_sign;

use revm_precompile::{u64_to_address, Address};

// Address constants
pub const RNG_ADDRESS: Address = u64_to_address(100); // Hex address `0x64`.
pub const ECDH_ADDRESS: Address = u64_to_address(101); // Hex address `0x65`.
pub const AES_GCM_ENC_ADDRESS: Address = u64_to_address(102); // Hex address `0x66`.
pub const AES_GCM_DEC_ADDRESS: Address = u64_to_address(103); // Hex address `0x67`.
pub const HDKF_ADDRESS: Address = u64_to_address(104); // Hex address `0x68`.
pub const SECP256K1_SIGN_ADDRESS: Address = u64_to_address(105); // Hex address `0x69`.
