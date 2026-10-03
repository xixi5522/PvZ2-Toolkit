//! RTID value parsing and serialization.

use crate::error::Error;
use regex::Regex;
use serde::de::{self, Visitor};
use serde::ser::{self, SerializeTuple};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::OnceLock;

/// Resource type identifier used by PvZ2 RTON.
///
/// The binary RTON representation uses tag `0x83` for non-null RTIDs and
/// `0x84` for RTID-null. Human-readable serializers emit RTIDs as strings such
/// as `RTID(0)`, `RTID(id.group.obj@name)`, or `RTID(name@parent)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Rtid {
    /// RTID zero / null.
    Null,
    /// Format: group.id.obj@name
    Uid {
        /// RTID group.
        group: u64,
        /// RTID id.
        id: u64,
        /// RTID object component.
        obj: u32,
        /// Optional RTID name.
        name: Option<String>,
    },
    /// Format: name@parent
    Raw {
        /// Raw RTID name.
        name: String,
        /// Raw RTID parent.
        parent: String,
    },
}

impl Rtid {
    /// Formats the RTID using PvZ2 runtime decimal conventions.
    pub fn to_runtime_string(&self) -> String {
        match self {
            Rtid::Null => "RTID(0)".to_string(),
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => match name {
                Some(name) => format!("RTID({id}.{group}.{obj:08x}@{name})"),
                None if *obj == 0 => format!("RTID(:{id}.{group})"),
                None => format!("RTID(:{id}.{group}@{obj})"),
            },
            Rtid::Raw { name, parent } => format!("RTID({name}@{parent})"),
        }
    }

    /// Parses a PvZ2 runtime RTID string.
    ///
    /// This accepts runtime decimal forms such as `RTID(:1.2)` and
    /// `RTID(1.2.00000003@name)`.
    pub fn from_runtime_str(s: &str) -> Result<Self, Error> {
        static OUTER_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let outer_re = OUTER_REGEX
            .get_or_init(|| Regex::new(r"^RTID\((.*)\)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        let caps = outer_re
            .captures(s)
            .ok_or_else(|| Error::InvalidRtid("Not an RTID string".into()))?;
        let inner = caps
            .get(1)
            .ok_or_else(|| Error::InvalidRtid("Empty content".into()))?
            .as_str();

        if inner == "0" {
            return Ok(Rtid::Null);
        }

        static RUNTIME_UID_NAME_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let runtime_uid_name_re = RUNTIME_UID_NAME_REGEX
            .get_or_init(|| Regex::new(r"^(\d+)\.(\d+)\.([0-9a-fA-F]+)@(.*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = runtime_uid_name_re.captures(inner) {
            let id = caps.get(1).unwrap().as_str().parse::<u64>()?;
            let group = caps.get(2).unwrap().as_str().parse::<u64>()?;
            let obj = u32::from_str_radix(caps.get(3).unwrap().as_str(), 16)?;
            let name = caps.get(4).unwrap().as_str();

            return Ok(Rtid::Uid {
                group,
                id,
                obj,
                name: if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                },
            });
        }

        static RUNTIME_UID_NO_NAME_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let runtime_uid_no_name_re = RUNTIME_UID_NO_NAME_REGEX
            .get_or_init(|| Regex::new(r"^:(\d+)\.(\d+)@(\d+)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = runtime_uid_no_name_re.captures(inner) {
            return Ok(Rtid::Uid {
                id: caps.get(1).unwrap().as_str().parse::<u64>()?,
                group: caps.get(2).unwrap().as_str().parse::<u64>()?,
                obj: caps.get(3).unwrap().as_str().parse::<u32>()?,
                name: None,
            });
        }

        static RUNTIME_UID_SHORT_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let runtime_uid_short_re = RUNTIME_UID_SHORT_REGEX
            .get_or_init(|| Regex::new(r"^:(\d+)\.(\d+)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = runtime_uid_short_re.captures(inner) {
            return Ok(Rtid::Uid {
                id: caps.get(1).unwrap().as_str().parse::<u64>()?,
                group: caps.get(2).unwrap().as_str().parse::<u64>()?,
                obj: 0,
                name: None,
            });
        }

        static RAW_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let raw_re = RAW_REGEX
            .get_or_init(|| Regex::new(r"^([^@]+)@([^@]*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = raw_re.captures(inner) {
            return Ok(Rtid::Raw {
                name: caps.get(1).unwrap().as_str().to_string(),
                parent: caps.get(2).unwrap().as_str().to_string(),
            });
        }

        Err(Error::InvalidRtid("Inner structure mismatch".into()))
    }
}

impl fmt::Display for Rtid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rtid::Null => write!(f, "RTID(0)"),
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                if let Some(n) = name {
                    write!(f, "RTID({:x}.{:x}.{:08x}@{})", id, group, obj, n)
                } else {
                    write!(f, "RTID({:x}.{:x}.{:08x}@)", id, group, obj)
                }
            }
            Rtid::Raw { name, parent } => write!(f, "RTID({}@{})", name, parent),
        }
    }
}

impl FromStr for Rtid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Step 1: Match Outer Wrapper "RTID(...)"
        static OUTER_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let outer_re = OUTER_REGEX
            .get_or_init(|| Regex::new(r"^RTID\((.*)\)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        let caps = outer_re
            .captures(s)
            .ok_or_else(|| Error::InvalidRtid("Not an RTID string".into()))?;
        let inner = caps
            .get(1)
            .ok_or_else(|| Error::InvalidRtid("Empty content".into()))?
            .as_str();

        // Step 2: Analyze Content
        if inner == "0" {
            return Ok(Rtid::Null);
        }

        // Case B: UID (Strict Lowercase Hex) — standard .rton format
        static UID_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let uid_re = UID_REGEX
            .get_or_init(|| Regex::new(r"^([0-9a-f]+)\.([0-9a-f]+)\.([0-9a-f]+)@(.*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = uid_re.captures(inner) {
            let id_str = caps.get(1).unwrap().as_str();
            let group_str = caps.get(2).unwrap().as_str();
            let obj_str = caps.get(3).unwrap().as_str();
            let name_str = caps.get(4).unwrap().as_str();

            let id = u64::from_str_radix(id_str, 16)?;
            let group = u64::from_str_radix(group_str, 16)?;
            let obj = u32::from_str_radix(obj_str, 16)?;

            let name = if name_str.is_empty() {
                None
            } else {
                Some(name_str.to_string())
            };

            return Ok(Rtid::Uid {
                group,
                id,
                obj,
                name,
            });
        }

        // Case B2: Colon-prefixed UID (RtIdProtocol format, used in PvZ2
        //          runtime protocol messages — not in .rton files).
        //
        // Hopper artifacts at 0x1029ee9b4–0x1029ee9f1 expose printf-style
        // format strings used by RtIdProtocol:
        //   RTID(:%d.%d)            — id.group only (obj = 0, no name)
        //   RTID(:%d.%d@%d)         — id.group@obj (decimal obj, no name)
        //   RTID(%d.%d.%08x@%s)    — id.group.obj@name (decimal id/group,
        //                              hex obj, no leading colon)
        //
        // The full no-colon runtime form is ambiguous with standard .rton hex
        // strings when id/group contain only decimal digits.  Keep no-colon
        // parsing as standard hex to preserve Display/FromStr round-trips, and
        // accept the colon-prefixed full form below as an explicit extension.

        // :%d.%d.%08x@%s — most specific (decimal id, decimal group, hex obj, string name)
        static COLON_UID_NAME_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let colon_uid_name_re = COLON_UID_NAME_REGEX
            .get_or_init(|| Regex::new(r"^:(\d+)\.(\d+)\.([0-9a-f]+)@(.+)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = colon_uid_name_re.captures(inner) {
            let id = caps.get(1).unwrap().as_str().parse::<u64>()?;
            let group = caps.get(2).unwrap().as_str().parse::<u64>()?;
            let obj = u32::from_str_radix(caps.get(3).unwrap().as_str(), 16)?;
            let name = caps.get(4).unwrap().as_str();

            return Ok(Rtid::Uid {
                group,
                id,
                obj,
                name: if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                },
            });
        }

        // :%d.%d@%d — decimal id, decimal group, decimal obj, no name
        static COLON_UID_NO_NAME_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let colon_uid_no_name_re = COLON_UID_NO_NAME_REGEX
            .get_or_init(|| Regex::new(r"^:(\d+)\.(\d+)@(\d+)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = colon_uid_no_name_re.captures(inner) {
            let id = caps.get(1).unwrap().as_str().parse::<u64>()?;
            let group = caps.get(2).unwrap().as_str().parse::<u64>()?;
            let obj = caps.get(3).unwrap().as_str().parse::<u32>()?;

            return Ok(Rtid::Uid {
                group,
                id,
                obj,
                name: None,
            });
        }

        // :%d.%d — decimal id, decimal group, obj = 0, no name
        static COLON_UID_SHORT_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let colon_uid_short_re = COLON_UID_SHORT_REGEX
            .get_or_init(|| Regex::new(r"^:(\d+)\.(\d+)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = colon_uid_short_re.captures(inner) {
            let id = caps.get(1).unwrap().as_str().parse::<u64>()?;
            let group = caps.get(2).unwrap().as_str().parse::<u64>()?;

            return Ok(Rtid::Uid {
                group,
                id,
                obj: 0,
                name: None,
            });
        }

        // Case C: Raw
        static RAW_REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
        let raw_re = RAW_REGEX
            .get_or_init(|| Regex::new(r"^([^@]+)@([^@]*)$"))
            .as_ref()
            .map_err(|e| Error::Regex(e.clone()))?;

        if let Some(caps) = raw_re.captures(inner) {
            let name = caps.get(1).unwrap().as_str();
            let parent = caps.get(2).unwrap().as_str();
            return Ok(Rtid::Raw {
                name: name.to_string(),
                parent: parent.to_string(),
            });
        }

        Err(Error::InvalidRtid("Inner structure mismatch".into()))
    }
}

impl Serialize for Rtid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            Rtid::Null => serializer.serialize_unit_variant("RTID", 0x84, "Zero"),
            _ => serializer.serialize_newtype_struct("RTID", &RtidBody(self)),
        }
    }
}

impl<'de> Deserialize<'de> for Rtid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct RtidVisitor;

        impl<'de> Visitor<'de> for RtidVisitor {
            type Value = Rtid;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an RTID string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Rtid::from_str(value).map_err(E::custom)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }
        }

        deserializer.deserialize_string(RtidVisitor)
    }
}

struct RtidBody<'a>(&'a Rtid);

impl Serialize for RtidBody<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self.0 {
            Rtid::Null => serializer.serialize_unit_variant("RTID", 0x84, "Zero"),
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                if let Some(name) = name {
                    let mut tup = serializer.serialize_tuple(5)?;
                    tup.serialize_element(&OverrideByte(2))?;
                    tup.serialize_element(name)?;
                    tup.serialize_element(group)?;
                    tup.serialize_element(id)?;
                    tup.serialize_element(obj)?;
                    tup.end()
                } else {
                    let mut tup = serializer.serialize_tuple(4)?;
                    tup.serialize_element(&OverrideByte(1))?;
                    tup.serialize_element(group)?;
                    tup.serialize_element(id)?;
                    tup.serialize_element(obj)?;
                    tup.end()
                }
            }
            Rtid::Raw { name, parent } => {
                let mut tup = serializer.serialize_tuple(3)?;
                tup.serialize_element(&OverrideByte(3))?;
                tup.serialize_element(name)?;
                tup.serialize_element(parent)?;
                tup.end()
            }
        }
    }
}

struct OverrideByte(u8);
impl Serialize for OverrideByte {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_unit_variant("OverrideByte", self.0 as u32, "")
    }
}
