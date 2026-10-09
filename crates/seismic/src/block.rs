//! Seismic's seconds-based block environment with sub-second precision.

use core::ops::{Deref, DerefMut};
use revm::{
    context::BlockEnv,
    context_interface::block::{BlobExcessGasAndPrice, Block},
    primitives::{Address, B256, U256},
};

/// Standard Ethereum block environment plus Seismic's millisecond component.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SeismicBlockEnv {
    /// Inner environment; its timestamp is Unix seconds.
    pub inner: BlockEnv,
    /// Sub-second millisecond component, in `0..1000` for a valid block.
    pub timestamp_millis_part: u64,
}

impl SeismicBlockEnv {
    /// Returns the full Unix millisecond timestamp exposed by `TIMESTAMPMS`.
    pub fn timestamp_millis(&self) -> U256 {
        self.inner
            .timestamp
            .saturating_mul(U256::from(1000))
            .saturating_add(U256::from(self.timestamp_millis_part))
    }
}

impl From<BlockEnv> for SeismicBlockEnv {
    fn from(inner: BlockEnv) -> Self {
        Self {
            inner,
            timestamp_millis_part: 0,
        }
    }
}

impl Deref for SeismicBlockEnv {
    type Target = BlockEnv;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SeismicBlockEnv {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Block for SeismicBlockEnv {
    fn number(&self) -> U256 {
        self.inner.number()
    }

    fn beneficiary(&self) -> Address {
        self.inner.beneficiary()
    }

    fn timestamp(&self) -> U256 {
        self.inner.timestamp()
    }

    fn gas_limit(&self) -> u64 {
        self.inner.gas_limit()
    }

    fn basefee(&self) -> u64 {
        self.inner.basefee()
    }

    fn difficulty(&self) -> U256 {
        self.inner.difficulty()
    }

    fn prevrandao(&self) -> Option<B256> {
        self.inner.prevrandao()
    }

    fn blob_excess_gas_and_price(&self) -> Option<BlobExcessGasAndPrice> {
        self.inner.blob_excess_gas_and_price()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn timestamp_keeps_seconds_and_recombines_millis() {
        for (seconds, part, millis) in [
            (0, 0, 0),
            (0, 999, 999),
            (1_700_000_000u64, 123, 1_700_000_000_123u64),
        ] {
            let block = SeismicBlockEnv {
                inner: BlockEnv {
                    timestamp: U256::from(seconds),
                    ..Default::default()
                },
                timestamp_millis_part: part,
            };
            assert_eq!(Block::timestamp(&block), U256::from(seconds));
            assert_eq!(block.timestamp_millis(), U256::from(millis));
        }
    }

    #[test]
    fn millisecond_timestamp_saturates_on_overflow() {
        let mut block = SeismicBlockEnv {
            inner: BlockEnv {
                timestamp: U256::MAX,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(block.timestamp_millis(), U256::MAX);

        block.timestamp = U256::MAX / U256::from(1000);
        block.timestamp_millis_part = 999;
        assert_eq!(block.timestamp_millis(), U256::MAX);
    }

    #[test]
    fn standard_environment_conversion_has_zero_millis_part() {
        let inner = BlockEnv {
            timestamp: U256::from(42),
            ..Default::default()
        };
        let mut block = SeismicBlockEnv::from(inner.clone());
        assert_eq!(block.inner, inner);
        assert_eq!(block.timestamp_millis_part, 0);
        assert_eq!(block.timestamp_millis(), U256::from(42_000));

        block.timestamp = U256::from(43);
        assert_eq!(block.timestamp_millis(), U256::from(43_000));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trip_preserves_millis_part() {
        let block = SeismicBlockEnv {
            inner: BlockEnv {
                timestamp: U256::from(1_700_000_000),
                ..Default::default()
            },
            timestamp_millis_part: 123,
        };
        let encoded = serde_json::to_string(&block).unwrap();
        let decoded: SeismicBlockEnv = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, block);
    }
}
