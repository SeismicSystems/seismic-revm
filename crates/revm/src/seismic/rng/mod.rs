//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `hashing`: Provides functions related to hashing the provided env context.
//! - `precompile`: Provides the precompile to be called by other contracts.

pub mod domain_sep_rng; 
pub mod hashing;
pub mod precompile;
