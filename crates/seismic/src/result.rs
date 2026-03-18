use core::fmt;

use revm::primitives::Bytes;

/// Seismic-specific EVM revert reasons.
///
/// These are injected as standard EVM reverts (`InstructionResult::Revert`) with
/// the reason encoded in the revert output bytes. This means:
///
/// - **Callers can catch them**: an outer contract using try/catch will see a revert
///   with `revert_bytes()` as output, rather than a halt which returns no data.
/// - **RPC surfaces them clearly**: `eth_call` returns `"execution reverted: <reason>"`
///   with the reason bytes as data, giving developers a clear error message.
/// - **Gas is preserved**: only gas actually consumed is charged, unlike a halt which
///   burns all gas allocated to the frame.
///
/// See `revert_with_reason()` in `confidential_storage.rs` for the injection point.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SeismicRevertReason {
    /// SLOAD/SSTORE on a private storage slot.
    InvalidPrivateStorageAccess,
    /// CSTORE on a non-zero public storage slot.
    InvalidPublicStorageAccess,
}

impl SeismicRevertReason {
    /// Returns the reason encoded as bytes for inclusion in revert output.
    pub fn revert_bytes(&self) -> Bytes {
        match self {
            Self::InvalidPublicStorageAccess => Bytes::from_static(b"InvalidPublicStorageAccess"),
            Self::InvalidPrivateStorageAccess => Bytes::from_static(b"InvalidPrivateStorageAccess"),
        }
    }
}

impl fmt::Display for SeismicRevertReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrivateStorageAccess => write!(f, "InvalidPrivateStorageAccess"),
            Self::InvalidPublicStorageAccess => write!(f, "InvalidPublicStorageAccess"),
        }
    }
}
