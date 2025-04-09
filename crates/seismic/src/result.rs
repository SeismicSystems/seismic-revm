use revm::context_interface::result::HaltReason;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SeismicHaltReason {
    Base(HaltReason),
    /// Invalid Private Storage Access: Cannot access private storage with public instructions
    InvalidPrivateStorageAccess,
    /// Invalid Public Storage Access: Cannot access public storage with private instructions
    InvalidPublicStorageAccess,
}

impl From<HaltReason> for SeismicHaltReason {
    fn from(value: HaltReason) -> Self {
        Self::Base(value)
    }
}

