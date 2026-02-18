use revm::primitives::{Address, HashMap, StorageKey, U256};
use revm::state::FlaggedStorage;
use serde::{de, Deserialize};

/// Deserializes a [string][String] as a [u64].
pub fn deserialize_str_as_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;

    if let Some(stripped) = string.strip_prefix("0x") {
        u64::from_str_radix(stripped, 16)
    } else {
        string.parse()
    }
    .map_err(serde::de::Error::custom)
}

/// Deserializes a [string][String] as an optional [Address].
pub fn deserialize_maybe_empty<'de, D>(deserializer: D) -> Result<Option<Address>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    if string.is_empty() {
        Ok(None)
    } else {
        string.parse().map_err(de::Error::custom).map(Some)
    }
}

/// Deserializes a storage HashMap from the upstream Ethereum test format (hex string values)
/// into public `FlaggedStorage` values.
/// This is needed so that we can deserialize the Ethereum test fixtures, which assume Storage values are U256 hex strings.
pub fn deserialize_storage<'de, D>(
    deserializer: D,
) -> Result<HashMap<StorageKey, FlaggedStorage>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let map: HashMap<StorageKey, U256> = HashMap::deserialize(deserializer)?;
    Ok(map
        .into_iter()
        .map(|(key, value)| (key, FlaggedStorage::public(value)))
        .collect())
}
