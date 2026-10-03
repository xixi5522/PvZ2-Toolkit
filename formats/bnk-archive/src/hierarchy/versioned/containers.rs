//! Actor playlist, switch, and blend container layouts.

use super::audio::{parse_parameter_node, parse_rtpc};
use crate::error::Result;
use crate::hierarchy::{FieldReader, parse_children, parse_graph_points};
use crate::semantics::{EnumerationKind, PackedLayout};
use crate::version::BankVersion;

pub(super) fn parse_playlist_container(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
) -> Result<()> {
    parse_parameter_node(reader, "node", version)?;
    reader.i16("playback.loop_count")?;
    if version.at_least(88) {
        reader.i16("playback.loop_minimum")?;
        reader.i16("playback.loop_maximum")?;
    }
    reader.f32("playback.transition_time")?;
    reader.f32("playback.transition_minimum")?;
    reader.f32("playback.transition_maximum")?;
    reader.u16("playback.avoid_repeat_count")?;
    reader.enum_u8(
        "playback.transition_mode",
        EnumerationKind::PlaylistTransitionMode,
        version,
    )?;
    reader.enum_u8(
        "playback.random_mode",
        EnumerationKind::PlaylistRandomMode,
        version,
    )?;
    reader.enum_u8(
        "playback.container_mode",
        EnumerationKind::PlaylistContainerMode,
        version,
    )?;
    if version.before(112) {
        reader.constant_u8("playback.reserved", 0)?;
        reader.packed_u8("playback.always_reset", PackedLayout::Boolean)?;
        reader.enum_u8(
            "playback.end_behavior",
            EnumerationKind::AudioPlaySequenceEnd,
            version,
        )?;
        reader.enum_u8(
            "playback.play_mode",
            EnumerationKind::AudioPlayMode,
            version,
        )?;
        reader.enum_u8(
            "playback.scope",
            EnumerationKind::SoundPlaylistScope,
            version,
        )?;
    } else {
        reader.packed_u8("playback.flags", PackedLayout::Playlist)?;
    }
    parse_children(reader, "children")?;
    let count = reader.u16("playlist.count")? as usize;
    for index in 0..count {
        reader.id(format!("playlist.items[{index}].child_id"))?;
        reader.u32(format!("playlist.items[{index}].weight"))?;
    }
    Ok(())
}

pub(super) fn parse_switch_container(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
) -> Result<()> {
    parse_parameter_node(reader, "node", version)?;
    if version.before(112) {
        reader.packed_u32("switcher.group_type", PackedLayout::Boolean)?;
    } else {
        reader.packed_u8("switcher.group_type", PackedLayout::Boolean)?;
    }
    reader.id("switcher.group_id")?;
    reader.id("switcher.default_value")?;
    reader.enum_u8("playback.mode", EnumerationKind::AudioPlayMode, version)?;
    parse_children(reader, "children")?;
    let group_count = reader.u32("assignments.count")? as usize;
    for group_index in 0..group_count {
        let group = format!("assignments.groups[{group_index}]");
        reader.id(format!("{group}.value_id"))?;
        let count = reader.u32(format!("{group}.item_count"))? as usize;
        for index in 0..count {
            reader.id(format!("{group}.items[{index}]"))?;
        }
    }
    let count = reader.u32("attributes.count")? as usize;
    for index in 0..count {
        let item = format!("attributes.items[{index}]");
        reader.id(format!("{item}.node_id"))?;
        if version.before(112) {
            reader.packed_u8(format!("{item}.play_first_only"), PackedLayout::Boolean)?;
            reader.packed_u8(
                format!("{item}.continue_across_switch"),
                PackedLayout::Boolean,
            )?;
            reader.u32(format!("{item}.u1"))?;
        } else {
            reader.packed_u8(
                format!("{item}.playback_flags"),
                PackedLayout::SwitchPlayback,
            )?;
            reader.u8(format!("{item}.u1"))?;
        }
        reader.u32(format!("{item}.fade_out_time"))?;
        reader.u32(format!("{item}.fade_in_time"))?;
    }
    Ok(())
}

pub(super) fn parse_blend_container(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
) -> Result<()> {
    parse_parameter_node(reader, "node", version)?;
    parse_children(reader, "children")?;
    let count = reader.u32("layers.count")? as usize;
    for layer_index in 0..count {
        let layer = format!("layers.items[{layer_index}]");
        reader.id(format!("{layer}.id"))?;
        parse_rtpc(reader, &format!("{layer}.rtpc"), version)?;
        reader.id(format!("{layer}.crossfade_parameter"))?;
        if version.at_least(112) {
            reader.enum_u8(
                format!("{layer}.parameter_category"),
                EnumerationKind::ParameterCategory,
                version,
            )?;
        }
        let associations = reader.u32(format!("{layer}.association_count"))? as usize;
        for index in 0..associations {
            let association = format!("{layer}.associations[{index}]");
            reader.id(format!("{association}.child_id"))?;
            let points = reader.u32(format!("{association}.point_count"))? as usize;
            parse_graph_points(reader, &format!("{association}.points"), points, version)?;
        }
    }
    if version.at_least(120) {
        reader.enum_u8("playback.mode", EnumerationKind::AudioPlayMode, version)?;
    }
    Ok(())
}
