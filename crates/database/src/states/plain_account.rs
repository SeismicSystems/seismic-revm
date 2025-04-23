use primitives::{HashMap, U256};
use state::{AccountInfo, EvmStorageSlot, StorageValue};

// Plain account of StateDatabase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlainAccount<V: StorageValue = U256> {
    pub info: AccountInfo,
    pub storage: PlainStorage<V>,
}

impl <V: StorageValue> PlainAccount<V> {
    pub fn new_empty_with_storage(storage: PlainStorage<V>) -> Self {
        Self {
            info: AccountInfo::default(),
            storage,
        }
    }

    pub fn into_components(self) -> (AccountInfo, PlainStorage<V>) {
        (self.info, self.storage)
    }
}

/// This type keeps track of the current value of a storage slot.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageSlot<V: StorageValue = U256> {
    /// The value of the storage slot before it was changed.
    ///
    /// When the slot is first loaded, this is the original value.
    ///
    /// If the slot was not changed, this is equal to the present value.
    pub previous_or_original_value: V,
    /// When loaded with sload present value is set to original value
    pub present_value: V,
}

impl<V: StorageValue> From<EvmStorageSlot<V>> for StorageSlot<V> {
    fn from(value: EvmStorageSlot<V>) -> Self {          
        Self::new_changed(value.original_value, value.present_value)
    }
}

impl <V: StorageValue> StorageSlot<V> {
    /// Creates a new _unchanged_ `StorageSlot` for the given value.
    pub fn new(original: V) -> Self {
        Self {
            previous_or_original_value: original,
            present_value: original,
        }
    }

    /// Creates a new _changed_ `StorageSlot`.
    pub fn new_changed(
        previous_or_original_value: V,
        present_value: V,
    ) -> Self {
        Self {
            previous_or_original_value,
            present_value,
        }
    }

    /// Returns true if the present value differs from the original value
    pub fn is_changed(&self) -> bool {
        self.previous_or_original_value != self.present_value
    }

    /// Returns the original value of the storage slot.
    pub fn original_value(&self) -> V {
        self.previous_or_original_value
    }

    /// Returns the current value of the storage slot.
    pub fn present_value(&self) -> V {
        self.present_value
    }
}

/// This storage represent values that are before block changed.
///
/// Note: Storage that we get EVM contains original values before block changed.
pub type StorageWithOriginalValues<V: StorageValue = U256> = HashMap<U256, StorageSlot<V>>;

/// Simple plain storage that does not have previous value.
/// This is used for loading from database, cache and for bundle state.
pub type PlainStorage<V: StorageValue = U256> = HashMap<U256, V>;

impl<V: StorageValue> From<AccountInfo> for PlainAccount<V> {
    fn from(info: AccountInfo) -> Self {
        Self {
            info,
            storage: HashMap::default(),
        }
    }
}
