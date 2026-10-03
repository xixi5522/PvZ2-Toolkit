use crate::{
    NewtonError, RawResource, RawResourceGroup, RawResourceManifest, Resource, ResourceGroup,
    ResourceManifest, Result, SimpleGroup, Subgroup,
};
use byteorder::{LE, WriteBytesExt};
use std::io::Write;

const ABSENT_COORDINATE: i32 = i32::MAX;
const DOCUMENT_HEADER_SIZE: usize = 8;
const GROUP_FIXED_SIZE: usize = 15;
const SUBGROUP_FIXED_SIZE: usize = 4;
const RESOURCE_FIXED_SIZE: usize = 49;

/// Validate that a semantic manifest can be represented by NEWTON without
/// writing any output.
pub fn validate_encoding(manifest: &ResourceManifest) -> Result<()> {
    encoded_len(manifest).map(|_| ())
}

/// Compute the exact encoded byte length of a semantic manifest.
pub fn encoded_len(manifest: &ResourceManifest) -> Result<usize> {
    integer("slot count", manifest.slot_count)?;
    count("group", manifest.groups.len())?;
    let mut size = DOCUMENT_HEADER_SIZE;

    for group in &manifest.groups {
        size = checked_add(size, GROUP_FIXED_SIZE, "document")?;
        size = checked_add(size, string_size(group.id())?, "document")?;
        if let Some(parent) = group.parent() {
            size = checked_add(size, string_size(parent)?, "document")?;
        }
        if let Some(resolution) = group.resolution() {
            integer("group resolution", resolution)?;
        }

        match group {
            ResourceGroup::Composite(group) => {
                count("subgroup", group.subgroups.len())?;
                for subgroup in &group.subgroups {
                    if let Some(resolution) = subgroup.resolution {
                        integer("subgroup resolution", resolution)?;
                    }
                    size = checked_add(size, SUBGROUP_FIXED_SIZE, "document")?;
                    size = checked_add(size, string_size(&subgroup.id)?, "document")?;
                }
            }
            ResourceGroup::Simple(group) => {
                count("resource", group.resources.len())?;
                for resource in &group.resources {
                    validate_resource_numbers(resource)?;
                    size = checked_add(size, RESOURCE_FIXED_SIZE, "document")?;
                    size = checked_add(size, string_size(&resource.id)?, "document")?;
                    size = checked_add(size, string_size(&resource.path)?, "document")?;
                    if let Some(parent) = &resource.parent {
                        size = checked_add(size, string_size(parent)?, "document")?;
                    }
                }
            }
        }
    }

    Ok(size)
}

/// Encode a semantic NEWTON document.
///
/// The entire document is preflighted before the first byte is written.
pub fn encode_newton(manifest: &ResourceManifest, mut writer: impl Write) -> Result<()> {
    validate_encoding(manifest)?;
    write_integer(&mut writer, manifest.slot_count)?;
    write_count(&mut writer, manifest.groups.len())?;

    for group in &manifest.groups {
        match group {
            ResourceGroup::Composite(group) => {
                writer.write_u8(1)?;
                write_optional_nonzero(&mut writer, group.resolution)?;
                write_count(&mut writer, group.subgroups.len())?;
                write_count(&mut writer, 0)?;
                write_group_header(&mut writer, &group.id, group.parent.as_deref())?;
                for subgroup in &group.subgroups {
                    write_subgroup(&mut writer, subgroup)?;
                }
            }
            ResourceGroup::Simple(group) => {
                writer.write_u8(2)?;
                write_optional_nonzero(&mut writer, group.resolution)?;
                write_count(&mut writer, 0)?;
                write_count(&mut writer, group.resources.len())?;
                write_group_header(&mut writer, &group.id, group.parent.as_deref())?;
                write_resources(&mut writer, group)?;
            }
        }
    }
    Ok(())
}

/// Compute the exact encoded length of a wire-faithful raw manifest.
pub fn raw_encoded_len<B: AsRef<[u8]>>(manifest: &RawResourceManifest<B>) -> Result<usize> {
    count("group", manifest.groups.len())?;
    let mut size = DOCUMENT_HEADER_SIZE;
    for group in &manifest.groups {
        count("subgroup", group.subgroups.len())?;
        count("resource", group.resources.len())?;
        size = checked_add(size, GROUP_FIXED_SIZE, "raw document")?;
        size = checked_add(
            size,
            optional_raw_string_size(group.has_id, group.id.as_ref(), "group id")?,
            "raw document",
        )?;
        size = checked_add(
            size,
            optional_raw_string_size(group.has_parent, group.parent.as_ref(), "group parent")?,
            "raw document",
        )?;
        for subgroup in &group.subgroups {
            size = checked_add(size, SUBGROUP_FIXED_SIZE, "raw document")?;
            size = checked_add(size, raw_string_size(subgroup.id.as_ref())?, "raw document")?;
        }
        for resource in &group.resources {
            size = checked_add(size, RESOURCE_FIXED_SIZE, "raw document")?;
            size = checked_add(
                size,
                optional_raw_string_size(resource.has_id, resource.id.as_ref(), "resource id")?,
                "raw document",
            )?;
            size = checked_add(
                size,
                optional_raw_string_size(
                    resource.has_path,
                    resource.path.as_ref(),
                    "resource path",
                )?,
                "raw document",
            )?;
            size = checked_add(
                size,
                optional_raw_string_size(
                    resource.has_parent,
                    resource.parent.as_ref(),
                    "resource parent",
                )?,
                "raw document",
            )?;
        }
    }
    Ok(size)
}

/// Encode a wire-faithful raw NEWTON document.
///
/// Raw flag bytes are preserved. A string tail is emitted only when its flag
/// byte is exactly `1`, matching the game reader.
pub fn encode_raw_newton<B: AsRef<[u8]>>(
    manifest: &RawResourceManifest<B>,
    mut writer: impl Write,
) -> Result<()> {
    raw_encoded_len(manifest)?;
    writer.write_i32::<LE>(manifest.slot_count)?;
    write_count(&mut writer, manifest.groups.len())?;
    for group in &manifest.groups {
        write_raw_group(&mut writer, group)?;
    }
    Ok(())
}

fn write_raw_group<B: AsRef<[u8]>>(
    writer: &mut impl Write,
    group: &RawResourceGroup<B>,
) -> Result<()> {
    writer.write_u8(group.group_type)?;
    writer.write_i32::<LE>(group.resolution)?;
    write_count(writer, group.subgroups.len())?;
    write_count(writer, group.resources.len())?;
    writer.write_u8(group.has_id)?;
    writer.write_u8(group.has_parent)?;
    write_optional_raw_string(writer, group.has_id, group.id.as_ref())?;
    write_optional_raw_string(writer, group.has_parent, group.parent.as_ref())?;
    for subgroup in &group.subgroups {
        writer.write_i32::<LE>(subgroup.resolution)?;
        write_raw_string(writer, subgroup.id.as_ref())?;
    }
    for resource in &group.resources {
        write_raw_resource(writer, resource)?;
    }
    Ok(())
}

fn write_raw_resource<B: AsRef<[u8]>>(
    writer: &mut impl Write,
    resource: &RawResource<B>,
) -> Result<()> {
    writer.write_u8(resource.resource_type)?;
    for value in [
        resource.slot,
        resource.width,
        resource.height,
        resource.x,
        resource.y,
        resource.ax,
        resource.ay,
        resource.aw,
        resource.ah,
        resource.cols,
        resource.rows,
    ] {
        writer.write_i32::<LE>(value)?;
    }
    writer.write_all(&[
        resource.atlas,
        resource.has_id,
        resource.has_path,
        resource.has_parent,
    ])?;
    write_optional_raw_string(writer, resource.has_id, resource.id.as_ref())?;
    write_optional_raw_string(writer, resource.has_path, resource.path.as_ref())?;
    write_optional_raw_string(writer, resource.has_parent, resource.parent.as_ref())
}

fn write_group_header(writer: &mut impl Write, id: &str, parent: Option<&str>) -> Result<()> {
    writer.write_u8(1)?;
    write_bool(writer, parent.is_some())?;
    write_string(writer, id)?;
    if let Some(parent) = parent {
        write_string(writer, parent)?;
    }
    Ok(())
}

fn write_subgroup(writer: &mut impl Write, subgroup: &Subgroup) -> Result<()> {
    write_optional_nonzero(writer, subgroup.resolution)?;
    write_string(writer, &subgroup.id)
}

fn write_resources(writer: &mut impl Write, group: &SimpleGroup) -> Result<()> {
    for resource in &group.resources {
        write_resource(writer, resource)?;
    }
    Ok(())
}

fn write_resource(writer: &mut impl Write, resource: &Resource) -> Result<()> {
    writer.write_u8(resource.resource_type.to_u8())?;
    write_integer(writer, resource.slot)?;
    write_optional_nonzero(writer, resource.width)?;
    write_optional_nonzero(writer, resource.height)?;
    writer.write_i32::<LE>(coordinate_or_default(resource.x, resource))?;
    writer.write_i32::<LE>(coordinate_or_default(resource.y, resource))?;
    write_optional_nonzero(writer, resource.ax)?;
    write_optional_nonzero(writer, resource.ay)?;
    write_optional_nonzero(writer, resource.aw)?;
    write_optional_nonzero(writer, resource.ah)?;
    write_optional_default_one(writer, resource.cols)?;
    write_optional_default_one(writer, resource.rows)?;
    write_bool(writer, resource.atlas)?;
    write_bool(writer, true)?;
    write_bool(writer, true)?;
    write_bool(writer, resource.parent.is_some())?;
    write_string(writer, &resource.id)?;
    write_string(writer, &resource.path)?;
    if let Some(parent) = &resource.parent {
        write_string(writer, parent)?;
    }
    Ok(())
}

fn validate_resource_numbers(resource: &Resource) -> Result<()> {
    integer("resource slot", resource.slot)?;
    for (field, value) in [
        ("resource width", resource.width),
        ("resource height", resource.height),
        ("resource atlas x", resource.ax),
        ("resource atlas y", resource.ay),
        ("resource atlas width", resource.aw),
        ("resource atlas height", resource.ah),
        ("resource columns", resource.cols),
        ("resource rows", resource.rows),
    ] {
        if let Some(value) = value {
            integer(field, value)?;
        }
    }
    Ok(())
}

fn coordinate_or_default(value: Option<i32>, resource: &Resource) -> i32 {
    value.unwrap_or_else(|| {
        if resource.is_sprite() {
            0
        } else {
            ABSENT_COORDINATE
        }
    })
}

fn write_optional_raw_string<B: AsRef<[u8]>>(
    writer: &mut impl Write,
    flag: u8,
    value: Option<&B>,
) -> Result<()> {
    if flag == 1 {
        write_raw_string(
            writer,
            value
                .ok_or(NewtonError::InconsistentPresence {
                    field: "raw string",
                    flag,
                    has_value: false,
                })?
                .as_ref(),
        )?;
    }
    Ok(())
}

fn write_raw_string(writer: &mut impl Write, value: &[u8]) -> Result<()> {
    let length =
        i32::try_from(value.len()).map_err(|_| NewtonError::LengthOverflow { field: "string" })?;
    writer.write_i32::<LE>(length)?;
    writer.write_all(value)?;
    Ok(())
}

fn write_string(writer: &mut impl Write, value: &str) -> Result<()> {
    write_raw_string(writer, value.as_bytes())
}

fn write_bool(writer: &mut impl Write, value: bool) -> Result<()> {
    writer.write_u8(u8::from(value))?;
    Ok(())
}

fn write_integer(writer: &mut impl Write, value: u32) -> Result<()> {
    writer.write_i32::<LE>(i32::try_from(value).map_err(|_| {
        NewtonError::IntegerOutOfRange {
            field: "integer",
            value,
        }
    })?)?;
    Ok(())
}

fn write_optional_nonzero(writer: &mut impl Write, value: Option<u32>) -> Result<()> {
    write_integer(writer, value.unwrap_or(0))
}

fn write_optional_default_one(writer: &mut impl Write, value: Option<u32>) -> Result<()> {
    write_integer(writer, value.unwrap_or(1))
}

fn write_count(writer: &mut impl Write, value: usize) -> Result<()> {
    writer.write_i32::<LE>(
        i32::try_from(value).map_err(|_| NewtonError::LengthOverflow { field: "count" })?,
    )?;
    Ok(())
}

fn integer(field: &'static str, value: u32) -> Result<i32> {
    i32::try_from(value).map_err(|_| NewtonError::IntegerOutOfRange { field, value })
}

fn count(field: &'static str, value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| NewtonError::LengthOverflow { field })
}

fn string_size(value: &str) -> Result<usize> {
    raw_string_size(value.as_bytes())
}

fn raw_string_size(value: &[u8]) -> Result<usize> {
    i32::try_from(value.len()).map_err(|_| NewtonError::LengthOverflow { field: "string" })?;
    checked_add(4, value.len(), "string")
}

fn optional_raw_string_size<B: AsRef<[u8]>>(
    flag: u8,
    value: Option<&B>,
    field: &'static str,
) -> Result<usize> {
    match (flag == 1, value) {
        (true, Some(value)) => raw_string_size(value.as_ref()),
        (false, None) => Ok(0),
        (_, value) => Err(NewtonError::InconsistentPresence {
            field,
            flag,
            has_value: value.is_some(),
        }),
    }
}

fn checked_add(left: usize, right: usize, field: &'static str) -> Result<usize> {
    left.checked_add(right)
        .ok_or(NewtonError::LengthOverflow { field })
}
