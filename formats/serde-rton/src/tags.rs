//! RTON file markers and tag identifiers.
//!
//! These types model the on-disk tag bytes used by standard RTON and PvZ2
//! compact runtime RTON.

use num_enum::TryFromPrimitive;

// ================= CONSTANTS =================

/// Standard RTON file magic.
pub const FILE_HEADER: &[u8] = b"RTON";
/// Standard RTON footer marker.
pub const FILE_FOOTER: &[u8] = b"DONE";
/// Standard RTON low-word file version.
pub const STANDARD_FILE_VERSION: u32 = 1;
/// PvZ2 compact runtime RTON file version.
pub const COMPACT_FILE_VERSION: u32 = 0x0001_0001;

/// Raw RTON tag byte identifiers.
///
/// Standard RTON uses tags such as `0x85` for objects and `0x90`-`0x93` for
/// interned strings. PvZ2 compact runtime RTON additionally uses `0xB0`-`0xBC`
/// tags for offset-addressed strings, compact arrays, RTID, binary blobs, and
/// booleans.
#[derive(Debug, Eq, PartialEq, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum RtonTag {
    /// Boolean false.
    BooleanFalse = 0x00,
    /// Boolean true.
    BooleanTrue = 0x01,
    /// Literal `*` string marker.
    StringAsterisk = 0x02,

    I8 = 0x08,
    I8Zero = 0x09,
    U8 = 0x0a,
    U8Zero = 0x0b,

    I16 = 0x10,
    I16Zero = 0x11,
    U16 = 0x12,
    U16Zero = 0x13,

    I32 = 0x20,
    I32Zero = 0x21,
    U32 = 0x26,
    U32Zero = 0x27,

    I64 = 0x40,
    I64Zero = 0x41,
    U64 = 0x46,
    U64Zero = 0x47,

    RawVarInt32 = 0x24,
    ZigZagVarInt32 = 0x25,
    /// Unsigned varint (adaptive in PvZ2's dedicated unsigned writer).
    UnsignedVarInt32 = 0x28,

    RawVarInt64 = 0x44,
    ZigZagVarInt64 = 0x45,
    /// Unsigned varint alt (adaptive in PvZ2's dedicated unsigned writer).
    UnsignedVarInt64 = 0x48,

    F32 = 0x22,
    F32Zero = 0x23,
    F64 = 0x42,
    F64Zero = 0x43,

    /// Direct Latin-1 / single-byte string.
    StringLatin1Direct = 0x81,
    /// Direct UTF-8 string.
    StringUtf8Direct = 0x82,
    /// Interned Latin-1 / single-byte string definition.
    StringLatin1Definition = 0x90,
    /// Reference to a previous [`RtonTag::StringLatin1Definition`].
    StringLatin1Reference = 0x91,
    /// Interned UTF-8 string definition.
    StringUtf8Definition = 0x92,
    /// Reference to a previous [`RtonTag::StringUtf8Definition`].
    StringUtf8Reference = 0x93,

    BinaryBlob = 0x87,

    Rtid = 0x83,
    RtidNull = 0x84,

    ObjectBegin = 0x85,
    ArrayBegin = 0x86,

    /// Standard array capacity marker.
    ///
    /// Standard arrays may terminate early with [`RtonTag::ArrayEnd`]. Compact
    /// arrays use this field as an exact element count.
    ArrayCapacity = 0xfd,

    /// Standard array end marker.
    ArrayEnd = 0xfe,

    /// Object end marker.
    ObjectEnd = 0xff,

    // ---- Compact-transcode tags (0xB0–0xBC) -----------------------------------
    //
    // PvZ2 emits these tags exclusively on the compact-transcode path
    // (sub_1024e7b5c → sub_1024e7d18 → sub_1024eb604), triggered during
    // resource/package loading (mResourceManager->Init → sub_1024e2b48).
    // They are NOT used by the main JSON→RTON writer and do not appear in
    // standard .rton distribution files — they are a runtime memory format.
    /// Compact Latin-1 / single-byte string definition.
    ///
    /// Payload: `u32 byte_len_including_nul`, then Latin-1 bytes including the
    /// trailing NUL.  References point at the absolute output offset of this
    /// byte payload.
    CompactLatin1StringDefinition = 0xB0,

    /// Compact Latin-1 / single-byte string reference.
    ///
    /// Payload: `u32 payload_offset`.
    CompactLatin1StringReference = 0xB1,

    /// Compact UTF-32 string definition.
    ///
    /// Despite the historical name, the compact payload is UTF-32LE-ish:
    /// `u32 byte_len` followed by 32-bit codepoints including a trailing zero.
    CompactUtf32StringDefinition = 0xB2,

    /// Compact UTF-32 string reference.
    ///
    /// Payload: `u32 payload_offset`.
    CompactUtf32StringReference = 0xB3,

    /// Compact Latin-1 / single-byte string definition with value-end offset tracking.
    ///
    /// Payload is B0 plus an extra trailing `u32`.
    CompactLatin1StringDefinitionWithValueOffset = 0xB4,

    /// Compact Latin-1 / single-byte string reference with value-end offset tracking.
    ///
    /// Payload is B1 plus an extra trailing `u32`.
    CompactLatin1StringReferenceWithValueOffset = 0xB5,

    /// Compact UTF-32 string definition with value-end offset tracking.
    ///
    /// Payload is B2 plus an extra trailing `u32`.
    CompactUtf32StringDefinitionWithValueOffset = 0xB6,

    /// Compact UTF-32 string reference with value-end offset tracking.
    ///
    /// Payload is B3 plus an extra trailing `u32`.
    CompactUtf32StringReferenceWithValueOffset = 0xB7,

    /// Compact object start (compact-path ≈ 0x85).
    CompactObjectBegin = 0xB8,

    /// Compact array start.
    ///
    /// Payload: `0xFD`, `u32 count`, `u32[count + 1]` element-offset table,
    /// then exactly `count` elements.  There is no trailing 0xFE marker.
    CompactArrayBegin = 0xB9,

    /// Compact RTID / RTID-zero.
    ///
    /// ⚠ Previously misnamed `StrNativeX3` (inherited from Sen's reference
    /// implementation).  Hopper decompilation of sub_1024eb604 confirms
    /// this tag encodes RTID values, not strings.
    CompactRtid = 0xBA,

    /// Compact binary blob.
    ///
    /// Payload: a compact Latin-1 hex string (B0/B1/B4/B5), then `u32 raw_len`.
    CompactBinaryBlob = 0xBB,

    /// Bool with a payload byte (0 → false, non-zero → true).
    CompactBoolean = 0xBC,
}

/// RTID payload sub-tag byte identifiers.
#[derive(Debug, Eq, PartialEq, TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum RtidPayloadTag {
    /// RTID zero / null.
    Null = 0x00,
    /// Numeric RTID without a name payload.
    UidWithoutName = 0x01,
    /// Numeric RTID with a UTF-8 name payload.
    UidWithName = 0x02,
    /// Raw `name@parent` RTID payload.
    RawString = 0x03,
}
