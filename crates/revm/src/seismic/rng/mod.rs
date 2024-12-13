//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `env_hash`: Provides functions related to hashing the provided env context.
//! - `precompile`: Provides the precompile to be called by other contracts.

pub mod domain_sep_rng; 
pub mod env_hash;
pub mod precompile;

pub use domain_sep_rng::RootRng;
