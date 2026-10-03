//! Lossless typed field representation and versioned HIRC layouts.

use crate::error::{BnkError, Result};
use crate::limits::{DecodeLimits, ValidationMode};
use crate::semantics::{
    CommonPropertyDomain, CommonPropertyValueKind, EnumerationKind, HierarchyFieldSemantic,
    common_property_descriptor,
};
use crate::types::HierarchyKind;
use crate::version::BankVersion;

mod action;
mod field;
mod versioned;

pub(crate) use action::{decode_dialogue_fields, decode_event_action_fields};
use field::{FieldReader, indexed};
pub use field::{HierarchyField, HierarchyFieldIndex, HierarchyFieldValue, HierarchyFields};

fn parse_common_property_value(
    reader: &mut FieldReader<'_>,
    path: String,
    domain: CommonPropertyDomain,
    version: BankVersion,
    property_id: u8,
) -> Result<u32> {
    let offset = reader.offset();
    let descriptor = common_property_descriptor(version.number(), domain, property_id);
    if reader.validation.is_strict() && descriptor.is_none() {
        return Err(BnkError::invalid(
            "HIRC common property",
            offset,
            format!(
                "{path} uses unknown {domain:?} property identifier {property_id} in Wwise {}",
                version.number()
            ),
        ));
    }
    let value = reader.property_semantic(
        path.clone(),
        Some(HierarchyFieldSemantic::CommonProperty {
            domain,
            version: version.number(),
            property_id,
        }),
    )?;
    if let Some(descriptor) = descriptor {
        match descriptor.kind {
            CommonPropertyValueKind::Boolean if reader.validation.is_strict() && value > 1 => {
                return Err(BnkError::invalid(
                    "HIRC common property",
                    offset,
                    format!("{path} has non-boolean value {value}"),
                ));
            }
            CommonPropertyValueKind::Enumerated => {
                let enumeration = descriptor.enumeration.ok_or_else(|| {
                    BnkError::invalid(
                        "HIRC common property",
                        offset,
                        format!("{path} has no Twinning enumeration"),
                    )
                })?;
                reader.validate_enumeration(offset, &path, enumeration, version, value.into())?;
            }
            _ => {}
        }
    }
    Ok(value)
}

pub(crate) fn parse_common_properties(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    randomizable: bool,
    domain: CommonPropertyDomain,
    version: BankVersion,
) -> Result<()> {
    let regular_count = reader.u8(format!("{prefix}.regular_count"))? as usize;
    let mut ids = Vec::with_capacity(regular_count);
    for index in 0..regular_count {
        ids.push(reader.u8(indexed(&format!("{prefix}.regular"), index, "property_id"))?);
    }
    for (index, property_id) in ids.into_iter().enumerate() {
        parse_common_property_value(
            reader,
            indexed(
                &format!("{prefix}.regular"),
                index,
                &format!("value_for_{property_id}"),
            ),
            domain,
            version,
            property_id,
        )?;
    }

    if randomizable {
        let random_count = reader.u8(format!("{prefix}.randomizer_count"))? as usize;
        let mut ids = Vec::with_capacity(random_count);
        for index in 0..random_count {
            ids.push(reader.u8(indexed(
                &format!("{prefix}.randomizers"),
                index,
                "property_id",
            ))?);
        }
        for (index, property_id) in ids.into_iter().enumerate() {
            let base = format!("{prefix}.randomizers[{index}].property_{property_id}");
            parse_common_property_value(
                reader,
                format!("{base}.minimum"),
                domain,
                version,
                property_id,
            )?;
            parse_common_property_value(
                reader,
                format!("{base}.maximum"),
                domain,
                version,
                property_id,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn parse_graph_points(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    count: usize,
    version: BankVersion,
) -> Result<()> {
    for index in 0..count {
        reader.f32(indexed(prefix, index, "x"))?;
        reader.f32(indexed(prefix, index, "y"))?;
        reader.enum_u32(
            indexed(prefix, index, "curve"),
            EnumerationKind::FadeCurve,
            version,
        )?;
    }
    Ok(())
}

pub(crate) fn parse_children(reader: &mut FieldReader<'_>, prefix: &str) -> Result<()> {
    let count = reader.u32(format!("{prefix}.count"))? as usize;
    for index in 0..count {
        reader.id(format!("{prefix}.items[{index}]"))?;
    }
    Ok(())
}

pub(crate) fn parse_association_paths(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    size: usize,
) -> Result<()> {
    if !size.is_multiple_of(12) {
        return Err(BnkError::invalid(
            "association path list",
            reader.offset(),
            format!("path byte size {size} is not divisible by 12"),
        ));
    }
    for index in 0..size / 12 {
        let item = format!("{prefix}[{index}]");
        reader.id(format!("{item}.u1"))?;
        reader.id(format!("{item}.object"))?;
        reader.u16(format!("{item}.weight"))?;
        reader.u16(format!("{item}.probability"))?;
    }
    Ok(())
}

pub(crate) fn decode_fields(
    kind: HierarchyKind,
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    validation: ValidationMode,
    limits: DecodeLimits,
) -> Result<HierarchyFields> {
    versioned::decode_fields(kind, bytes, offset, version, validation, limits)
}
