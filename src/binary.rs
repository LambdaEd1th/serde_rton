use crate::error::Error;
use regex::Regex;
use serde::{Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryBlob(pub Vec<u8>);

impl Serialize for BinaryBlob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl FromStr for BinaryBlob {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // C# Format: $BINARY("HEX_STRING", RAW_LENGTH)
        // Regex matches: $BINARY("...", 123)
        static BINARY_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();

        let re = BINARY_REGEX
            .get_or_init(|| Regex::new(r#"^\$BINARY\("([0-9a-fA-F]*)",\s*([0-9]+)\)$"#))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        let caps = re.captures(s).ok_or_else(|| {
            Error::InvalidBinaryBlob("Format mismatch, expected $BINARY(\"hex\", len)".into())
        })?;

        let hex_str = caps.get(1).unwrap().as_str();
        let len_str = caps.get(2).unwrap().as_str();

        let declared_len = u64::from_str(len_str)
            .map_err(|_| Error::InvalidBinaryBlob("Invalid length number".into()))?;

        // Hex string length implies byte length (2 chars = 1 byte)
        if (hex_str.len() as u64) / 2 != declared_len {
            return Err(Error::InvalidBinaryBlob(format!(
                "Length mismatch: declared {}, but hex encodes {}",
                declared_len,
                hex_str.len() / 2
            )));
        }

        let mut bytes = Vec::with_capacity(hex_str.len() / 2);
        for i in (0..hex_str.len()).step_by(2) {
            let b = u8::from_str_radix(&hex_str[i..i + 2], 16)?;
            bytes.push(b);
        }

        Ok(BinaryBlob(bytes))
    }
}

impl fmt::Display for BinaryBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "$BINARY(\"")?;
        for b in &self.0 {
            write!(f, "{:02X}", b)?;
        }
        // Match C# format: ", {len})"
        write!(f, "\", {})", self.0.len())
    }
}
