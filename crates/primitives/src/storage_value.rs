//! Storage value trait for generic storage in the EVM.
//!
//! This trait abstracts over the storage value type, allowing the EVM to be generic
//! over `U256` (upstream) or `FlaggedStorage` (Seismic, which carries a privacy flag).

use crate::U256;
use alloy_primitives::FlaggedStorage;
use core::fmt::Debug;
use core::hash::Hash;

/// Trait for EVM storage values.
///
/// Upstream revm uses `U256` for storage values. Seismic extends this to
/// `FlaggedStorage` which carries an `is_private` flag alongside the value.
///
/// This trait provides the minimal interface needed by gas calculations and
/// instruction stack operations. Seismic-specific methods (like `is_private`)
/// stay on `FlaggedStorage` directly.
pub trait StorageValueTr:
    Copy + Clone + Debug + Default + PartialEq + Eq + PartialOrd + Ord + Hash + Send + Sync + 'static
{
    /// The zero value.
    const ZERO: Self;

    /// Extract the U256 value.
    fn value(&self) -> U256;

    /// Check if the value is zero (for gas calculation purposes).
    /// For `FlaggedStorage`, this checks both value == 0 AND is_public.
    fn is_zero(&self) -> bool;

    /// Create from a U256 value (used by upstream SSTORE instruction).
    fn from_u256(value: U256) -> Self;
}

impl StorageValueTr for U256 {
    const ZERO: Self = U256::ZERO;

    #[inline]
    fn value(&self) -> U256 {
        *self
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.is_zero()
    }

    #[inline]
    fn from_u256(value: U256) -> Self {
        value
    }
}

impl StorageValueTr for FlaggedStorage {
    const ZERO: Self = FlaggedStorage::ZERO;

    #[inline]
    fn value(&self) -> U256 {
        self.value
    }

    #[inline]
    fn is_zero(&self) -> bool {
        // FlaggedStorage::is_zero checks is_public() && value.is_zero()
        FlaggedStorage::is_zero(self)
    }

    #[inline]
    fn from_u256(value: U256) -> Self {
        FlaggedStorage::new(value, false)
    }
}
