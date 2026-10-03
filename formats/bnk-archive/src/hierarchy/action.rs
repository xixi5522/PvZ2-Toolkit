//! Event-action and dialogue-event field layouts.

use super::{
    FieldReader, HierarchyFields, indexed, parse_association_paths, parse_common_properties,
};
use crate::error::{BnkError, Result};
use crate::limits::{DecodeLimits, ValidationMode};
use crate::semantics::{CommonPropertyDomain, EnumerationKind, PackedLayout};
use crate::version::BankVersion;

pub(crate) fn decode_dialogue_fields(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    validation: ValidationMode,
    limits: DecodeLimits,
) -> Result<HierarchyFields> {
    let mut reader = FieldReader::new(bytes, offset, validation, limits);
    let depth = reader.u32("association.depth")? as usize;
    for index in 0..depth {
        reader.id(format!("association.arguments[{index}].group_id"))?;
    }
    if version.at_least(88) {
        for index in 0..depth {
            reader.packed_u8(
                format!("association.arguments[{index}].is_state"),
                PackedLayout::Boolean,
            )?;
        }
    }
    let size = reader.u32("association.path_byte_size")? as usize;
    if version.before(88) {
        reader.u8("association.probability")?;
    }
    reader.enum_u8(
        "association.mode",
        EnumerationKind::AssociationMode,
        version,
    )?;
    parse_association_paths(&mut reader, "association.paths", size)?;
    if version.at_least(120) {
        // Twinning models this as a fixed zero u16, not as an empty common
        // property map (the two wire representations happen to be identical).
        reader.constant_u16("reserved", 0)?;
    }
    reader.finish().map_err(|error| {
        BnkError::invalid("dialogue event", offset, format!("association: {error}"))
    })
}

pub(crate) fn decode_event_action_fields(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    action_type: u8,
    validation: ValidationMode,
    limits: DecodeLimits,
) -> Result<HierarchyFields> {
    let mut reader = FieldReader::new(bytes, offset, validation, limits);
    parse_common_properties(
        &mut reader,
        "properties",
        true,
        CommonPropertyDomain::EventAction,
        version,
    )?;
    match action_type {
        1 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            if version.at_least(125) {
                reader.packed_u8("stop_flags", PackedLayout::EventActionStop)?;
            }
            parse_action_exceptions(&mut reader, version)?;
        }
        2 | 3 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            reader.packed_u8(
                "pause_resume_flags",
                if version.before(125) {
                    PackedLayout::EventActionPauseResumeLegacy
                } else {
                    PackedLayout::EventActionPauseResume
                },
            )?;
            parse_action_exceptions(&mut reader, version)?;
        }
        4 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            reader.id("sound_bank")?;
            if version.at_least(145) {
                reader.constant_u32("reserved", 0)?;
            }
        }
        6 | 7 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            parse_action_exceptions(&mut reader, version)?;
        }
        8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 32 | 48 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            reader.enum_u8("apply_mode", EnumerationKind::ValueApplyMode, version)?;
            reader.f32("value")?;
            reader.f32("minimum")?;
            reader.f32("maximum")?;
            parse_action_exceptions(&mut reader, version)?;
        }
        16 | 17 | 28 | 29 | 31 | 33 => {}
        18 => {
            reader.id("group")?;
            reader.id("item")?;
        }
        19 | 20 => {
            reader.enum_u8("fade_curve", EnumerationKind::FadeCurve, version)?;
            if version.at_least(112) {
                reader.packed_u8("bypass_game_parameter_interpolation", PackedLayout::Boolean)?;
            }
            reader.enum_u8("apply_mode", EnumerationKind::ValueApplyMode, version)?;
            reader.f32("value")?;
            reader.f32("minimum")?;
            reader.f32("maximum")?;
            parse_action_exceptions(&mut reader, version)?;
        }
        25 => {
            reader.id("group")?;
            reader.id("item")?;
        }
        26 | 27 => {
            reader.packed_u8("enable", PackedLayout::Boolean)?;
            reader.packed_u8(
                "effect_bypass_flags",
                if action_type == 27 {
                    PackedLayout::EventActionEffectBypassReset
                } else {
                    PackedLayout::EventActionEffectBypassSet
                },
            )?;
            parse_action_exceptions(&mut reader, version)?;
        }
        30 => {
            reader.enum_u8("seek_type", EnumerationKind::SeekType, version)?;
            reader.f32("seek_value")?;
            reader.f32("seek_minimum")?;
            reader.f32("seek_maximum")?;
            reader.packed_u8("seek_to_nearest_marker", PackedLayout::Boolean)?;
            parse_action_exceptions(&mut reader, version)?;
        }
        34 => {
            reader.constant_u8("fade_curve", 4)?;
            if version.before(115) {
                reader.constant_u32("reserved", 0)?;
            } else {
                reader.constant_u8("reserved", 0)?;
            }
        }
        _ => {
            if reader.remaining() != 0 {
                reader.bytes("action_specific.unclassified", reader.remaining())?;
            }
        }
    }
    reader.finish().map_err(|error| {
        BnkError::invalid(
            "event action",
            offset,
            format!("type {action_type}: {error}"),
        )
    })
}

fn parse_action_exceptions(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    let count = if version.before(125) {
        reader.u32("exceptions.count")? as usize
    } else {
        reader.u8("exceptions.count")? as usize
    };
    for index in 0..count {
        reader.id(indexed("exceptions.items", index, "identifier"))?;
        reader.packed_u8(
            indexed("exceptions.items", index, "u1"),
            PackedLayout::Boolean,
        )?;
    }
    Ok(())
}
