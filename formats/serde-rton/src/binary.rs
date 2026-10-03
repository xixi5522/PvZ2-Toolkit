//! Binary blob support for RTON.
//!
//! RTON binary blobs carry both a printable hex string and a raw byte payload.
//! [`BinaryBlob`] stores the semantic raw bytes. In human-readable serializers
//! such as `serde_json`, [`crate::value::Value::Binary`] is emitted as a
//! `$BINARY("HEX", len)` string.

use crate::error::Error;
use regex::Regex;
use serde::{Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

/// Raw bytes stored in an RTON BinaryBlob value.
///
/// `Display` renders the PvZ-style textual form:
/// `$BINARY("0A0B", 2)`.
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
        // Format: $BINARY("HEX_STRING", RAW_LENGTH)
        static BINARY_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();

        // Regex matches: $BINARY("...", 123)
        let re = BINARY_REGEX
            .get_or_init(|| Regex::new(r#"^\$BINARY\("([0-9a-fA-F]*)",\s*([0-9]+)\)$"#))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        // 1. Validate structure
        let caps = re.captures(s).ok_or_else(|| {
            Error::InvalidBinaryBlob("Format mismatch, expected $BINARY(\"hex\", len)".into())
        })?;

        // 2. Safe extraction of capture groups (removing unwrap)
        let hex_str = caps
            .get(1)
            .ok_or_else(|| Error::InvalidBinaryBlob("Missing hex string capture".into()))?
            .as_str();

        let len_str = caps
            .get(2)
            .ok_or_else(|| Error::InvalidBinaryBlob("Missing length capture".into()))?
            .as_str();

        // 3. Parse length and content
        let declared_len = u64::from_str(len_str)
            .map_err(|_| Error::InvalidBinaryBlob("Invalid length number".into()))?;

        // Validate consistency: Hex string length (chars) vs Declared Byte Length
        if (hex_str.len() as u64) / 2 != declared_len {
            return Err(Error::InvalidBinaryBlob(format!(
                "Length mismatch: declared {}, but hex string contains {} bytes",
                declared_len,
                hex_str.len() / 2
            )));
        }

        let mut bytes = Vec::with_capacity(hex_str.len() / 2);
        for i in (0..hex_str.len()).step_by(2) {
            // ? automatically converts ParseIntError -> Error::ParseInt
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
        write!(f, "\", {})", self.0.len())
    }
}
