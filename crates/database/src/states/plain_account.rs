use primitives::{HashMap, StorageKey, StorageValueTr, U256};
use state::{AccountInfo, EvmStorageSlot};

/// Plain account of StateDatabase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlainAccount<SV: StorageValueTr = U256> {
    /// Account information.
    pub info: AccountInfo,
    /// Account storage.
    pub storage: PlainStorage<SV>,
}

impl<SV: StorageValueTr> PlainAccount<SV> {
    /// Creates a new empty account with the given storage.
    pub fn new_empty_with_storage(storage: PlainStorage<SV>) -> Self {
        Self {
            info: AccountInfo::default(),
            storage,
        }
    }

    /// Consumes the account and returns its components.
    pub fn into_components(self) -> (AccountInfo, PlainStorage<SV>) {
        (self.info, self.storage)
    }
}

/// This type keeps track of the current value of a storage slot.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(bound = "SV: serde::Serialize + serde::de::DeserializeOwned"))]
pub struct StorageSlot<SV: StorageValueTr = U256> {
    /// The value of the storage slot before it was changed.
    ///
    /// When the slot is first loaded, this is the original value.
    ///
    /// If the slot was not changed, this is equal to the present value.
    pub previous_or_original_value: SV,
    /// When loaded with sload present value is set to original value
    pub present_value: SV,
}

impl<SV: StorageValueTr> From<EvmStorageSlot<SV>> for StorageSlot<SV> {
    fn from(value: EvmStorageSlot<SV>) -> Self {
        Self::new_changed(value.original_value, value.present_value)
    }
}

impl<SV: StorageValueTr> StorageSlot<SV> {
    /// Creates a new _unchanged_ `StorageSlot` for the given value.
    pub fn new(original: SV) -> Self {
        Self {
            previous_or_original_value: original,
            present_value: original,
        }
    }

    /// Creates a new _changed_ `StorageSlot`.
    pub fn new_changed(
        previous_or_original_value: SV,
        present_value: SV,
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
    pub fn original_value(&self) -> SV {
        self.previous_or_original_value
    }

    /// Returns the current value of the storage slot.
    pub fn present_value(&self) -> SV {
        self.present_value
    }
}

/// This storage represent values that are before block changed.
///
/// Note: Storage that we get EVM contains original values before block changed.
pub type StorageWithOriginalValues<SV = U256> = HashMap<StorageKey, StorageSlot<SV>>;

/// Simple plain storage that does not have previous value.
/// This is used for loading from database, cache and for bundle state.
pub type PlainStorage<SV = U256> = HashMap<U256, SV>;

impl<SV: StorageValueTr> From<AccountInfo> for PlainAccount<SV> {
    fn from(info: AccountInfo) -> Self {
        Self {
            info,
            storage: HashMap::default(),
        }
    }
}
