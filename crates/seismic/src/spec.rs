use core::str::FromStr;
use revm::primitives::hardfork::{name as eth_name, SpecId, UnknownHardfork};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(non_camel_case_types)]
pub enum SeismicSpecId {
    #[default]
    MERCURY = 100,
}

impl SeismicSpecId {
    /// Converts the [`SeismicSpecId`] into a [`SpecId`].
    pub const fn into_eth_spec(self) -> SpecId {
        match self {
            Self::MERCURY  => SpecId::PRAGUE,
        }
    }

    pub const fn is_enabled_in(self, other: SeismicSpecId) -> bool {
        other as u8 <= self as u8
    }
}

impl From<SeismicSpecId> for SpecId {
    fn from(spec: SeismicSpecId) -> Self {
        spec.into_eth_spec()
    }
}

impl FromStr for SeismicSpecId {
    type Err = UnknownHardfork;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            name::MERCURY => Ok(SeismicSpecId::MERCURY),
            _ => Err(UnknownHardfork),
        }
    }
}

impl From<SeismicSpecId> for &'static str {
    fn from(spec_id: SeismicSpecId) -> Self {
        match spec_id {
            SeismicSpecId::MERCURY => name::MERCURY,
        }
    }
}

/// String identifiers for Optimism hardforks
pub mod name {
    pub const MERCURY: &str = "Mercury";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn test_seismic_spec_id_eth_spec_compatibility() {
        // Define test cases: (SeismicSpecId, enabled in ETH specs, enabled in Seismic specs)
        let test_cases = [
            (
                SeismicSpecId::MERCURY,
                vec![
                    (SpecId::MERGE, true),
                    (SpecId::SHANGHAI, true),
                    (SpecId::CANCUN, true),
                    (SpecId::OSAKA, false),
                ],
                vec![(SeismicSpecId::MERCURY, true)],
            ),
        ];

        for (seismic_spec, eth_tests, seismic_tests) in test_cases {
            // Test ETH spec compatibility
            for (eth_spec, expected) in eth_tests {
                assert_eq!(
                    seismic_spec.into_eth_spec().is_enabled_in(eth_spec),
                    expected,
                    "{:?} should {} be enabled in ETH {:?}",
                    seismic_spec,
                    if expected { "" } else { "not " },
                    eth_spec
                );
            }

            // Test Seismic spec compatibility
            for (other_seismic_spec, expected) in seismic_tests {
                assert_eq!(
                    seismic_spec.is_enabled_in(other_seismic_spec),
                    expected,
                    "{:?} should {} be enabled in Seismic {:?}",
                    seismic_spec,
                    if expected { "" } else { "not " },
                    other_seismic_spec
                );
            }
        }
    }
}

