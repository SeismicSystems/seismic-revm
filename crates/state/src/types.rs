use super::{Account, EvmStorageSlot};
use primitives::{Address, HashMap, U256};

/// EVM State is a mapping from addresses to accounts.
pub type EvmState<T = U256> = HashMap<Address, Account<T>>;

/// Structure used for EIP-1153 transient storage
pub type TransientStorage = HashMap<(Address, U256), U256>;

/// An account's Storage is a mapping from 256-bit integer keys to [EvmStorageSlot]s.
pub type EvmStorage<T = U256> = HashMap<U256, EvmStorageSlot<T>>;

/// An abstraction around Slots.
pub trait StorageValue: Default + Copy + Eq + Clone + PartialEq + Eq + core::fmt::Debug {
    fn word(self) -> U256;
}

impl StorageValue for U256 {
    #[inline(always)]
    fn word(self) -> U256 { self }
}
