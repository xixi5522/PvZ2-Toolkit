//! Serialization wrappers for choosing specific RTON encodings.
//!
//! These wrappers are useful when the Rust semantic type is not enough to pick
//! the desired binary RTON tag. They affect binary RTON output; through
//! human-readable formats such as JSON they serialize as their inner values.

use serde::{Deserialize, Serialize, Serializer};

// ============================================================================
// VarInt — force compact varint encoding for integers
// ============================================================================

/// Wrapper that forces VarInt encoding for integer fields.
///
/// Without this wrapper, the standard serializer chooses between fixed-width
/// tags and compact VarInt tags adaptively. `VarInt<T>` requests the VarInt
/// path explicitly for supported integer types.
///
/// ```
/// use serde::Serialize;
/// use serde_rton::VarInt;
///
/// #[derive(Serialize)]
/// struct Packet {
///     id: VarInt<u32>,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarInt<T>(pub T);

impl Serialize for VarInt<i32> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntI32", &self.0)
    }
}

impl Serialize for VarInt<u32> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntU32", &self.0)
    }
}

impl Serialize for VarInt<i64> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntI64", &self.0)
    }
}

impl Serialize for VarInt<u64> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("VarIntU64", &self.0)
    }
}

// ============================================================================
// DirectStr — force direct string encoding (0x81/0x82) without interning
//
// PvZ2 uses direct strings when `arg3 == 0` in the Latin-1/UTF-8 writer
// helpers (`sub_1024e76bc` / `sub_1024e77cc`).  Wrap a `&str` or `String`
// in `DirectStr` to skip the interning cache and emit tag 0x81 or 0x82.
// ============================================================================

/// Wrapper that forces direct string encoding (tags 0x81/0x82).
///
/// By default the RTON serializer interns all strings (tags 0x90–0x93).
/// Wrap with `DirectStr` to emit a direct string instead — matching PvZ2's
/// `arg3 == 0` code path.
///
/// # Example
///
/// ```
/// use serde::Serialize;
/// use serde_rton::DirectStr;
///
/// #[derive(Serialize)]
/// struct MyData {
///     cached_name: String,           // emitted as 0x90/0x92 (interned)
///     direct_tag: DirectStr<String>, // emitted as 0x81/0x82 (direct)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirectStr<T: AsRef<str>>(pub T);

impl<T: AsRef<str> + Serialize> Serialize for DirectStr<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("DirectStr", &self.0.as_ref())
    }
}

impl<'de, T: AsRef<str> + Deserialize<'de>> Deserialize<'de> for DirectStr<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(DirectStr)
    }
}
