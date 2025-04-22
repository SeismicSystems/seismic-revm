

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlaggedStorage {
    pub word: U256,
    pub is_private: bool,
}

pub trait PrivateSlot: StorageValue {
    fn is_private(&self) -> bool;
}

impl StorageValue for FlaggedStorage {
    #[inline(always)]
    fn word(self) -> U256 { self.word }
}

impl PrivateSlot for FlaggedStorage {
    fn is_private(&self) -> bool {
        self.is_private
    }
}
