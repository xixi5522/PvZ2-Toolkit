use std::io::{Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{PacketPatch, PatchError, RSB_PATCH_MAGIC, RSB_PATCH_VERSION, Result, RsbPatch};

const PACKAGE_CONSTANT: u32 = 2;
const PACKAGE_RESERVED: u32 = 0;
const PACKET_NAME_SIZE: usize = 128;

/// Resource limits used while parsing an untrusted RSBP container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerDecodeOptions {
    /// Maximum number of packet records accepted from one container.
    pub max_packet_count: usize,
    /// Maximum byte length of any individual VCDIFF payload.
    pub max_delta_size: usize,
    /// Maximum combined byte length of all VCDIFF payloads.
    pub max_total_delta_size: usize,
}

impl ContainerDecodeOptions {
    /// Construct explicit RSBP parsing limits.
    pub const fn new(
        max_packet_count: usize,
        max_delta_size: usize,
        max_total_delta_size: usize,
    ) -> Self {
        Self {
            max_packet_count,
            max_delta_size,
            max_total_delta_size,
        }
    }

    /// Limits compatible with Twinning's signed 32-bit VCDIFF output bound.
    pub const fn twinning() -> Self {
        Self::new(1_000_000, 0x7FFF_FFFF, 0x7FFF_FFFF)
    }
}

impl Default for ContainerDecodeOptions {
    fn default() -> Self {
        Self::twinning()
    }
}

pub(crate) fn read<R: Read>(mut reader: R) -> Result<RsbPatch> {
    read_with_options(&mut reader, ContainerDecodeOptions::default())
}

pub(crate) fn read_with_options<R: Read>(
    mut reader: R,
    options: ContainerDecodeOptions,
) -> Result<RsbPatch> {
    let mut magic_bytes = [0_u8; 4];
    reader.read_exact(&mut magic_bytes)?;
    if u32::from_le_bytes(magic_bytes) != RSB_PATCH_MAGIC {
        return Err(PatchError::InvalidMagic {
            actual: magic_bytes,
        });
    }

    let version = reader.read_u32::<LittleEndian>()?;
    if version != RSB_PATCH_VERSION {
        return Err(PatchError::UnsupportedVersion {
            expected: RSB_PATCH_VERSION,
            actual: version,
        });
    }

    read_constant(&mut reader, "package marker", PACKAGE_CONSTANT)?;
    let all_after_size = reader.read_u32::<LittleEndian>()?;
    read_constant(&mut reader, "package reserved field", PACKAGE_RESERVED)?;
    let information_patch_size = reader.read_u32::<LittleEndian>()?;
    let mut before_hash = [0_u8; 16];
    reader.read_exact(&mut before_hash)?;
    let packet_count = reader.read_u32::<LittleEndian>()?;
    let information_patch_exists = read_flag(&mut reader, "information patch")?;
    validate_patch_state(
        "information patch",
        information_patch_exists,
        information_patch_size,
    )?;

    let packet_count = usize::try_from(packet_count).map_err(|_| PatchError::ValueOverflow {
        field: "packet count",
        value: usize::MAX,
    })?;
    enforce_limit("packet count", packet_count, options.max_packet_count)?;
    let mut total_delta_size = 0_usize;
    let information_patch = read_patch(
        &mut reader,
        information_patch_exists,
        information_patch_size,
        options,
        &mut total_delta_size,
    )?;
    let mut packets = Vec::new();
    for _ in 0..packet_count {
        let patch_exists = read_flag(&mut reader, "packet patch")?;
        let patch_size = reader.read_u32::<LittleEndian>()?;
        validate_patch_state("packet patch", patch_exists, patch_size)?;

        let mut name_buffer = [0_u8; PACKET_NAME_SIZE];
        reader.read_exact(&mut name_buffer)?;
        let name_length = name_buffer
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(PACKET_NAME_SIZE);
        let name = String::from_utf8(name_buffer[..name_length].to_vec())?;

        let mut packet_before_hash = [0_u8; 16];
        reader.read_exact(&mut packet_before_hash)?;
        let patch = read_patch(
            &mut reader,
            patch_exists,
            patch_size,
            options,
            &mut total_delta_size,
        )?;
        packets.push(PacketPatch {
            name,
            before_hash: packet_before_hash,
            patch,
        });
    }

    let trailing = std::io::copy(&mut reader, &mut std::io::sink())?;
    if trailing != 0 {
        return Err(PatchError::TrailingData {
            bytes: usize::try_from(trailing).unwrap_or(usize::MAX),
        });
    }

    Ok(RsbPatch {
        all_after_size,
        before_hash,
        information_patch,
        packets,
    })
}

pub(crate) fn write<W: Write>(mut writer: W, patch: &RsbPatch) -> Result<()> {
    let information_patch_size =
        optional_patch_size("information patch size", &patch.information_patch)?;
    let packet_count =
        u32::try_from(patch.packets.len()).map_err(|_| PatchError::ValueOverflow {
            field: "packet count",
            value: patch.packets.len(),
        })?;

    writer.write_u32::<LittleEndian>(RSB_PATCH_MAGIC)?;
    writer.write_u32::<LittleEndian>(RSB_PATCH_VERSION)?;
    writer.write_u32::<LittleEndian>(PACKAGE_CONSTANT)?;
    writer.write_u32::<LittleEndian>(patch.all_after_size)?;
    writer.write_u32::<LittleEndian>(PACKAGE_RESERVED)?;
    writer.write_u32::<LittleEndian>(information_patch_size)?;
    writer.write_all(&patch.before_hash)?;
    writer.write_u32::<LittleEndian>(packet_count)?;
    writer.write_u32::<LittleEndian>(u32::from(patch.information_patch.is_some()))?;
    if let Some(information_patch) = &patch.information_patch {
        writer.write_all(information_patch)?;
    }

    for packet in &patch.packets {
        let packet_patch_size = optional_patch_size("packet patch size", &packet.patch)?;
        let name = packet.name.as_bytes();
        if name.contains(&0) {
            return Err(PatchError::PacketNameContainsNul);
        }
        if name.len() > PACKET_NAME_SIZE {
            return Err(PatchError::PacketNameTooLong { length: name.len() });
        }

        writer.write_u32::<LittleEndian>(u32::from(packet.patch.is_some()))?;
        writer.write_u32::<LittleEndian>(packet_patch_size)?;
        writer.write_all(name)?;
        writer.write_all(&[0_u8; PACKET_NAME_SIZE][name.len()..])?;
        writer.write_all(&packet.before_hash)?;
        if let Some(packet_patch) = &packet.patch {
            writer.write_all(packet_patch)?;
        }
    }

    Ok(())
}

fn read_constant<R: Read>(reader: &mut R, field: &'static str, expected: u32) -> Result<()> {
    let actual = reader.read_u32::<LittleEndian>()?;
    if actual != expected {
        return Err(PatchError::InvalidConstant {
            field,
            expected,
            actual,
        });
    }
    Ok(())
}

fn read_flag<R: Read>(reader: &mut R, field: &'static str) -> Result<bool> {
    match reader.read_u32::<LittleEndian>()? {
        0 => Ok(false),
        1 => Ok(true),
        actual => Err(PatchError::InvalidFlag { field, actual }),
    }
}

fn validate_patch_state(field: &'static str, exists: bool, size: u32) -> Result<()> {
    if exists == (size == 0) {
        return Err(PatchError::InconsistentPatchState { field });
    }
    Ok(())
}

fn read_patch<R: Read>(
    reader: &mut R,
    exists: bool,
    size: u32,
    options: ContainerDecodeOptions,
    total_size: &mut usize,
) -> Result<Option<Vec<u8>>> {
    if !exists {
        return Ok(None);
    }
    let size = size as usize;
    enforce_limit("delta size", size, options.max_delta_size)?;
    *total_size = total_size
        .checked_add(size)
        .ok_or(PatchError::ContainerLimitExceeded {
            field: "total delta size",
            limit: options.max_total_delta_size,
            actual: usize::MAX,
        })?;
    enforce_limit(
        "total delta size",
        *total_size,
        options.max_total_delta_size,
    )?;
    let mut patch = vec![0_u8; size];
    reader.read_exact(&mut patch)?;
    Ok(Some(patch))
}

fn enforce_limit(field: &'static str, actual: usize, limit: usize) -> Result<()> {
    if actual > limit {
        return Err(PatchError::ContainerLimitExceeded {
            field,
            limit,
            actual,
        });
    }
    Ok(())
}

fn optional_patch_size(field: &'static str, patch: &Option<Vec<u8>>) -> Result<u32> {
    match patch {
        Some(bytes) if bytes.is_empty() => Err(PatchError::InconsistentPatchState { field }),
        Some(bytes) => u32::try_from(bytes.len()).map_err(|_| PatchError::ValueOverflow {
            field,
            value: bytes.len(),
        }),
        None => Ok(0),
    }
}
