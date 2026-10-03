//! Serde-based reader and writer for PopCap/PvZ2 RTON files.
//!
//! RTON is a binary object notation used by PopCap framework games such as
//! Plants vs. Zombies 2. This crate implements Serde serializers and
//! deserializers for the binary format, plus a dynamic [`Value`] tree for
//! schema-less editing.
//!
//! The crate focuses on binary RTON. For JSON output or input, use standard
//! Serde formats such as `serde_json` directly with [`Value`] or your own
//! structs.
//!
//! # Reading and writing typed data
//!
//! ```no_run
//! use serde::{Deserialize, Serialize};
//! use serde_rton::{from_bytes, to_bytes};
//!
//! #[derive(Debug, Deserialize, Serialize)]
//! struct Config {
//!     objclass: String,
//!     enabled: bool,
//! }
//!
//! # fn main() -> serde_rton::Result<()> {
//! let bytes = std::fs::read("config.rton")?;
//! let mut config: Config = from_bytes(&bytes)?;
//! config.enabled = true;
//! std::fs::write("config.rton", to_bytes(&config)?)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Reading and writing dynamic values
//!
//! ```no_run
//! use serde_rton::{from_bytes, to_bytes, Value};
//!
//! # fn main() -> serde_rton::Result<()> {
//! let bytes = std::fs::read("config.rton")?;
//! let mut value: Value = from_bytes(&bytes)?;
//!
//! if let Value::Object(entries) = &mut value {
//!     entries.push(("enabled".to_string(), Value::Bool(true)));
//! }
//!
//! std::fs::write("config.rton", to_bytes(&value)?)?;
//! # Ok(())
//! # }
//! ```
//!
//! # JSON interop
//!
//! ```no_run
//! use serde_rton::{from_bytes, Value};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("config.rton")?;
//! let value: Value = from_bytes(&bytes)?;
//! let json = serde_json::to_string_pretty(&value)?;
//! std::fs::write("config.json", json)?;
//! # Ok(())
//! # }
//! ```
//!
//! JSON is a semantic text representation here, not an exact RTON tag-preserving
//! bridge. Numeric widths, VarInt tags, and binary blob tags are not preserved
//! by plain `serde_json`.

pub mod binary;
pub mod crypto;
pub mod de;
pub mod error;
pub mod rtid;
pub mod ser;
pub mod tags;
pub mod value;
pub mod varint;

pub use binary::BinaryBlob;
pub use error::{Error, Result};
pub use rtid::Rtid;
pub use tags::{RtidPayloadTag, RtonTag};
pub use value::Value;
pub use varint::{DirectStr, VarInt};

pub use crypto::{decrypt_data, encrypt_data};
pub use de::{Deserializer, from_bytes, from_reader};
pub use ser::{Serializer, to_bytes, to_compact_bytes, to_compact_writer, to_writer};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{COMPACT_FILE_VERSION, FILE_FOOTER, FILE_HEADER, STANDARD_FILE_VERSION};
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;
    use std::str::FromStr;

    #[derive(Serialize)]
    struct VarIntU32(u32);

    #[derive(Serialize)]
    struct VarIntU64(u64);

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct SignedFields {
        small_i32: i32,
        negative_i32: i32,
        small_i64: i64,
        negative_i64: i64,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct UnsignedFields {
        small_u32: u32,
        large_u32: u32,
        small_u64: u64,
        large_u64: u64,
    }

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct DirectStringFields {
        #[serde(rename = "cached")]
        cached_name: String,
        #[serde(rename = "direct")]
        direct_tag: DirectStr<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct OptionalRtidField {
        a: Option<Rtid>,
    }

    fn payload_tag(bytes: &[u8]) -> u8 {
        bytes[FILE_HEADER.len() + 4]
    }

    fn round_trip(value: &Value) -> Result<Value> {
        let bytes = to_bytes(value)?;
        from_bytes(&bytes)
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn patch_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn compact_file_prefix() -> Vec<u8> {
        compact_file_prefix_with_version(COMPACT_FILE_VERSION)
    }

    fn standard_file_prefix() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(FILE_HEADER);
        bytes.extend_from_slice(&STANDARD_FILE_VERSION.to_le_bytes());
        bytes
    }

    fn compact_file_prefix_with_version(version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(FILE_HEADER);
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes
    }

    fn finish_compact_file(bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(FILE_FOOTER);
    }

    fn push_standard_latin1_def(bytes: &mut Vec<u8>, value: &str) {
        let char_count = value.chars().count();
        assert!(char_count < 0x80);

        bytes.push(RtonTag::StringLatin1Definition as u8);
        bytes.push(char_count as u8);
        for ch in value.chars() {
            assert!((ch as u32) <= 0xff);
            bytes.push(ch as u8);
        }
    }

    fn push_compact_latin1_def(bytes: &mut Vec<u8>, value: &str, paired: bool) -> u32 {
        bytes.push(if paired {
            RtonTag::CompactLatin1StringDefinitionWithValueOffset as u8
        } else {
            RtonTag::CompactLatin1StringDefinition as u8
        });
        push_u32(bytes, value.chars().count() as u32 + 1);
        let data_offset = bytes.len() as u32;
        for ch in value.chars() {
            assert!((ch as u32) <= 0xff);
            bytes.push(ch as u8);
        }
        bytes.push(0);
        if paired {
            push_u32(bytes, 0);
        }
        data_offset
    }

    fn push_compact_latin1_ref(bytes: &mut Vec<u8>, offset: u32, paired: bool) {
        bytes.push(if paired {
            RtonTag::CompactLatin1StringReferenceWithValueOffset as u8
        } else {
            RtonTag::CompactLatin1StringReference as u8
        });
        push_u32(bytes, offset);
        if paired {
            push_u32(bytes, 0);
        }
    }

    fn push_compact_utf32_def(bytes: &mut Vec<u8>, value: &str, paired: bool) -> u32 {
        bytes.push(if paired {
            RtonTag::CompactUtf32StringDefinitionWithValueOffset as u8
        } else {
            RtonTag::CompactUtf32StringDefinition as u8
        });
        push_u32(bytes, (value.chars().count() as u32 + 1) * 4);
        let data_offset = bytes.len() as u32;
        for ch in value.chars() {
            push_u32(bytes, ch as u32);
        }
        push_u32(bytes, 0);
        if paired {
            push_u32(bytes, 0);
        }
        data_offset
    }

    fn push_compact_utf32_ref(bytes: &mut Vec<u8>, offset: u32, paired: bool) {
        bytes.push(if paired {
            RtonTag::CompactUtf32StringReferenceWithValueOffset as u8
        } else {
            RtonTag::CompactUtf32StringReference as u8
        });
        push_u32(bytes, offset);
        if paired {
            push_u32(bytes, 0);
        }
    }

    #[test]
    fn test_rton_round_trip() {
        // Create a sample Value
        let original = Value::Object(vec![
            ("key1".to_string(), Value::String("value1".to_string())),
            ("key2".to_string(), Value::Int32(123)),
            (
                "key3".to_string(),
                Value::Array(vec![Value::Bool(true), Value::Bool(false)]),
            ),
        ]);

        // Serialize to bytes (using default key/writer logic)
        let mut buffer = Vec::new();
        to_writer(&mut buffer, &original).expect("Serialization failed");

        // Deserialize from bytes
        let mut cursor = Cursor::new(buffer);
        let decoded: Value = from_reader(&mut cursor).expect("Deserialization failed");

        // Verify equality
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_rtid_uid_round_trip() {
        let original = Value::Object(vec![(
            "a".to_string(),
            Value::Rtid(Rtid::Uid {
                group: 0x2,
                id: 0x1,
                obj: 0x00000003,
                name: Some("sheet".to_string()),
            }),
        )]);

        let decoded = round_trip(&original).expect("RTID UID round-trip failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_rtid_raw_round_trip() {
        let original = Value::Object(vec![(
            "a".to_string(),
            Value::Rtid(Rtid::Raw {
                name: "name".to_string(),
                parent: "parent".to_string(),
            }),
        )]);

        let decoded = round_trip(&original).expect("RTID raw round-trip failed");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_rtid_colon_format_parsing() {
        // Hopper: PvZ2 RtIdProtocol emits colon-prefixed RTID strings
        // in runtime protocol messages.  Verify we can parse all variants.

        // :%d.%d — decimal id.group only (obj=0, no name)
        let r = Rtid::from_str("RTID(:123.456)").expect("colon short parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 123,
                group: 456,
                obj: 0,
                name: None,
            }
        );

        // :%d.%d@%d — decimal id.group@obj
        let r = Rtid::from_str("RTID(:789.012@345)").expect("colon no-name parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 789,
                group: 12,
                obj: 345,
                name: None,
            }
        );

        // Explicit colon-prefixed full form; the no-colon runtime formatter is
        // ambiguous with standard hex strings when id/group are digits only.
        let r = Rtid::from_str("RTID(:1.2.0000000a@SomeName)").expect("colon full parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 1,
                group: 2,
                obj: 0xa,
                name: Some("SomeName".to_string()),
            }
        );

        // Existing hex format still works
        let r = Rtid::from_str("RTID(1a.2b.0000000c@name)").expect("hex parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 0x1a,
                group: 0x2b,
                obj: 0xc,
                name: Some("name".to_string()),
            }
        );
    }

    #[test]
    fn test_rtid_runtime_format_parsing() {
        let r =
            Rtid::from_runtime_str("RTID(10.20.0000000a@SomeName)").expect("runtime full parse");
        assert_eq!(
            r,
            Rtid::Uid {
                id: 10,
                group: 20,
                obj: 0xa,
                name: Some("SomeName".to_string()),
            }
        );
        assert_eq!(r.to_runtime_string(), "RTID(10.20.0000000a@SomeName)");

        let standard = Rtid::from_str("RTID(10.20.0000000a@SomeName)").expect("standard hex parse");
        assert_eq!(
            standard,
            Rtid::Uid {
                id: 0x10,
                group: 0x20,
                obj: 0xa,
                name: Some("SomeName".to_string()),
            }
        );
    }

    #[test]
    fn test_missing_done_footer_is_rejected() {
        let original = Value::Object(vec![("a".to_string(), Value::Int32(1))]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");
        bytes.truncate(bytes.len() - 4);

        let error = from_bytes::<Value>(&bytes).expect_err("Expected footer error");
        assert!(matches!(error, Error::InvalidFooter));
    }

    #[test]
    fn test_trailing_data_after_done_footer_is_ignored() {
        let original = Value::Object(vec![("a".to_string(), Value::Int32(1))]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");
        bytes.extend_from_slice(&[1, 2, 3, 4]);

        let decoded = from_bytes::<Value>(&bytes).expect("trailing data should be ignored");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_header_accepts_compact_version_with_compact_root() {
        let mut bytes = compact_file_prefix_with_version(0x0001_0001);
        bytes.push(RtonTag::CompactObjectBegin as u8);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact version should decode");
        assert_eq!(decoded, Value::Object(Vec::new()));
    }

    #[test]
    fn test_compact_version_requires_compact_root() {
        let mut bytes = compact_file_prefix_with_version(0x0001_0001);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let error = from_bytes::<Value>(&bytes).expect_err("compact version should reject");
        assert!(matches!(error, Error::Message(_)));
    }

    #[test]
    fn test_header_rejects_version_high_word_above_one() {
        let mut bytes = compact_file_prefix_with_version(0x0002_0001);
        bytes.push(RtonTag::CompactObjectBegin as u8);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let error = from_bytes::<Value>(&bytes).expect_err("version should reject");
        assert!(matches!(error, Error::Message(_)));
    }

    #[test]
    fn test_standard_array_allows_end_before_declared_length() {
        let original = Value::Object(vec![(
            "a".to_string(),
            Value::Array(vec![Value::UInt8(1), Value::UInt8(2)]),
        )]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");

        let capacity_idx = bytes
            .iter()
            .position(|&byte| byte == RtonTag::ArrayCapacity as u8)
            .expect("Array capacity tag missing")
            + 1;
        bytes[capacity_idx] = 3;

        let decoded = from_bytes::<Value>(&bytes).expect("array should end at 0xfe");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_standard_array_rejects_more_than_declared_length() {
        let original = Value::Object(vec![(
            "a".to_string(),
            Value::Array(vec![Value::UInt8(1), Value::UInt8(2)]),
        )]);
        let mut bytes = to_bytes(&original).expect("Serialization failed");

        let capacity_idx = bytes
            .iter()
            .position(|&byte| byte == RtonTag::ArrayCapacity as u8)
            .expect("Array capacity tag missing")
            + 1;
        bytes[capacity_idx] = 1;

        let error = from_bytes::<Value>(&bytes).expect_err("Expected array mismatch");
        assert!(matches!(error, Error::ArrayCapacityExceeded));
    }

    #[test]
    fn test_explicit_u32_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU32(300)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::UnsignedVarInt32 as u8);
    }

    #[test]
    fn test_explicit_u64_varint_uses_alt_tag() {
        let bytes = to_bytes(&VarIntU64(300)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::UnsignedVarInt64 as u8);
    }

    #[test]
    fn test_removed_signed_alt_varint_tags_are_rejected() {
        for tag in [0x29, 0x49] {
            let mut bytes = compact_file_prefix_with_version(STANDARD_FILE_VERSION);
            push_standard_latin1_def(&mut bytes, "old");
            bytes.push(tag);
            bytes.push(0);
            bytes.push(RtonTag::ObjectEnd as u8);
            finish_compact_file(&mut bytes);

            let error = from_bytes::<Value>(&bytes).expect_err("removed tag should be rejected");
            assert!(matches!(error, Error::UnknownTag(t) if t == tag));
        }
    }

    #[test]
    fn test_small_positive_i32_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i32).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::RawVarInt32 as u8);
    }

    #[test]
    fn test_negative_i32_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i32).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::ZigZagVarInt32 as u8);
    }

    #[test]
    fn test_large_i32_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i32 << 21)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::I32 as u8);
    }

    #[test]
    fn test_small_positive_i64_uses_compact_signed_positive_tag() {
        let bytes = to_bytes(&123i64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::RawVarInt64 as u8);
    }

    #[test]
    fn test_negative_i64_uses_zigzag_tag() {
        let bytes = to_bytes(&-1i64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::ZigZagVarInt64 as u8);
    }

    #[test]
    fn test_large_i64_uses_fixed_width_tag() {
        let bytes = to_bytes(&(1i64 << 49)).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::I64 as u8);
    }

    #[test]
    fn test_f64_uses_typed_double_tag_even_when_f32_exact() {
        let bytes = to_bytes(&1.5f64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::F64 as u8);
    }

    #[test]
    fn test_f64_zero_uses_double_zero_tag() {
        let bytes = to_bytes(&0.0f64).expect("Serialization failed");

        assert_eq!(payload_tag(&bytes), RtonTag::F64Zero as u8);
    }

    #[test]
    fn test_signed_fields_round_trip_with_compact_tags() {
        let original = SignedFields {
            small_i32: 123,
            negative_i32: -1,
            small_i64: 123,
            negative_i64: -1,
        };

        let decoded =
            from_bytes::<SignedFields>(&to_bytes(&original).expect("Serialization failed"))
                .expect("Deserialization failed");

        assert_eq!(decoded, original);
    }

    // ——— Adaptive unsigned integer encoding ———

    #[test]
    fn test_small_u32_uses_compact_alt_tag() {
        // 300 fits in 2 varint bytes (< 4) → should use 0x28 (UnsignedVarInt32)
        let bytes = to_bytes(&300u32).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonTag::UnsignedVarInt32 as u8);
    }

    #[test]
    fn test_large_u32_uses_fixed_width_tag() {
        // 1 << 28 needs 5 varint bytes (>= 4) → should use 0x26 (U32)
        let bytes = to_bytes(&(1u32 << 28)).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonTag::U32 as u8);
    }

    #[test]
    fn test_small_u64_uses_compact_alt_tag() {
        // 300 fits in 2 varint bytes (< 8) → should use 0x48 (UnsignedVarInt64)
        let bytes = to_bytes(&300u64).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonTag::UnsignedVarInt64 as u8);
    }

    #[test]
    fn test_large_u64_uses_fixed_width_tag() {
        // 1 << 56 needs 9 varint bytes (>= 8) → should use 0x46 (U64)
        let bytes = to_bytes(&(1u64 << 56)).expect("Serialization failed");
        assert_eq!(payload_tag(&bytes), RtonTag::U64 as u8);
    }

    #[test]
    fn test_unsigned_fields_round_trip_with_compact_tags() {
        let original = UnsignedFields {
            small_u32: 300,
            large_u32: 1 << 28,
            small_u64: 300,
            large_u64: 1u64 << 56,
        };

        let decoded =
            from_bytes::<UnsignedFields>(&to_bytes(&original).expect("Serialization failed"))
                .expect("Deserialization failed");

        assert_eq!(decoded, original);
    }

    // ——— DirectStr ———

    #[test]
    fn test_direct_str_latin1_uses_direct_tag() {
        let val = DirectStringFields {
            cached_name: "hello".to_string(),
            direct_tag: DirectStr("world".to_string()),
        };
        let bytes = to_bytes(&val).expect("Serialization failed");

        // The file structure is: RTON header (8B) → object start → key+value pairs
        // We just need to verify the bytes contain both 0x90 (cached) and 0x81 (direct)
        let body = &bytes[FILE_HEADER.len() + 4..];
        assert!(
            body.windows(1)
                .any(|w| w[0] == RtonTag::StringLatin1Definition as u8),
            "cached string should use 0x90 (StringLatin1Definition)"
        );
        assert!(
            body.windows(1)
                .any(|w| w[0] == RtonTag::StringLatin1Direct as u8),
            "direct string should use 0x81 (StringLatin1Direct)"
        );
    }

    #[test]
    fn test_standard_latin1_strings_encode_chars_as_single_bytes() {
        let latin1 = "\u{00e9}".to_string();
        let original = Value::Object(vec![("v".to_string(), Value::String(latin1))]);
        let bytes = to_bytes(&original).expect("Serialization failed");

        assert!(
            bytes
                .windows(3)
                .any(|w| w == [RtonTag::StringLatin1Definition as u8, 1, 0xe9,]),
            "Latin-1 string should use the 0x90 Latin-1 payload path"
        );

        let decoded = from_bytes::<Value>(&bytes).expect("Deserialization failed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_standard_utf8_string_uses_char_count_not_declared_byte_len() {
        let mut bytes = standard_file_prefix();
        bytes.push(RtonTag::ObjectBegin as u8);
        push_standard_latin1_def(&mut bytes, "v");
        bytes.push(RtonTag::StringUtf8Direct as u8);
        bytes.push(2);
        bytes.push(0);
        bytes.extend_from_slice("雪x".as_bytes());
        bytes.push(RtonTag::ObjectEnd as u8);
        bytes.extend_from_slice(FILE_FOOTER);

        let decoded = from_bytes::<Value>(&bytes).expect("byte length should be ignored");
        assert_eq!(
            decoded,
            Value::Object(vec![("v".to_string(), Value::String("雪x".to_string()))])
        );
    }

    #[test]
    fn test_rtid_utf8_name_uses_char_count_not_declared_byte_len() {
        let mut bytes = standard_file_prefix();
        bytes.push(RtonTag::ObjectBegin as u8);
        push_standard_latin1_def(&mut bytes, "v");
        bytes.push(RtonTag::Rtid as u8);
        bytes.push(RtidPayloadTag::UidWithName as u8);
        bytes.push(1);
        bytes.push(0);
        bytes.extend_from_slice("雪".as_bytes());
        bytes.push(2);
        bytes.push(1);
        push_u32(&mut bytes, 3);
        bytes.push(RtonTag::ObjectEnd as u8);
        bytes.extend_from_slice(FILE_FOOTER);

        let decoded = from_bytes::<Value>(&bytes).expect("RTID byte length should be ignored");
        assert_eq!(
            decoded,
            Value::Object(vec![(
                "v".to_string(),
                Value::Rtid(Rtid::Uid {
                    group: 2,
                    id: 1,
                    obj: 3,
                    name: Some("雪".to_string())
                })
            )])
        );
    }

    #[test]
    fn test_compact_latin1_strings_encode_chars_as_single_bytes() {
        let latin1 = "\u{00e9}".to_string();
        let original = Value::Object(vec![("v".to_string(), Value::String(latin1))]);
        let bytes = to_compact_bytes(&original).expect("Compact serialization failed");

        assert!(
            bytes.windows(7).any(|w| w
                == [
                    RtonTag::CompactLatin1StringDefinition as u8,
                    2,
                    0,
                    0,
                    0,
                    0xe9,
                    0,
                ]),
            "Latin-1 compact string should use the B0 Latin-1 payload path"
        );

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_binary_blob_marker_and_length_are_lenient() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "bin", false);
        bytes.push(RtonTag::BinaryBlob as u8);
        bytes.push(1);
        bytes.push(4);
        bytes.extend_from_slice(b"0A0B");
        bytes.push(2);
        bytes.extend_from_slice(&[0xaa, 0xbb]);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("marker should be ignored");
        assert_eq!(
            decoded,
            Value::Object(vec![(
                "bin".to_string(),
                Value::Binary(BinaryBlob(vec![0x0a, 0x0b]))
            )])
        );

        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "bin", false);
        bytes.push(RtonTag::BinaryBlob as u8);
        bytes.push(0);
        bytes.push(4);
        bytes.extend_from_slice(b"0A0B");
        bytes.push(3);
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("length mismatch should be ignored");
        assert_eq!(
            decoded,
            Value::Object(vec![(
                "bin".to_string(),
                Value::Binary(BinaryBlob(vec![0x0a, 0x0b]))
            )])
        );
    }

    #[test]
    fn test_compact_rtid_zero_deserializes_as_none() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "a", false);
        bytes.push(RtonTag::CompactRtid as u8);
        bytes.push(RtidPayloadTag::Null as u8);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<OptionalRtidField>(&bytes).expect("compact option decode");
        assert_eq!(decoded, OptionalRtidField { a: None });
    }

    #[test]
    fn test_compact_latin1_strings_reference_payload_offsets() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "first", false);
        let value_offset = push_compact_latin1_def(&mut bytes, "hello", false);
        push_compact_latin1_def(&mut bytes, "second", false);
        push_compact_latin1_ref(&mut bytes, value_offset, false);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![
                ("first".to_string(), Value::String("hello".to_string())),
                ("second".to_string(), Value::String("hello".to_string())),
            ])
        );
    }

    #[test]
    fn test_paired_compact_latin1_strings_consume_aux_offsets() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "first", false);
        let value_offset = push_compact_latin1_def(&mut bytes, "hello", true);
        push_compact_latin1_def(&mut bytes, "second", false);
        push_compact_latin1_ref(&mut bytes, value_offset, true);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![
                ("first".to_string(), Value::String("hello".to_string())),
                ("second".to_string(), Value::String("hello".to_string())),
            ])
        );
    }

    #[test]
    fn test_compact_wide_strings_are_utf32_payloads() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "first", false);
        let value_offset = push_compact_utf32_def(&mut bytes, "\u{82bd}", false);
        push_compact_latin1_def(&mut bytes, "second", false);
        push_compact_utf32_ref(&mut bytes, value_offset, false);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![
                ("first".to_string(), Value::String("\u{82bd}".to_string())),
                ("second".to_string(), Value::String("\u{82bd}".to_string())),
            ])
        );
    }

    #[test]
    fn test_compact_array_uses_offset_table_and_fixed_count() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "arr", false);
        bytes.push(RtonTag::CompactArrayBegin as u8);
        bytes.push(RtonTag::ArrayCapacity as u8);
        push_u32(&mut bytes, 2);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        bytes.push(RtonTag::CompactBoolean as u8);
        bytes.push(1);
        bytes.push(RtonTag::CompactBoolean as u8);
        bytes.push(0);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![(
                "arr".to_string(),
                Value::Array(vec![Value::Bool(true), Value::Bool(false)])
            )])
        );
    }

    #[test]
    fn test_compact_array_validates_nonzero_offsets() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "arr", false);
        bytes.push(RtonTag::CompactArrayBegin as u8);
        bytes.push(RtonTag::ArrayCapacity as u8);
        push_u32(&mut bytes, 2);

        let table_pos = bytes.len();
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 0);

        let first_offset = bytes.len() as u32;
        bytes.push(RtonTag::CompactBoolean as u8);
        bytes.push(1);
        let second_offset = bytes.len() as u32;
        bytes.push(RtonTag::CompactBoolean as u8);
        bytes.push(0);
        let end_offset = bytes.len() as u32;
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        patch_u32(&mut bytes, table_pos, first_offset);
        patch_u32(&mut bytes, table_pos + 4, second_offset);
        patch_u32(&mut bytes, table_pos + 8, end_offset);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");
        assert_eq!(
            decoded,
            Value::Object(vec![(
                "arr".to_string(),
                Value::Array(vec![Value::Bool(true), Value::Bool(false)])
            )])
        );

        patch_u32(&mut bytes, table_pos + 4, second_offset + 1);
        let error = from_bytes::<Value>(&bytes).expect_err("bad offset should fail");
        assert!(matches!(error, Error::Message(_)));
    }

    #[test]
    fn test_compact_binary_blob_uses_hex_string_and_raw_length() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "bin", false);
        bytes.push(RtonTag::CompactBinaryBlob as u8);
        push_compact_latin1_def(&mut bytes, "0A0B", false);
        push_u32(&mut bytes, 2);
        bytes.extend_from_slice(&[0xaa, 0xbb]);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![(
                "bin".to_string(),
                Value::Binary(BinaryBlob(vec![0x0a, 0x0b]))
            )])
        );
    }

    #[test]
    fn test_compact_binary_blob_declared_length_is_lenient() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "bin", false);
        bytes.push(RtonTag::CompactBinaryBlob as u8);
        push_compact_latin1_def(&mut bytes, "0A0B", false);
        push_u32(&mut bytes, 3);
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![(
                "bin".to_string(),
                Value::Binary(BinaryBlob(vec![0x0a, 0x0b]))
            )])
        );
    }

    #[test]
    fn test_compact_binary_blob_unknown_inner_tag_matches_binary_fallback() {
        let mut bytes = compact_file_prefix();
        bytes.push(RtonTag::CompactObjectBegin as u8);
        push_compact_latin1_def(&mut bytes, "bin", false);
        bytes.push(RtonTag::CompactBinaryBlob as u8);
        bytes.push(0xcc);
        push_u32(&mut bytes, 2);
        bytes.extend_from_slice(&[0xaa, 0xbb]);
        bytes.push(RtonTag::ObjectEnd as u8);
        finish_compact_file(&mut bytes);

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");

        assert_eq!(
            decoded,
            Value::Object(vec![(
                "bin".to_string(),
                Value::String("$BINARY(\"<unknown>\", 2)".to_string())
            )])
        );
    }

    #[test]
    fn test_compact_writer_emits_runtime_tags() {
        let original = Value::Object(vec![
            ("flag".to_string(), Value::Bool(true)),
            (
                "items".to_string(),
                Value::Array(vec![
                    Value::String("same".to_string()),
                    Value::String("same".to_string()),
                ]),
            ),
            (
                "bin".to_string(),
                Value::Binary(BinaryBlob(vec![0x0a, 0x0b])),
            ),
            (
                "rid".to_string(),
                Value::Rtid(Rtid::Raw {
                    name: "child".to_string(),
                    parent: "parent".to_string(),
                }),
            ),
        ]);

        let bytes = to_compact_bytes(&original).expect("compact serialization failed");
        assert_eq!(read_u32(&bytes, FILE_HEADER.len()), COMPACT_FILE_VERSION);
        let body = &bytes[FILE_HEADER.len() + 4..bytes.len() - FILE_FOOTER.len()];

        assert!(body.contains(&(RtonTag::CompactObjectBegin as u8)));
        assert!(body.contains(&(RtonTag::CompactArrayBegin as u8)));
        assert!(body.contains(&(RtonTag::CompactBoolean as u8)));
        assert!(body.contains(&(RtonTag::CompactBinaryBlob as u8)));
        assert!(body.contains(&(RtonTag::CompactRtid as u8)));
        assert!(body.contains(&(RtonTag::CompactLatin1StringReference as u8)));
        assert!(body.contains(&(RtonTag::CompactLatin1StringDefinitionWithValueOffset as u8)));

        let decoded = from_bytes::<Value>(&bytes).expect("compact decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_compact_writer_uses_paired_key_aux_offsets() {
        let original = Value::Object(vec![
            ("flag".to_string(), Value::Bool(true)),
            ("flag".to_string(), Value::Bool(false)),
        ]);

        let bytes = to_compact_bytes(&original).expect("compact serialization failed");
        let mut pos = FILE_HEADER.len() + 4;
        assert_eq!(bytes[pos], RtonTag::CompactObjectBegin as u8);
        pos += 1;

        assert_eq!(
            bytes[pos],
            RtonTag::CompactLatin1StringDefinitionWithValueOffset as u8
        );
        pos += 1;
        assert_eq!(read_u32(&bytes, pos), 5);
        pos += 4;
        let payload_offset = pos as u32;
        assert_eq!(&bytes[pos..pos + 5], b"flag\0");
        pos += 5;
        let first_aux = read_u32(&bytes, pos);
        pos += 4;
        assert_eq!(bytes[pos], RtonTag::CompactBoolean as u8);
        pos += 2;
        assert_eq!(first_aux, pos as u32);

        assert_eq!(
            bytes[pos],
            RtonTag::CompactLatin1StringReferenceWithValueOffset as u8
        );
        pos += 1;
        assert_eq!(read_u32(&bytes, pos), payload_offset);
        pos += 4;
        let second_aux = read_u32(&bytes, pos);
        pos += 4;
        assert_eq!(bytes[pos], RtonTag::CompactBoolean as u8);
        pos += 2;
        assert_eq!(second_aux, pos as u32);
    }
}
