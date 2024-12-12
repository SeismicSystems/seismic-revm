//! # Module Overview
//! This module provides functionality a RNG precompile and related utilities.
//!
//! ## Submodules
//! - `context`: Context about the current block provided to the RNG
//! - `domain_sep_rng`: Implements a domain-separated random number generator.
//! - `precompile`: Provides the precompile to be called by other contracts.

pub mod context; 
pub mod domain_sep_rng; 
pub mod precompile;