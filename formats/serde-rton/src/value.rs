//! Dynamic RTON value tree.
//!
//! [`Value`] is useful when the RTON schema is unknown or when object order and
//! duplicate keys need to be preserved. It can be serialized through
//! `serde_rton` for binary output and through human-readable Serde formats such
//! as `serde_json` for debugging or text editing.
//!
//! Human-readable formats are semantic, not tag-preserving: integer widths,
//! VarInt tags, and binary blob tags may not round-trip exactly through JSON.

use crate::binary::BinaryBlob;
use crate::rtid::Rtid;
use crate::varint::VarInt;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, SerializeMap, SerializeSeq};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Schema-less RTON data model.
///
/// Objects are stored as ordered key-value pairs rather than a map. This matches
/// PvZ2's order-sensitive object streams and allows duplicate keys to survive
/// `serde_json` serialization.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Semantic null value.
    ///
    /// In binary RTON this is written as the RTID-null form. In `serde_json`,
    /// this serializes as `null`, but JSON `null` is not accepted back into
    /// `Value` by the current deserializer.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Signed 8-bit integer.
    Int8(i8),
    /// Unsigned 8-bit integer.
    UInt8(u8),
    /// Signed 16-bit integer.
    Int16(i16),
    /// Unsigned 16-bit integer.
    UInt16(u16),
    /// Signed 32-bit integer.
    Int32(i32),
    /// Unsigned 32-bit integer.
    UInt32(u32),
    /// Signed 64-bit integer.
    Int64(i64),
    /// Unsigned 64-bit integer.
    UInt64(u64),
    /// Signed 32-bit value that should use the signed VarInt tag when written
    /// to binary RTON.
    VarIntI32(VarInt<i32>),
    /// Unsigned 32-bit value that should use the unsigned VarInt tag when
    /// written to binary RTON.
    VarIntU32(VarInt<u32>),
    /// Signed 64-bit value that should use the signed VarInt tag when written
    /// to binary RTON.
    VarIntI64(VarInt<i64>),
    /// Unsigned 64-bit value that should use the unsigned VarInt tag when
    /// written to binary RTON.
    VarIntU64(VarInt<u64>),
    /// 32-bit floating point value.
    Float(f32),
    /// 64-bit floating point value.
    Double(f64),
    /// String value.
    String(String),
    /// Binary blob value.
    Binary(BinaryBlob),
    /// Resource type identifier.
    Rtid(Rtid),
    /// Ordered array.
    Array(Vec<Value>),
    /// Ordered object entries.
    ///
    /// Duplicate keys are preserved.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Creates the narrowest signed integer variant that can hold `v`.
    ///
    /// This is used when deserializing generic signed integer inputs such as
    /// JSON numbers.
    pub fn new_int(v: i64) -> Self {
        if (i8::MIN as i64..=i8::MAX as i64).contains(&v) {
            Value::Int8(v as i8)
        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&v) {
            Value::Int16(v as i16)
        } else if (i32::MIN as i64..=i32::MAX as i64).contains(&v) {
            Value::Int32(v as i32)
        } else {
            Value::Int64(v)
        }
    }

    /// Creates the narrowest unsigned integer variant that can hold `v`.
    ///
    /// This is used when deserializing generic unsigned integer inputs such as
    /// JSON numbers.
    pub fn new_uint(v: u64) -> Self {
        if v <= u8::MAX as u64 {
            Value::UInt8(v as u8)
        } else if v <= u16::MAX as u64 {
            Value::UInt16(v as u16)
        } else if v <= u32::MAX as u64 {
            Value::UInt32(v as u32)
        } else {
            Value::UInt64(v)
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match self {
            Value::Null => serializer.serialize_none(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int8(v) => serializer.serialize_i8(*v),
            Value::UInt8(v) => serializer.serialize_u8(*v),
            Value::Int16(v) => serializer.serialize_i16(*v),
            Value::UInt16(v) => serializer.serialize_u16(*v),
            Value::Int32(v) => serializer.serialize_i32(*v),
            Value::UInt32(v) => serializer.serialize_u32(*v),
            Value::Int64(v) => serializer.serialize_i64(*v),
            Value::UInt64(v) => serializer.serialize_u64(*v),
            Value::VarIntI32(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_i32(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            Value::VarIntU32(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_u32(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            Value::VarIntI64(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_i64(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            Value::VarIntU64(v) => {
                if serializer.is_human_readable() {
                    serializer.serialize_u64(v.0)
                } else {
                    v.serialize(serializer)
                }
            }
            Value::Float(f) => serializer.serialize_f32(*f),
            Value::Double(d) => serializer.serialize_f64(*d),
            Value::String(s) => serializer.serialize_str(s),
            Value::Binary(b) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&b.to_string())
                } else {
                    b.serialize(serializer)
                }
            }
            Value::Rtid(rtid) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&rtid.to_string())
                } else {
                    rtid.serialize(serializer)
                }
            }
            Value::Array(vec) => {
                let mut seq = serializer.serialize_seq(Some(vec.len()))?;
                for element in vec {
                    seq.serialize_element(element)?;
                }
                seq.end()
            }
            Value::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = crate::value::Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any valid RTON value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(Value::Bool(value))
            }

            fn visit_i8<E>(self, value: i8) -> Result<Self::Value, E> {
                Ok(Value::Int8(value))
            }

            fn visit_u8<E>(self, value: u8) -> Result<Self::Value, E> {
                Ok(Value::UInt8(value))
            }

            fn visit_i16<E>(self, value: i16) -> Result<Self::Value, E> {
                Ok(Value::Int16(value))
            }

            fn visit_u16<E>(self, value: u16) -> Result<Self::Value, E> {
                Ok(Value::UInt16(value))
            }

            fn visit_i32<E>(self, value: i32) -> Result<Self::Value, E> {
                Ok(Value::Int32(value))
            }

            fn visit_u32<E>(self, value: u32) -> Result<Self::Value, E> {
                Ok(Value::UInt32(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Value::new_int(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Value::new_uint(value))
            }

            fn visit_f32<E>(self, value: f32) -> Result<Self::Value, E> {
                Ok(Value::Float(value))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Value::Double(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if value.starts_with("RTID(")
                    && let Ok(rtid) = Rtid::from_str(value)
                {
                    return Ok(Value::Rtid(rtid));
                }
                Ok(Value::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Binary(BinaryBlob(v.to_vec())))
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Value::Binary(BinaryBlob(v)))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Rtid(Rtid::Null))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Rtid(Rtid::Null))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some(elem) = visitor.next_element()? {
                    vec.push(elem);
                }
                Ok(Value::Array(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some((key, value)) = visitor.next_entry()? {
                    entries.push((key, value));
                }
                Ok(Value::Object(entries))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}
