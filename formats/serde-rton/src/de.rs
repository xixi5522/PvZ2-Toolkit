//! RTON deserialization.
//!
//! The primary entry points are [`from_bytes`] and [`from_reader`]. They accept
//! standard RTON files and the PvZ2 compact runtime variant when the file header
//! uses the compact version marker.

use byteorder::{LittleEndian, ReadBytesExt};
use integer_encoding::VarIntReader;
use serde::de::{self, DeserializeOwned};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::error::{Error, Result};
use crate::tags::{FILE_FOOTER, FILE_HEADER, RtidPayloadTag, RtonTag, STANDARD_FILE_VERSION};

/// Serde deserializer for RTON streams.
///
/// Most callers should use [`from_bytes`] or [`from_reader`] instead of
/// constructing this type directly. Direct construction expects the caller to
/// handle the file header and footer themselves.
pub struct Deserializer<'de, R> {
    reader: R,
    standard_latin1_strings_by_index: Vec<String>,
    standard_utf8_strings_by_index: Vec<String>,
    compact_latin1_strings_by_offset: HashMap<u32, String>,
    compact_utf32_strings_by_offset: HashMap<u32, String>,
    is_root: bool,
    phantom: std::marker::PhantomData<&'de ()>,
}

impl<'de, R: Read> Deserializer<'de, R> {
    /// Creates a deserializer over an already-positioned reader.
    ///
    /// This does not consume or validate the `RTON` file header. Use
    /// [`from_reader`] for whole-file deserialization.
    pub fn new(reader: R) -> Self {
        Deserializer {
            reader,
            standard_latin1_strings_by_index: Vec::new(),
            standard_utf8_strings_by_index: Vec::new(),
            compact_latin1_strings_by_offset: HashMap::new(),
            compact_utf32_strings_by_offset: HashMap::new(),
            is_root: true,
            phantom: std::marker::PhantomData,
        }
    }
}

macro_rules! read_primitive {
    ($reader:expr, $read_fn:ident) => {
        $reader.$read_fn::<LittleEndian>().map_err(Error::Io)?
    };
}

fn read_latin1_chars_by_count<R: Read>(reader: &mut R, count: u64) -> Result<String> {
    let mut buf = vec![0u8; count as usize];
    reader.read_exact(&mut buf)?;
    Ok(buf.into_iter().map(char::from).collect())
}

fn read_latin1_string_payload<R: Read>(reader: &mut R) -> Result<String> {
    let char_count: u64 = reader.read_varint()?;
    read_latin1_chars_by_count(reader, char_count)
}

fn read_utf8_chars_by_count<R: Read>(reader: &mut R, count: u64) -> Result<String> {
    let mut s = String::new();
    for _ in 0..count {
        let mut first_byte = [0u8; 1];
        reader.read_exact(&mut first_byte)?;
        let b = first_byte[0];

        let width = if b & 0x80 == 0 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else if b & 0xF8 == 0xF0 {
            4
        } else {
            return Err(Error::InvalidUtf8StartByte(b));
        };

        let mut char_buf = vec![0u8; width];
        char_buf[0] = b;
        if width > 1 {
            reader.read_exact(&mut char_buf[1..])?;
        }

        let ch = String::from_utf8(char_buf)?;
        s.push_str(&ch);
    }
    Ok(s)
}

fn read_utf8_string_payload<R: Read>(reader: &mut R) -> Result<String> {
    let char_count: u64 = reader.read_varint()?;
    let _byte_len: u64 = reader.read_varint()?;
    read_utf8_chars_by_count(reader, char_count)
}

fn decode_hex_bytes(hex_str: &str) -> Result<Vec<u8>> {
    if !hex_str.len().is_multiple_of(2) {
        return Err(Error::InvalidBinaryBlob(format!(
            "Odd-length hex string: {} characters",
            hex_str.len()
        )));
    }

    let mut bytes = Vec::with_capacity(hex_str.len() / 2);
    for i in (0..hex_str.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&hex_str[i..i + 2], 16)?);
    }

    Ok(bytes)
}

fn skip_blob_raw_payload<R: Seek>(reader: &mut R, len: u64) -> Result<()> {
    let offset = i64::try_from(len)
        .map_err(|_| Error::Message("BinaryBlob raw payload length exceeds i64".into()))?;
    reader.seek(SeekFrom::Current(offset)).map_err(Error::Io)?;
    Ok(())
}

/// Validate the standard RTON header: magic "RTON" and version.
/// Advances the reader past the header.
fn validate_header<R: Read + Seek>(reader: &mut R) -> Result<()> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;

    if header != *FILE_HEADER {
        return Err(Error::InvalidHeader);
    }

    let version_lo = reader.read_u16::<LittleEndian>()?;
    let version_hi = reader.read_u16::<LittleEndian>()?;
    let version = (u32::from(version_hi) << 16) | u32::from(version_lo);
    if u32::from(version_lo) != STANDARD_FILE_VERSION || version_hi > 1 {
        return Err(Error::Message(format!("Unsupported version: {}", version)));
    }

    if version_hi == 1 {
        let mut root_tag = [0u8; 1];
        reader.read_exact(&mut root_tag)?;
        if root_tag[0] != RtonTag::CompactObjectBegin as u8 {
            return Err(Error::Message(
                "Compact RTON version requires compact object root".into(),
            ));
        }
        reader.seek(SeekFrom::Current(-1)).map_err(Error::Io)?;
    }
    Ok(())
}

fn validate_footer<R: Read>(reader: &mut R) -> Result<()> {
    let mut footer = [0u8; 4];
    match reader.read_exact(&mut footer) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(Error::InvalidFooter);
        }
        Err(err) => return Err(Error::Io(err)),
    }

    if footer != FILE_FOOTER {
        return Err(Error::InvalidFooter);
    }

    Ok(())
}

/// Deserializes a complete RTON byte slice into `T`.
///
/// The input must include the file header and `DONE` footer. Trailing bytes
/// after the footer are ignored, matching the lenient PvZ2 reader behavior.
pub fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    from_reader(std::io::Cursor::new(bytes))
}

/// Deserializes a complete RTON stream into `T`.
///
/// The reader must implement both [`Read`] and [`Seek`] because some compact
/// payloads are offset-addressed and because header validation may rewind one
/// byte after probing the root tag.
pub fn from_reader<R: Read + Seek, T: DeserializeOwned>(mut reader: R) -> Result<T> {
    validate_header(&mut reader)?;

    let mut deserializer = Deserializer::new(reader);
    let value = T::deserialize(&mut deserializer)?;
    validate_footer(&mut deserializer.reader)?;
    Ok(value)
}

impl<'de, R: Read + Seek> Deserializer<'de, R> {
    fn read_rtid_string(&mut self) -> Result<String> {
        let sub_id_byte = self.reader.read_u8()?;
        let sub_id = RtidPayloadTag::try_from(sub_id_byte)
            .map_err(|_| Error::UnknownRtidSubId(sub_id_byte))?;

        match sub_id {
            RtidPayloadTag::Null => Ok("RTID(0)".to_string()),
            RtidPayloadTag::UidWithoutName => {
                let v2: u64 = self.reader.read_varint()?;
                let v1: u64 = self.reader.read_varint()?;
                let x = self.reader.read_u32::<LittleEndian>()?;
                Ok(format!("RTID({:x}.{:x}.{:08x}@)", v1, v2, x))
            }
            RtidPayloadTag::UidWithName => {
                let name = read_utf8_string_payload(&mut self.reader)?;

                let v2: u64 = self.reader.read_varint()?;
                let v1: u64 = self.reader.read_varint()?;
                let x = self.reader.read_u32::<LittleEndian>()?;
                Ok(format!("RTID({:x}.{:x}.{:08x}@{})", v1, v2, x, name))
            }
            RtidPayloadTag::RawString => {
                let s1 = read_utf8_string_payload(&mut self.reader)?;

                let s2 = read_utf8_string_payload(&mut self.reader)?;

                Ok(format!("RTID({}@{})", s1, s2))
            }
        }
    }

    fn read_compact_latin1_string_definition(&mut self, paired: bool) -> Result<String> {
        let len = self.reader.read_u32::<LittleEndian>()?;
        let data_offset = u32::try_from(self.reader.stream_position()?)
            .map_err(|_| Error::Message("Compact string offset exceeds u32".into()))?;
        let mut buf = vec![0u8; len as usize];
        self.reader.read_exact(&mut buf)?;

        if matches!(buf.last(), Some(0)) {
            buf.pop();
        }

        let s = buf.into_iter().map(char::from).collect::<String>();
        self.compact_latin1_strings_by_offset
            .insert(data_offset, s.clone());

        if paired {
            let _aux_offset = self.reader.read_u32::<LittleEndian>()?;
        }

        Ok(s)
    }

    fn read_compact_latin1_string_reference(&mut self, paired: bool) -> Result<String> {
        let offset = self.reader.read_u32::<LittleEndian>()?;
        let s = self
            .compact_latin1_strings_by_offset
            .get(&offset)
            .ok_or(Error::RefIndexOutOfBounds)?
            .clone();

        if paired {
            let _aux_offset = self.reader.read_u32::<LittleEndian>()?;
        }

        Ok(s)
    }

    fn read_compact_utf32_string_definition(&mut self, paired: bool) -> Result<String> {
        let byte_len = self.reader.read_u32::<LittleEndian>()?;
        if byte_len % 4 != 0 {
            return Err(Error::StringLengthMismatch {
                expected: byte_len as u64,
                actual: (byte_len / 4 * 4) as u64,
            });
        }

        let data_offset = u32::try_from(self.reader.stream_position()?)
            .map_err(|_| Error::Message("Compact string offset exceeds u32".into()))?;
        let codepoint_count = byte_len / 4;
        let mut s = String::new();
        let mut terminated = false;
        for _ in 0..codepoint_count {
            let codepoint = self.reader.read_u32::<LittleEndian>()?;
            if codepoint == 0 {
                terminated = true;
                continue;
            }
            if terminated {
                continue;
            }
            let ch = char::from_u32(codepoint).ok_or_else(|| {
                Error::Message(format!("Invalid compact UTF-32 codepoint: {codepoint:#x}"))
            })?;
            s.push(ch);
        }

        self.compact_utf32_strings_by_offset
            .insert(data_offset, s.clone());

        if paired {
            let _aux_offset = self.reader.read_u32::<LittleEndian>()?;
        }

        Ok(s)
    }

    fn read_compact_utf32_string_reference(&mut self, paired: bool) -> Result<String> {
        let offset = self.reader.read_u32::<LittleEndian>()?;
        let s = self
            .compact_utf32_strings_by_offset
            .get(&offset)
            .ok_or(Error::RefIndexOutOfBounds)?
            .clone();

        if paired {
            let _aux_offset = self.reader.read_u32::<LittleEndian>()?;
        }

        Ok(s)
    }

    fn read_compact_binary_blob_hex_string(&mut self) -> Result<Option<String>> {
        let tag_byte = self.reader.read_u8().map_err(Error::Io)?;
        match tag_byte {
            tag if tag == RtonTag::CompactLatin1StringDefinition as u8 => {
                self.read_compact_latin1_string_definition(false).map(Some)
            }
            tag if tag == RtonTag::CompactLatin1StringReference as u8 => {
                self.read_compact_latin1_string_reference(false).map(Some)
            }
            _ => Ok(None),
        }
    }
}

// Macro to generate simple forwarding deserialize methods
macro_rules! forward_to_deserialize_any {
    ($($method:ident),* $(,)?) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value>
            where
                V: de::Visitor<'de>,
            {
                self.deserialize_any(visitor)
            }
        )*
    };
}

impl<'de, R: Read + Seek> de::Deserializer<'de> for &mut Deserializer<'de, R> {
    type Error = Error;
    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        if self.is_root {
            self.is_root = false;
            let pos = self.reader.stream_position().map_err(Error::Io)?;
            let mut tag = [0u8; 1];
            self.reader.read_exact(&mut tag)?;
            if tag[0] != RtonTag::ObjectBegin as u8 && tag[0] != RtonTag::CompactObjectBegin as u8 {
                self.reader.seek(SeekFrom::Start(pos)).map_err(Error::Io)?;
            }
            return visitor.visit_map(RtonMapAccess::new(self));
        }
        let tag_byte = self.reader.read_u8().map_err(Error::Io)?;
        let tag = RtonTag::try_from(tag_byte).map_err(|_| Error::UnknownTag(tag_byte))?;

        match tag {
            RtonTag::BooleanFalse => visitor.visit_bool(false),
            RtonTag::BooleanTrue => visitor.visit_bool(true),
            RtonTag::StringAsterisk => visitor.visit_str("*"),

            RtonTag::I8Zero => visitor.visit_i8(0),
            RtonTag::U8Zero => visitor.visit_u8(0),
            RtonTag::I16Zero => visitor.visit_i16(0),
            RtonTag::U16Zero => visitor.visit_u16(0),
            RtonTag::I32Zero => visitor.visit_i32(0),
            RtonTag::U32Zero => visitor.visit_u32(0),
            RtonTag::I64Zero => visitor.visit_i64(0),
            RtonTag::U64Zero => visitor.visit_u64(0),

            RtonTag::I8 => visitor.visit_i8(self.reader.read_i8()?),
            RtonTag::U8 => visitor.visit_u8(self.reader.read_u8()?),
            RtonTag::I16 => visitor.visit_i16(read_primitive!(self.reader, read_i16)),
            RtonTag::U16 => visitor.visit_u16(read_primitive!(self.reader, read_u16)),
            RtonTag::I32 => visitor.visit_i32(read_primitive!(self.reader, read_i32)),
            RtonTag::U32 => visitor.visit_u32(read_primitive!(self.reader, read_u32)),
            RtonTag::I64 => visitor.visit_i64(read_primitive!(self.reader, read_i64)),
            RtonTag::U64 => visitor.visit_u64(read_primitive!(self.reader, read_u64)),

            RtonTag::RawVarInt32 => {
                let value = self.reader.read_varint::<u32>()?;
                if let Ok(value) = i32::try_from(value) {
                    visitor.visit_i32(value)
                } else {
                    visitor.visit_u32(value)
                }
            }
            RtonTag::UnsignedVarInt32 => visitor.visit_u32(self.reader.read_varint::<u32>()?),
            RtonTag::RawVarInt64 => {
                let value = self.reader.read_varint::<u64>()?;
                if let Ok(value) = i64::try_from(value) {
                    visitor.visit_i64(value)
                } else {
                    visitor.visit_u64(value)
                }
            }
            RtonTag::UnsignedVarInt64 => visitor.visit_u64(self.reader.read_varint::<u64>()?),
            RtonTag::ZigZagVarInt32 => visitor.visit_i32(self.reader.read_varint::<i32>()?),
            RtonTag::ZigZagVarInt64 => visitor.visit_i64(self.reader.read_varint::<i64>()?),

            RtonTag::F32 => visitor.visit_f32(read_primitive!(self.reader, read_f32)),
            RtonTag::F32Zero => visitor.visit_f32(0.0),
            RtonTag::F64 => visitor.visit_f64(read_primitive!(self.reader, read_f64)),
            RtonTag::F64Zero => visitor.visit_f64(0.0),

            RtonTag::StringLatin1Direct => {
                visitor.visit_string(read_latin1_string_payload(&mut self.reader)?)
            }
            RtonTag::StringLatin1Definition => {
                let s = read_latin1_string_payload(&mut self.reader)?;
                self.standard_latin1_strings_by_index.push(s.clone());
                visitor.visit_string(s)
            }
            RtonTag::StringLatin1Reference => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .standard_latin1_strings_by_index
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }

            RtonTag::StringUtf8Direct => {
                let s = read_utf8_string_payload(&mut self.reader)?;
                visitor.visit_string(s)
            }
            RtonTag::StringUtf8Definition => {
                let s = read_utf8_string_payload(&mut self.reader)?;
                self.standard_utf8_strings_by_index.push(s.clone());
                visitor.visit_string(s)
            }
            RtonTag::StringUtf8Reference => {
                let idx: u64 = self.reader.read_varint()?;
                let s = self
                    .standard_utf8_strings_by_index
                    .get(idx as usize)
                    .ok_or(Error::RefIndexOutOfBounds)?
                    .clone();
                visitor.visit_string(s)
            }

            RtonTag::BinaryBlob => {
                let _marker = self.reader.read_u8()?;
                let hex_str = read_latin1_string_payload(&mut self.reader)?;
                let declared_len = self.reader.read_varint::<u64>()?;
                skip_blob_raw_payload(&mut self.reader, declared_len)?;

                visitor.visit_byte_buf(decode_hex_bytes(&hex_str)?)
            }

            RtonTag::Rtid => {
                let rtid = self.read_rtid_string()?;
                visitor.visit_string(rtid)
            }
            RtonTag::RtidNull => visitor.visit_str("RTID(0)"),

            RtonTag::ArrayBegin => {
                if self.reader.read_u8()? != RtonTag::ArrayCapacity as u8 {
                    return Err(Error::ArrayCapacityTagMismatch);
                }
                let capacity: u64 = self.reader.read_varint()?;
                visitor.visit_seq(RtonSeqAccess::standard(self, capacity as usize))
            }
            RtonTag::ObjectBegin => visitor.visit_map(RtonMapAccess::new(self)),
            RtonTag::CompactObjectBegin => visitor.visit_map(RtonMapAccess::new(self)),

            RtonTag::CompactArrayBegin => {
                if self.reader.read_u8()? != RtonTag::ArrayCapacity as u8 {
                    return Err(Error::ArrayCapacityTagMismatch);
                }
                let capacity = self.reader.read_u32::<LittleEndian>()?;
                let offset_count = capacity
                    .checked_add(1)
                    .ok_or_else(|| Error::Message("Compact array capacity overflow".into()))?;
                let mut offsets = Vec::with_capacity(offset_count as usize);
                for _ in 0..offset_count {
                    offsets.push(self.reader.read_u32::<LittleEndian>()?);
                }
                visitor.visit_seq(RtonSeqAccess::compact(self, capacity as usize, offsets))
            }

            RtonTag::CompactRtid => {
                let rtid = self.read_rtid_string()?;
                visitor.visit_string(rtid)
            }

            RtonTag::CompactBinaryBlob => {
                let hex_str = self.read_compact_binary_blob_hex_string()?;
                let declared_len = self.reader.read_u32::<LittleEndian>()?;
                skip_blob_raw_payload(&mut self.reader, u64::from(declared_len))?;
                if let Some(hex_str) = hex_str {
                    visitor.visit_byte_buf(decode_hex_bytes(&hex_str)?)
                } else {
                    visitor.visit_string(format!("$BINARY(\"<unknown>\", {declared_len})"))
                }
            }

            RtonTag::CompactBoolean => {
                let b = self.reader.read_u8()?;
                visitor.visit_bool(b != 0)
            }

            RtonTag::CompactLatin1StringDefinition => {
                let s = self.read_compact_latin1_string_definition(false)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactLatin1StringReference => {
                let s = self.read_compact_latin1_string_reference(false)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactUtf32StringDefinition => {
                let s = self.read_compact_utf32_string_definition(false)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactUtf32StringReference => {
                let s = self.read_compact_utf32_string_reference(false)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactLatin1StringDefinitionWithValueOffset => {
                let s = self.read_compact_latin1_string_definition(true)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactLatin1StringReferenceWithValueOffset => {
                let s = self.read_compact_latin1_string_reference(true)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactUtf32StringDefinitionWithValueOffset => {
                let s = self.read_compact_utf32_string_definition(true)?;
                visitor.visit_string(s)
            }
            RtonTag::CompactUtf32StringReferenceWithValueOffset => {
                let s = self.read_compact_utf32_string_reference(true)?;
                visitor.visit_string(s)
            }
            _ => Err(Error::UnknownTag(tag_byte)),
        }
    }

    forward_to_deserialize_any! {
        deserialize_bool,
        deserialize_i8, deserialize_i16, deserialize_i32, deserialize_i64,
        deserialize_u8, deserialize_u16, deserialize_u32, deserialize_u64,
        deserialize_f32, deserialize_f64,
        deserialize_char, deserialize_str, deserialize_string,
        deserialize_bytes, deserialize_byte_buf,
        deserialize_unit, deserialize_seq, deserialize_map,
        deserialize_identifier, deserialize_ignored_any,
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        // PvZ2 represents JSON null as RtidNull (0x84).  The compact
        // transcode path rewrites that as CompactRtid + Null (0xBA 0x00).
        let pos = self.reader.stream_position().map_err(Error::Io)?;
        let tag_byte = self.reader.read_u8().map_err(Error::Io)?;
        if tag_byte == RtonTag::RtidNull as u8 {
            visitor.visit_none()
        } else if tag_byte == RtonTag::CompactRtid as u8 {
            let sub_id = self.reader.read_u8().map_err(Error::Io)?;
            if sub_id == RtidPayloadTag::Null as u8 {
                visitor.visit_none()
            } else {
                self.reader.seek(SeekFrom::Start(pos)).map_err(Error::Io)?;
                visitor.visit_some(self)
            }
        } else {
            self.reader.seek(SeekFrom::Start(pos)).map_err(Error::Io)?;
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::Message(
            "RTON does not support enum deserialization".into(),
        ))
    }
}

struct RtonSeqAccess<'a, 'de, R> {
    de: &'a mut Deserializer<'de, R>,
    remaining_capacity: usize,
    consumed: usize,
    expects_end_marker: bool,
    compact_offsets: Option<Vec<u32>>,
}
impl<'a, 'de, R: Read + Seek> RtonSeqAccess<'a, 'de, R> {
    fn standard(de: &'a mut Deserializer<'de, R>, capacity: usize) -> Self {
        Self {
            de,
            remaining_capacity: capacity,
            consumed: 0,
            expects_end_marker: true,
            compact_offsets: None,
        }
    }

    fn compact(de: &'a mut Deserializer<'de, R>, capacity: usize, offsets: Vec<u32>) -> Self {
        Self {
            de,
            remaining_capacity: capacity,
            consumed: 0,
            expects_end_marker: false,
            compact_offsets: Some(offsets),
        }
    }

    fn validate_compact_offset(&mut self) -> Result<()> {
        let Some(offsets) = &self.compact_offsets else {
            return Ok(());
        };

        let Some(&expected) = offsets.get(self.consumed) else {
            return Err(Error::ArrayCapacityExceeded);
        };

        if expected != 0 {
            let actual = u32::try_from(self.de.reader.stream_position()?)
                .map_err(|_| Error::Message("Compact array offset exceeds u32".into()))?;
            if actual != expected {
                return Err(Error::Message(format!(
                    "Compact array offset mismatch: expected {expected}, got {actual}"
                )));
            }
        }

        Ok(())
    }
}
impl<'de, 'a, R: Read + Seek> de::SeqAccess<'de> for RtonSeqAccess<'a, 'de, R> {
    type Error = Error;
    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        if !self.expects_end_marker && self.remaining_capacity == 0 {
            self.validate_compact_offset()?;
            return Ok(None);
        }

        let mut buf = [0u8; 1];
        self.validate_compact_offset()?;
        self.de.reader.read_exact(&mut buf)?;
        if self.expects_end_marker && buf[0] == RtonTag::ArrayEnd as u8 {
            return Ok(None);
        }
        if self.remaining_capacity == 0 {
            return Err(Error::ArrayCapacityExceeded);
        }
        self.de.reader.seek(SeekFrom::Current(-1))?;
        self.remaining_capacity -= 1;
        self.consumed += 1;
        seed.deserialize(&mut *self.de).map(Some)
    }
}

struct RtonMapAccess<'a, 'de, R> {
    de: &'a mut Deserializer<'de, R>,
}
impl<'a, 'de, R: Read + Seek> RtonMapAccess<'a, 'de, R> {
    fn new(de: &'a mut Deserializer<'de, R>) -> Self {
        Self { de }
    }
}
impl<'de, 'a, R: Read + Seek> de::MapAccess<'de> for RtonMapAccess<'a, 'de, R> {
    type Error = Error;
    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        let mut buf = [0u8; 1];
        self.de.reader.read_exact(&mut buf)?;
        if buf[0] == RtonTag::ObjectEnd as u8 {
            return Ok(None);
        }
        self.de.reader.seek(SeekFrom::Current(-1))?;
        seed.deserialize(&mut *self.de).map(Some)
    }
    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self.de)
    }
}
