use std::fmt;

use primitives::hardfork::SpecId;
use seismic_revm::SeismicSpecId;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub(crate) enum EVMVersion {
    Homestead,
    Byzantium,
    Constantinople,
    Istanbul,
    Berlin,
    London,
    Paris,
    Shangain,
    Cancun,
    Mercury,
}

// Not fully exhaustive list of versions, trying to cover all SOLIDITY VERSIONING is the goal here
impl EVMVersion {
    pub(crate) fn from_str(version: &str) -> Option<Self> {
        match version.to_lowercase().as_str() {
            "homestead" => Some(Self::Homestead),
            "byzantium" => Some(Self::Byzantium),
            "constantinople" => Some(Self::Constantinople),
            "istanbul" => Some(Self::Istanbul),
            "berlin" => Some(Self::Berlin),
            "london" => Some(Self::London),
            "paris" => Some(Self::Paris),
            "shanghai" => Some(Self::Shangain),
            "cancun" => Some(Self::Cancun),
            "mercury" => Some(Self::Mercury),
            _ => None,
        }
    }

    pub(crate) fn previous(&self) -> Option<&'static str> {
        match self {
            EVMVersion::Mercury => Some("cancun"),
            EVMVersion::Cancun => Some("shanghai"),
            EVMVersion::Shangain => Some("paris"),
            EVMVersion::Paris => Some("London"),
            EVMVersion::London => Some("berlin"),
            EVMVersion::Berlin => Some("istanbul"),
            EVMVersion::Istanbul => Some("constantinople"),
            EVMVersion::Constantinople => Some("byzantium"),
            EVMVersion::Byzantium => Some("homestead"),
            EVMVersion::Homestead => None,
        }
    }
    pub(crate) fn next(&self) -> Option<&'static str> {
        match self {
            EVMVersion::Homestead => Some("byzantium"),
            EVMVersion::Byzantium => Some("constantinople"),
            EVMVersion::Constantinople => Some("istanbul"),
            EVMVersion::Istanbul => Some("berlin"),
            EVMVersion::Berlin => Some("london"),
            EVMVersion::London => Some("arrowglacier"),
            EVMVersion::Paris => Some("shanghai"),
            EVMVersion::Shangain => Some("cancun"),
            EVMVersion::Cancun => Some("mercury"),
            EVMVersion::Mercury => None,
        }
    }
}

impl fmt::Display for EVMVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let version_str = match self {
            EVMVersion::Homestead => "homestead",
            EVMVersion::Byzantium => "byzantium",
            EVMVersion::Constantinople => "constantinople",
            EVMVersion::Istanbul => "istanbul",
            EVMVersion::Berlin => "berlin",
            EVMVersion::London => "london",
            EVMVersion::Paris => "paris",
            EVMVersion::Shangain => "shanghai",
            EVMVersion::Cancun => "cancun",
            EVMVersion::Mercury => "mercury",
        };
        write!(f, "{}", version_str)
    }
}

impl EVMVersion {
    pub(crate) fn extract(content: &str) -> Option<Self> {
        let parts: Vec<&str> = content.split("// ====").collect();
        if parts.len() < 2 {
            return None;
        }

        for line in parts[1].lines() {
            if let Some(version_part) = line.trim().strip_prefix("// EVMVersion:") {
                let version_str = version_part.trim();

                let (comparison, version) = if version_str.starts_with("<=") {
                    ("<=", version_str.trim_start_matches("<=").trim())
                } else if version_str.starts_with('<') {
                    ("<", version_str.trim_start_matches('<').trim())
                } else if version_str.starts_with(">=") {
                    (">=", version_str.trim_start_matches(">=").trim())
                } else if version_str.starts_with('>') {
                    (">", version_str.trim_start_matches('>').trim())
                } else if version_str.starts_with('=') {
                    ("=", version_str.trim_start_matches('=').trim())
                } else {
                    ("=", version_str)
                };

                if let Some(ev_version) = EVMVersion::from_str(version) {
                    return match comparison {
                        "<" => ev_version.previous().and_then(EVMVersion::from_str),
                        "<=" | "=" => Some(ev_version),
                        ">" => ev_version.next().and_then(EVMVersion::from_str),
                        ">=" => Some(ev_version),
                        _ => None,
                    };
                }
            }
        }
        None
    }
}

impl EVMVersion {
    pub fn to_spec_id(&self) -> SpecId {
        match self {
            EVMVersion::Homestead => SpecId::HOMESTEAD,
            EVMVersion::Byzantium => SpecId::BYZANTIUM,
            EVMVersion::Constantinople => SpecId::CONSTANTINOPLE,
            EVMVersion::Istanbul => SpecId::ISTANBUL,
            EVMVersion::Berlin => SpecId::BERLIN,
            EVMVersion::London => SpecId::LONDON,
            EVMVersion::Paris => SpecId::MERGE,
            EVMVersion::Shangain => SpecId::SHANGHAI,
            EVMVersion::Cancun => SpecId::CANCUN,
            EVMVersion::Mercury => {
                panic!("Mercury cannot be converted to a mainnet SpecId. Use to_seismic_spec_id() instead.")
            }
        }
    }

    pub fn to_seismic_spec_id(&self) -> SeismicSpecId {
        match self {
            EVMVersion::Mercury => SeismicSpecId::MERCURY,
            _ => {
                panic!("Only Mercury can be converted to a SeismicSpecId")
            }
        }
    }
}
