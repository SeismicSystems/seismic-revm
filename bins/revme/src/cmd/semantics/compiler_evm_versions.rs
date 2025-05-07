use std::fmt;

use primitives::hardfork::SpecId;
use seismic_revm::SeismicSpecId;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub(crate) enum EVMVersion {
    Homestead,
    Byzantium,
    Constantinople,
    Petersburg,
    Istanbul,
    Berlin,
    London,
    Paris,
    Shangain,
    Cancun,
    Mercury,
    Osaka,
}

// Not fully exhaustive list of versions, trying to cover all SOLIDITY VERSIONING is the goal here
impl EVMVersion {
    pub(crate) fn from_str(version: &str) -> Option<Self> {
        match version.to_lowercase().as_str() {
            "homestead" => Some(Self::Homestead),
            "byzantium" => Some(Self::Byzantium),
            "constantinople" => Some(Self::Constantinople),
            "petersburg" => Some(Self::Petersburg),
            "istanbul" => Some(Self::Istanbul),
            "berlin" => Some(Self::Berlin),
            "london" => Some(Self::London),
            "paris" => Some(Self::Paris),
            "shanghai" => Some(Self::Shangain),
            "cancun" => Some(Self::Cancun),
            "mercury" => Some(Self::Mercury),
            "osaka" => Some(Self::Osaka),
            _ => None,
        }
    }

    pub(crate) fn previous(&self) -> Option<&'static str> {
        match self {
            EVMVersion::Osaka => Some("mercury"),
            EVMVersion::Mercury => Some("cancun"),
            EVMVersion::Cancun => Some("shanghai"),
            EVMVersion::Shangain => Some("paris"),
            EVMVersion::Paris => Some("London"),
            EVMVersion::London => Some("berlin"),
            EVMVersion::Berlin => Some("istanbul"),
            EVMVersion::Istanbul => Some("constantinople"),
            EVMVersion::Constantinople => Some("byzantium"),
            EVMVersion::Petersburg => Some("byzantium"),
            EVMVersion::Byzantium => Some("homestead"),
            EVMVersion::Homestead => None,
        }
    }
    pub(crate) fn next(&self) -> Option<&'static str> {
        match self {
            EVMVersion::Homestead => Some("byzantium"),
            EVMVersion::Byzantium => Some("constantinople"),
            EVMVersion::Constantinople => Some("istanbul"),
            EVMVersion::Petersburg => Some("istanbul"),
            EVMVersion::Istanbul => Some("berlin"),
            EVMVersion::Berlin => Some("london"),
            EVMVersion::London => Some("arrowglacier"),
            EVMVersion::Paris => Some("shanghai"),
            EVMVersion::Shangain => Some("cancun"),
            EVMVersion::Cancun => Some("mercury"),
            EVMVersion::Mercury => Some("osaka"),
            EVMVersion::Osaka => None,
        }
    }
}

impl fmt::Display for EVMVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let version_str = match self {
            EVMVersion::Homestead => "homestead",
            EVMVersion::Byzantium => "byzantium",
            EVMVersion::Constantinople => "constantinople",
            EVMVersion::Petersburg => "petersburg",
            EVMVersion::Istanbul => "istanbul",
            EVMVersion::Berlin => "berlin",
            EVMVersion::London => "london",
            EVMVersion::Paris => "paris",
            EVMVersion::Shangain => "shanghai",
            EVMVersion::Cancun => "cancun",
            EVMVersion::Mercury => "mercury",
            EVMVersion::Osaka => "osaka",
        };
        write!(f, "{}", version_str)
    }
}

impl EVMVersion {
    fn parse_version_token(tok: &str) -> (&str, &str) {
        let tok = tok.trim();
        for op in ["<=", "<", ">=", ">", "="] {
            if let Some(rest) = tok.strip_prefix(op) {
                return (op, rest.trim());
            }
        }
        ("=", tok)
    }

    fn apply_cmp(op: &str, ver: Self) -> Option<Self> {
        match op {
            "<" => ver.previous().and_then(Self::from_str),
            "<=" | "=" => Some(ver),
            ">" => ver.next().and_then(Self::from_str),
            ">=" => Some(ver),
            _ => None,
        }
    }

    pub(crate) fn extract(content: &str) -> Option<Self> {
        let header = content.split("// ====").nth(1)?;

        // 1. Prefer an explicit "// EVMVersion:" tag
        for line in header.lines() {
            if let Some(v_part) = line.trim().strip_prefix("// EVMVersion:") {
                let (op, ver_str) = Self::parse_version_token(v_part);
                let base = Self::from_str(ver_str)?;
                return Self::apply_cmp(op, base);
            }
        }

        // 2. Fallback: new "// bytecodeFormat:" tag
        for line in header.lines() {
            if let Some(fmt) = line.trim().strip_prefix("// bytecodeFormat:") {
                return match fmt.trim() {
                    "legacy" | "legacy,>=EOFv1" | ">=EOFv1,legacy" => Some(Self::Mercury), // default
                    ">=EOFv1" => Some(Self::Osaka),
                    _ => None, // unknown / future flag
                };
            }
        }

        None
    }

    pub fn to_spec_id(&self) -> SpecId {
        match self {
            EVMVersion::Homestead => SpecId::HOMESTEAD,
            EVMVersion::Byzantium => SpecId::BYZANTIUM,
            EVMVersion::Constantinople => SpecId::PETERSBURG,
            EVMVersion::Petersburg => SpecId::PETERSBURG,
            EVMVersion::Istanbul => SpecId::ISTANBUL,
            EVMVersion::Berlin => SpecId::BERLIN,
            EVMVersion::London => SpecId::LONDON,
            EVMVersion::Paris => SpecId::MERGE,
            EVMVersion::Shangain => SpecId::SHANGHAI,
            EVMVersion::Cancun => SpecId::CANCUN,
            EVMVersion::Mercury => {
                panic!("Mercury cannot be converted to a mainnet SpecId. Use to_seismic_spec_id() instead.")
            }
            EVMVersion::Osaka => SpecId::OSAKA,
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

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "// ====\n";

    #[test]
    fn explicit_version_beats_bytecode_format() {
        let s = format!("{HEADER}// EVMVersion: >= Osaka\n// bytecodeFormat: legacy\n");
        assert_eq!(EVMVersion::extract(&s), Some(EVMVersion::Osaka));
    }

    #[test]
    fn bytecode_format_legacy() {
        let s = format!("{HEADER}// bytecodeFormat: legacy\n");
        assert_eq!(EVMVersion::extract(&s), Some(EVMVersion::Mercury));
    }

    #[test]
    fn bytecode_format_eof() {
        let s = format!("{HEADER}// bytecodeFormat: >=EOFv1\n");
        assert_eq!(EVMVersion::extract(&s), Some(EVMVersion::Osaka));
    }
}
