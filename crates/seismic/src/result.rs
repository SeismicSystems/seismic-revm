use revm::{context::DBErrorMarker, context_interface::result::HaltReason};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeismicDbError<BaseDbError> {
    Base(BaseDbError),
    InvalidPrivateStorageAccess,
    InvalidPublicStorageAccess,
}

// Implement the DBErrorMarker trait
impl<T: DBErrorMarker> DBErrorMarker for SeismicDbError<T> {}

// Conversion from base DB error
impl<T> From<T> for SeismicDbError<T> {
    fn from(err: T) -> Self {
        Self::Base(err)
    }
}

// Conversion to SeismicHaltReason
impl<T> From<SeismicDbError<T>> for SeismicHaltReason 
where 
    T: DBErrorMarker,
    HaltReason: From<T>, 
{
    fn from(err: SeismicDbError<T>) -> Self {
        match err {
            SeismicDbError::Base(err) => SeismicHaltReason::Base(HaltReason::from(err)), // Convert err to HaltReason
            SeismicDbError::InvalidPrivateStorageAccess => SeismicHaltReason::InvalidPrivateStorageAccess,
            SeismicDbError::InvalidPublicStorageAccess => SeismicHaltReason::InvalidPublicStorageAccess,
        }
    }
}
