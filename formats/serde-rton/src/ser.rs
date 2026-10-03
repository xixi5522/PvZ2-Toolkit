//! RTON serialization.
//!
//! [`to_bytes`] and [`to_writer`] write standard RTON files. [`to_compact_bytes`]
//! and [`to_compact_writer`] write PvZ2's compact runtime RTON form from a
//! semantic [`Value`] tree.

use byteorder::{LittleEndian, WriteBytesExt};
use integer_encoding::VarIntWriter;
use serde::{Serialize, ser};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::Write;

use crate::binary::BinaryBlob;
use crate::error::{Error, Result};
use crate::rtid::Rtid;
use crate::tags::{COMPACT_FILE_VERSION, FILE_FOOTER, FILE_HEADER, RtonTag, STANDARD_FILE_VERSION};
use crate::value::Value;

// === Helper Functions for String Writing ===

fn fits_latin1_string(s: &str) -> bool {
    s.chars().all(|ch| (ch as u32) <= 0xff)
}

fn write_latin1_string_payload<W: Write>(writer: &mut W, s: &str) -> Result<()> {
    writer.write_varint(s.chars().count() as u64)?;
    for ch in s.chars() {
        writer.write_u8(ch as u8)?;
    }
    Ok(())
}

fn write_utf8_string_payload<W: Write>(writer: &mut W, s: &str) -> Result<()> {
    writer.write_varint(s.chars().count() as u64)?;
    writer.write_varint(s.len() as u64)?;
    writer.write_all(s.as_bytes())?;
    Ok(())
}

fn varint_len_u32(mut value: u32) -> usize {
    let mut len = 1;
    while value > 0x7f {
        value >>= 7;
        len += 1;
    }
    len
}

fn varint_len_u64(mut value: u64) -> usize {
    let mut len = 1;
    while value > 0x7f {
        value >>= 7;
        len += 1;
    }
    len
}

fn zigzag_i32(value: i32) -> u32 {
    ((value as u32) << 1) ^ ((value >> 31) as u32)
}

fn zigzag_i64(value: i64) -> u64 {
    ((value as u64) << 1) ^ ((value >> 63) as u64)
}

// === Helper Functions for Header/Footer ===

fn write_header<W: Write>(writer: &mut W) -> Result<()> {
    writer.write_all(FILE_HEADER)?;
    writer.write_u32::<LittleEndian>(STANDARD_FILE_VERSION)?;
    Ok(())
}

fn write_footer<W: Write>(writer: &mut W) -> Result<()> {
    writer.write_all(FILE_FOOTER)?;
    Ok(())
}

// === Serializer Implementation ===

#[derive(PartialEq, Clone, Copy)]
enum PendingVarInt {
    None,
    I32,
    U32,
    I64,
    U64,
}

/// Serde serializer for standard RTON streams.
///
/// Most callers should use [`to_bytes`] or [`to_writer`]. Direct construction is
/// useful only when integrating with custom framing; it does not write the RTON
/// file header or footer by itself.
pub struct Serializer<W> {
    writer: W,
    standard_latin1_indices_by_string: HashMap<String, u32>,
    next_standard_latin1_string_index: u32,
    standard_utf8_indices_by_string: HashMap<String, u32>,
    next_standard_utf8_string_index: u32,
    is_root: bool,
    pending_varint: PendingVarInt,
    pending_rtid: bool,
    pending_direct_str: bool,
}

impl<W: Write> Serializer<W> {
    /// Creates a serializer over an already-positioned writer.
    ///
    /// Use [`to_writer`] when serializing a complete RTON file.
    pub fn new(writer: W) -> Self {
        Serializer {
            writer,
            standard_latin1_indices_by_string: HashMap::new(),
            next_standard_latin1_string_index: 0,
            standard_utf8_indices_by_string: HashMap::new(),
            next_standard_utf8_string_index: 0,
            is_root: true,
            pending_varint: PendingVarInt::None,
            pending_rtid: false,
            pending_direct_str: false,
        }
    }

    fn write_interned_string(&mut self, v: &str) -> Result<()> {
        if fits_latin1_string(v) {
            if let Some(&idx) = self.standard_latin1_indices_by_string.get(v) {
                self.writer.write_u8(RtonTag::StringLatin1Reference as u8)?;
                self.writer.write_varint(idx as u64)?;
            } else {
                self.writer
                    .write_u8(RtonTag::StringLatin1Definition as u8)?;
                write_latin1_string_payload(&mut self.writer, v)?;
                self.standard_latin1_indices_by_string
                    .insert(v.to_string(), self.next_standard_latin1_string_index);
                self.next_standard_latin1_string_index += 1;
            }
        } else if let Some(&idx) = self.standard_utf8_indices_by_string.get(v) {
            self.writer.write_u8(RtonTag::StringUtf8Reference as u8)?;
            self.writer.write_varint(idx as u64)?;
        } else {
            self.writer.write_u8(RtonTag::StringUtf8Definition as u8)?;
            write_utf8_string_payload(&mut self.writer, v)?;
            self.standard_utf8_indices_by_string
                .insert(v.to_string(), self.next_standard_utf8_string_index);
            self.next_standard_utf8_string_index += 1;
        }
        Ok(())
    }

    /// Write a string directly (tags 0x81/0x82) without interning.
    /// This matches PvZ2's direct-string path (`arg3 == 0` in
    /// `sub_1024e76bc` / `sub_1024e77cc`).
    fn write_direct_string(&mut self, v: &str) -> Result<()> {
        if fits_latin1_string(v) {
            self.writer.write_u8(RtonTag::StringLatin1Direct as u8)?;
            write_latin1_string_payload(&mut self.writer, v)?;
        } else {
            self.writer.write_u8(RtonTag::StringUtf8Direct as u8)?;
            write_utf8_string_payload(&mut self.writer, v)?;
        }
        Ok(())
    }
}

/// Serializes `value` to a complete standard RTON file in memory.
///
/// The returned bytes include the `RTON` header, version word, encoded payload,
/// and `DONE` footer.
pub fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    to_writer(&mut data, value)?;
    Ok(data)
}

/// Serializes `value` as a complete standard RTON file into `writer`.
///
/// Strings are interned by default using `StringLatin1Definition` /
/// `StringUtf8Definition` and their reference tags. Wrap strings in
/// [`crate::DirectStr`] to force direct string tags instead.
pub fn to_writer<W: Write, T: Serialize>(mut writer: W, value: &T) -> Result<()> {
    write_header(&mut writer)?;

    // Create serializer borrowing the writer
    {
        let mut serializer = Serializer::new(&mut writer);
        value.serialize(&mut serializer)?;
    }

    write_footer(&mut writer)?;
    Ok(())
}

/// Serializes a semantic [`Value`] into PvZ2's compact runtime RTON form.
///
/// Compact RTON uses version [`COMPACT_FILE_VERSION`] and compact tags such as
/// `0xB0`-`0xBC`. It is intended for runtime-compatible semantic output rather
/// than preserving every original standard RTON tag.
pub fn to_compact_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    to_compact_writer(&mut data, value)?;
    Ok(data)
}

/// Serializes a semantic [`Value`] as compact runtime RTON into `writer`.
pub fn to_compact_writer<W: Write>(mut writer: W, value: &Value) -> Result<()> {
    let mut compact = CompactRtonWriter::new();
    compact.write_header()?;
    compact.write_value(value)?;
    compact.write_footer()?;
    writer.write_all(&compact.bytes)?;
    Ok(())
}

struct CompactRtonWriter {
    bytes: Vec<u8>,
    compact_latin1_offsets_by_string: HashMap<String, u32>,
    compact_utf32_offsets_by_string: HashMap<String, u32>,
}

impl CompactRtonWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            compact_latin1_offsets_by_string: HashMap::new(),
            compact_utf32_offsets_by_string: HashMap::new(),
        }
    }

    fn write_header(&mut self) -> Result<()> {
        self.bytes.write_all(FILE_HEADER)?;
        self.bytes.write_u32::<LittleEndian>(COMPACT_FILE_VERSION)?;
        Ok(())
    }

    fn write_footer(&mut self) -> Result<()> {
        self.bytes.write_all(FILE_FOOTER)?;
        Ok(())
    }

    fn position_u32(&self) -> Result<u32> {
        u32::try_from(self.bytes.len())
            .map_err(|_| Error::Message("Compact RTON exceeds u32".into()))
    }

    fn patch_u32(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u8_tag(&mut self, tag: RtonTag) -> Result<()> {
        self.bytes.write_u8(tag as u8)?;
        Ok(())
    }

    fn write_aux_placeholder(&mut self) -> Result<usize> {
        let offset = self.bytes.len();
        self.bytes.write_u32::<LittleEndian>(0)?;
        Ok(offset)
    }

    fn write_compact_string(&mut self, value: &str, paired: bool) -> Result<Option<usize>> {
        if fits_latin1_string(value) {
            self.write_compact_latin1_string(value, paired)
        } else {
            self.write_compact_utf32_string(value, paired)
        }
    }

    fn write_compact_latin1_string(&mut self, value: &str, paired: bool) -> Result<Option<usize>> {
        if let Some(&offset) = self.compact_latin1_offsets_by_string.get(value) {
            self.write_u8_tag(if paired {
                RtonTag::CompactLatin1StringReferenceWithValueOffset
            } else {
                RtonTag::CompactLatin1StringReference
            })?;
            self.bytes.write_u32::<LittleEndian>(offset)?;
            return if paired {
                self.write_aux_placeholder().map(Some)
            } else {
                Ok(None)
            };
        }

        self.write_u8_tag(if paired {
            RtonTag::CompactLatin1StringDefinitionWithValueOffset
        } else {
            RtonTag::CompactLatin1StringDefinition
        })?;
        self.bytes
            .write_u32::<LittleEndian>(value.chars().count() as u32 + 1)?;
        let offset = self.position_u32()?;
        for ch in value.chars() {
            self.bytes.write_u8(ch as u8)?;
        }
        self.bytes.write_u8(0)?;
        self.compact_latin1_offsets_by_string
            .insert(value.to_string(), offset);
        if paired {
            self.write_aux_placeholder().map(Some)
        } else {
            Ok(None)
        }
    }

    fn write_compact_utf32_string(&mut self, value: &str, paired: bool) -> Result<Option<usize>> {
        if let Some(&offset) = self.compact_utf32_offsets_by_string.get(value) {
            self.write_u8_tag(if paired {
                RtonTag::CompactUtf32StringReferenceWithValueOffset
            } else {
                RtonTag::CompactUtf32StringReference
            })?;
            self.bytes.write_u32::<LittleEndian>(offset)?;
            return if paired {
                self.write_aux_placeholder().map(Some)
            } else {
                Ok(None)
            };
        }

        self.write_u8_tag(if paired {
            RtonTag::CompactUtf32StringDefinitionWithValueOffset
        } else {
            RtonTag::CompactUtf32StringDefinition
        })?;
        self.bytes
            .write_u32::<LittleEndian>((value.chars().count() as u32 + 1) * 4)?;
        let offset = self.position_u32()?;
        for ch in value.chars() {
            self.bytes.write_u32::<LittleEndian>(ch as u32)?;
        }
        self.bytes.write_u32::<LittleEndian>(0)?;
        self.compact_utf32_offsets_by_string
            .insert(value.to_string(), offset);
        if paired {
            self.write_aux_placeholder().map(Some)
        } else {
            Ok(None)
        }
    }

    fn write_utf8_string_payload(&mut self, value: &str) -> Result<()> {
        self.bytes.write_varint(value.chars().count() as u64)?;
        self.bytes.write_varint(value.len() as u64)?;
        self.bytes.write_all(value.as_bytes())?;
        Ok(())
    }

    fn write_rtid_payload(&mut self, value: &Rtid) -> Result<()> {
        match value {
            Rtid::Null => self.bytes.write_u8(0)?,
            Rtid::Uid {
                group,
                id,
                obj,
                name,
            } => {
                if let Some(name) = name {
                    self.bytes.write_u8(2)?;
                    self.write_utf8_string_payload(name)?;
                } else {
                    self.bytes.write_u8(1)?;
                }
                self.bytes.write_varint(*group)?;
                self.bytes.write_varint(*id)?;
                self.bytes.write_u32::<LittleEndian>(*obj)?;
            }
            Rtid::Raw { name, parent } => {
                self.bytes.write_u8(3)?;
                self.write_utf8_string_payload(name)?;
                self.write_utf8_string_payload(parent)?;
            }
        }
        Ok(())
    }

    fn write_binary_blob(&mut self, value: &BinaryBlob) -> Result<()> {
        self.write_u8_tag(RtonTag::CompactBinaryBlob)?;

        let mut hex_str = String::with_capacity(value.0.len() * 2);
        for b in &value.0 {
            write!(&mut hex_str, "{:02X}", b)?;
        }

        self.write_compact_latin1_string(&hex_str, false)?;
        self.bytes.write_u32::<LittleEndian>(value.0.len() as u32)?;
        self.bytes.write_all(&value.0)?;
        Ok(())
    }

    fn write_array(&mut self, values: &[Value]) -> Result<()> {
        self.write_u8_tag(RtonTag::CompactArrayBegin)?;
        self.write_u8_tag(RtonTag::ArrayCapacity)?;
        self.bytes.write_u32::<LittleEndian>(values.len() as u32)?;

        let table_offset = self.bytes.len();
        for _ in 0..=values.len() {
            self.bytes.write_u32::<LittleEndian>(0)?;
        }

        for (idx, value) in values.iter().enumerate() {
            let element_offset = self.position_u32()?;
            self.patch_u32(table_offset + idx * 4, element_offset);
            self.write_value(value)?;
        }

        let end_offset = self.position_u32()?;
        self.patch_u32(table_offset + values.len() * 4, end_offset);
        Ok(())
    }

    fn write_object(&mut self, entries: &[(String, Value)]) -> Result<()> {
        self.write_u8_tag(RtonTag::CompactObjectBegin)?;
        for (key, value) in entries {
            let aux_offset = self.write_compact_string(key, true)?;
            self.write_value(value)?;
            if let Some(aux_offset) = aux_offset {
                self.patch_u32(aux_offset, self.position_u32()?);
            }
        }
        self.write_u8_tag(RtonTag::ObjectEnd)?;
        Ok(())
    }

    fn write_value(&mut self, value: &Value) -> Result<()> {
        match value {
            Value::Null => {
                self.write_u8_tag(RtonTag::CompactRtid)?;
                self.bytes.write_u8(0)?;
            }
            Value::Bool(value) => {
                self.write_u8_tag(RtonTag::CompactBoolean)?;
                self.bytes.write_u8(u8::from(*value))?;
            }
            Value::Int8(value) => {
                self.write_u8_tag(RtonTag::I8)?;
                self.bytes.write_i8(*value)?;
            }
            Value::UInt8(value) => {
                self.write_u8_tag(RtonTag::U8)?;
                self.bytes.write_u8(*value)?;
            }
            Value::Int16(value) => {
                self.write_u8_tag(RtonTag::I16)?;
                self.bytes.write_i16::<LittleEndian>(*value)?;
            }
            Value::UInt16(value) => {
                self.write_u8_tag(RtonTag::U16)?;
                self.bytes.write_u16::<LittleEndian>(*value)?;
            }
            Value::Int32(value) | Value::VarIntI32(crate::varint::VarInt(value)) => {
                self.write_u8_tag(RtonTag::I32)?;
                self.bytes.write_i32::<LittleEndian>(*value)?;
            }
            Value::UInt32(value) | Value::VarIntU32(crate::varint::VarInt(value)) => {
                self.write_u8_tag(RtonTag::U32)?;
                self.bytes.write_u32::<LittleEndian>(*value)?;
            }
            Value::Int64(value) | Value::VarIntI64(crate::varint::VarInt(value)) => {
                self.write_u8_tag(RtonTag::I64)?;
                self.bytes.write_i64::<LittleEndian>(*value)?;
            }
            Value::UInt64(value) | Value::VarIntU64(crate::varint::VarInt(value)) => {
                self.write_u8_tag(RtonTag::U64)?;
                self.bytes.write_u64::<LittleEndian>(*value)?;
            }
            Value::Float(value) => {
                self.write_u8_tag(RtonTag::F32)?;
                self.bytes.write_f32::<LittleEndian>(*value)?;
            }
            Value::Double(value) => {
                self.write_u8_tag(RtonTag::F64)?;
                self.bytes.write_f64::<LittleEndian>(*value)?;
            }
            Value::String(value) => {
                self.write_compact_string(value, false)?;
            }
            Value::Binary(value) => self.write_binary_blob(value)?,
            Value::Rtid(value) => {
                self.write_u8_tag(RtonTag::CompactRtid)?;
                self.write_rtid_payload(value)?;
            }
            Value::Array(values) => self.write_array(values)?,
            Value::Object(entries) => self.write_object(entries)?,
        }
        Ok(())
    }
}

impl<W: Write> ser::Serializer for &mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = ser::Impossible<(), Error>;
    type SerializeTupleVariant = ser::Impossible<(), Error>;
    type SerializeStructVariant = ser::Impossible<(), Error>;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<()> {
        match name {
            "RTID" => {
                self.writer.write_u8(RtonTag::Rtid as u8)?;
                self.pending_rtid = true;
                let result = value.serialize(&mut *self);
                self.pending_rtid = false;
                result
            }
            "VarIntI32" => {
                self.pending_varint = PendingVarInt::I32;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntU32" => {
                self.pending_varint = PendingVarInt::U32;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntI64" => {
                self.pending_varint = PendingVarInt::I64;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "VarIntU64" => {
                self.pending_varint = PendingVarInt::U64;
                value.serialize(&mut *self)?;
                self.pending_varint = PendingVarInt::None;
                Ok(())
            }
            "DirectStr" => {
                self.pending_direct_str = true;
                let result = value.serialize(&mut *self);
                self.pending_direct_str = false;
                result
            }
            _ => value.serialize(self),
        }
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        if self.pending_varint == PendingVarInt::I32 {
            self.writer.write_u8(RtonTag::ZigZagVarInt32 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonTag::I32Zero as u8)?;
        } else {
            let raw = v as u32;
            let raw_len = varint_len_u32(raw);
            let zigzag_len = varint_len_u32(zigzag_i32(v));

            if raw_len >= 4 && zigzag_len >= 4 {
                self.writer.write_u8(RtonTag::I32 as u8)?;
                self.writer.write_i32::<LittleEndian>(v)?;
            } else if zigzag_len < raw_len {
                self.writer.write_u8(RtonTag::ZigZagVarInt32 as u8)?;
                self.writer.write_varint(v)?;
            } else {
                self.writer.write_u8(RtonTag::RawVarInt32 as u8)?;
                self.writer.write_varint(raw)?;
            }
        }
        Ok(())
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        if self.pending_rtid {
            self.writer.write_u32::<LittleEndian>(v)?;
            return Ok(());
        }
        if self.pending_varint == PendingVarInt::U32 {
            self.writer.write_u8(RtonTag::UnsignedVarInt32 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonTag::U32Zero as u8)?;
        } else {
            let raw_len = varint_len_u32(v);
            // Adaptive: use compact varint (0x28) when it saves space,
            // otherwise fixed-width (0x26).  This matches PvZ2's dedicated
            // unsigned writer (verified against ARM64 binary).
            if raw_len >= 4 {
                self.writer.write_u8(RtonTag::U32 as u8)?;
                self.writer.write_u32::<LittleEndian>(v)?;
            } else {
                self.writer.write_u8(RtonTag::UnsignedVarInt32 as u8)?;
                self.writer.write_varint(v)?;
            }
        }
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        if self.pending_varint == PendingVarInt::I64 {
            self.writer.write_u8(RtonTag::ZigZagVarInt64 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonTag::I64Zero as u8)?;
        } else {
            let raw = v as u64;
            let raw_len = varint_len_u64(raw);
            let zigzag_len = varint_len_u64(zigzag_i64(v));

            if raw_len >= 8 && zigzag_len >= 8 {
                self.writer.write_u8(RtonTag::I64 as u8)?;
                self.writer.write_i64::<LittleEndian>(v)?;
            } else if zigzag_len < raw_len {
                self.writer.write_u8(RtonTag::ZigZagVarInt64 as u8)?;
                self.writer.write_varint(v)?;
            } else {
                self.writer.write_u8(RtonTag::RawVarInt64 as u8)?;
                self.writer.write_varint(raw)?;
            }
        }
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        if self.pending_rtid {
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if self.pending_varint == PendingVarInt::U64 {
            self.writer.write_u8(RtonTag::UnsignedVarInt64 as u8)?;
            self.writer.write_varint(v)?;
            return Ok(());
        }
        if v == 0 {
            self.writer.write_u8(RtonTag::U64Zero as u8)?;
        } else {
            let raw_len = varint_len_u64(v);
            // Adaptive: use compact varint (0x48) when it saves space,
            // otherwise fixed-width (0x46).  Matches PvZ2's dedicated
            // unsigned writer.
            if raw_len >= 8 {
                self.writer.write_u8(RtonTag::U64 as u8)?;
                self.writer.write_u64::<LittleEndian>(v)?;
            } else {
                self.writer.write_u8(RtonTag::UnsignedVarInt64 as u8)?;
                self.writer.write_varint(v)?;
            }
        }
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        if name == "RTID" && variant_index == 0x84 {
            self.writer.write_u8(RtonTag::RtidNull as u8)?;
            return Ok(());
        }
        self.writer.write_u8(variant_index as u8)?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        if self.pending_rtid {
            write_utf8_string_payload(&mut self.writer, v)?;
            return Ok(());
        }
        if v == "*" {
            self.writer.write_u8(RtonTag::StringAsterisk as u8)?;
            return Ok(());
        }
        if v == "RTID(0)" {
            self.writer.write_u8(RtonTag::RtidNull as u8)?;
            return Ok(());
        }
        if self.pending_direct_str {
            self.write_direct_string(v)
        } else {
            self.write_interned_string(v)
        }
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.writer.write_u8(RtonTag::BinaryBlob as u8)?;
        self.writer.write_u8(0)?;

        let mut hex_str = String::with_capacity(v.len() * 2);
        for b in v {
            write!(&mut hex_str, "{:02X}", b)?;
        }

        write_latin1_string_payload(&mut self.writer, &hex_str)?;
        self.writer.write_varint(v.len() as u64)?;
        self.writer.write_all(v)?;
        Ok(())
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let count = len.ok_or(Error::UnknownLength)?;
        self.writer.write_u8(RtonTag::ArrayBegin as u8)?;
        self.writer.write_u8(RtonTag::ArrayCapacity as u8)?;
        self.writer.write_varint(count as u64)?;
        Ok(self)
    }

    fn serialize_none(self) -> Result<()> {
        // PvZ2 maps JSON null → RtidNull (0x84), not StringAsterisk (0x02).
        // Hopper: sub_1024ee170 (JSON null) → sub_1024e78dc (RTID writer)
        // confirms that null RTID pointer → tag 0x84.
        self.writer.write_u8(RtonTag::RtidNull as u8)?;
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.writer.write_u8(if v {
            RtonTag::BooleanTrue as u8
        } else {
            RtonTag::BooleanFalse as u8
        })?;
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonTag::I8Zero as u8)?;
        } else {
            self.writer.write_u8(RtonTag::I8 as u8)?;
            self.writer.write_i8(v)?;
        }
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonTag::U8Zero as u8)?;
        } else {
            self.writer.write_u8(RtonTag::U8 as u8)?;
            self.writer.write_u8(v)?;
        }
        Ok(())
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonTag::I16Zero as u8)?;
        } else {
            self.writer.write_u8(RtonTag::I16 as u8)?;
            self.writer.write_i16::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        if v == 0 {
            self.writer.write_u8(RtonTag::U16Zero as u8)?;
        } else {
            self.writer.write_u8(RtonTag::U16 as u8)?;
            self.writer.write_u16::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<()> {
        if v == 0.0 {
            self.writer.write_u8(RtonTag::F32Zero as u8)?;
        } else {
            self.writer.write_u8(RtonTag::F32 as u8)?;
            self.writer.write_f32::<LittleEndian>(v)?;
        }
        Ok(())
    }
    fn serialize_f64(self, v: f64) -> Result<()> {
        if v == 0.0 {
            self.writer.write_u8(RtonTag::F64Zero as u8)?;
            return Ok(());
        }

        self.writer.write_u8(RtonTag::F64 as u8)?;
        self.writer.write_f64::<LittleEndian>(v)?;
        Ok(())
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        if self.is_root {
            self.is_root = false;
        } else {
            self.writer.write_u8(RtonTag::ObjectBegin as u8)?;
        }
        Ok(self)
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        if self.is_root {
            self.is_root = false;
        } else {
            self.writer.write_u8(RtonTag::ObjectBegin as u8)?;
        }
        Ok(self)
    }
    fn serialize_char(self, _v: char) -> Result<()> {
        Err(Error::Message("char not supported".into()))
    }
    fn serialize_unit(self) -> Result<()> {
        self.serialize_none()
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_none()
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(Error::Message("enum variants not supported".into()))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::Message("tuple structs not supported".into()))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::Message("tuple variants not supported".into()))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::Message("struct variants not supported".into()))
    }
}

impl<W: Write> ser::SerializeSeq for &mut Serializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonTag::ArrayEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeMap for &mut Serializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(&mut **self)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonTag::ObjectEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeStruct for &mut Serializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        key.serialize(&mut **self)?;
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        self.writer.write_u8(RtonTag::ObjectEnd as u8)?;
        Ok(())
    }
}
impl<W: Write> ser::SerializeTuple for &mut Serializer<W> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<()> {
        Ok(())
    }
}
