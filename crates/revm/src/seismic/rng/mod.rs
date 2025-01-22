//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `precompile`: Provides the precompile to be called by other contracts.

pub mod domain_sep_rng;
pub mod precompile;

#[cfg(test)]
mod test;

pub use domain_sep_rng::{LeafRng, RootRng};
