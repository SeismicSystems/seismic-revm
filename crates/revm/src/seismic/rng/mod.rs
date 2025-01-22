//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `test`: Contains test cases for the RNG logic, e.g. domain separation, cloning, etc

mod domain_sep_rng;
pub use domain_sep_rng::{LeafRng, RootRng};

#[cfg(test)]
mod test;
