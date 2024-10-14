use std::fmt;

#[derive(Debug, Clone)]
pub(crate) enum EVMVersion {
    Homestead,
    Byzantium,
    Constantinople,
    Istanbul,
    Berlin,
    London,
    Paris,
    Shangain,
    Cancun
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
            _ => None,
        }
    }

    pub(crate) fn previous(&self) -> Option<&'static str> {
        match self {
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
            EVMVersion::Cancun => None,
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
        };
        write!(f, "{}", version_str)
    }
}


