use super::{Account, EvmStorageSlot};
use primitives::{Address, HashMap, StorageKey, StorageValue, StorageValueTr, U256};

/// EVM State is a mapping from addresses to accounts.
pub type EvmState<SV = U256> = HashMap<Address, Account<SV>>;

/// Structure used for EIP-1153 transient storage
pub type TransientStorage = HashMap<(Address, StorageKey), StorageValue>;

/// An account's Storage is a mapping from 256-bit integer keys to [EvmStorageSlot]s.
pub type EvmStorage<SV: StorageValueTr = U256> = HashMap<StorageKey, EvmStorageSlot<SV>>;
