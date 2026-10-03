//! Music track, segment, playlist, and switch layouts.

use super::audio::{parse_audio_source, parse_parameter_node};
use crate::error::Result;
use crate::hierarchy::{FieldReader, parse_association_paths, parse_children, parse_graph_points};
use crate::semantics::{EnumerationKind, PackedLayout};
use crate::version::BankVersion;

fn parse_music_common(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    if version.at_least(112) {
        reader.packed_u8(format!("{prefix}.midi_flags"), PackedLayout::MusicMidi)?;
    }
    parse_parameter_node(reader, &format!("{prefix}.node"), version)?;
    parse_children(reader, &format!("{prefix}.children"))?;
    reader.f64(format!("{prefix}.meter.grid_period"))?;
    reader.f64(format!("{prefix}.meter.grid_offset"))?;
    reader.f32(format!("{prefix}.meter.tempo"))?;
    reader.u8(format!("{prefix}.meter.beats_per_bar"))?;
    reader.u8(format!("{prefix}.meter.beat_value"))?;
    reader.packed_u8(
        format!("{prefix}.meter.override_parent"),
        if version.before(140) {
            PackedLayout::BooleanU8UniformHigh
        } else {
            PackedLayout::Boolean
        },
    )?;
    let count = reader.u32(format!("{prefix}.stingers.count"))? as usize;
    for index in 0..count {
        let item = format!("{prefix}.stingers.items[{index}]");
        reader.id(format!("{item}.trigger_id"))?;
        reader.id(format!("{item}.segment_id"))?;
        reader.enum_u32(
            format!("{item}.synchronize_at"),
            EnumerationKind::TimePoint,
            version,
        )?;
        reader.u32(format!("{item}.cue_filter"))?;
        reader.u32(format!("{item}.do_not_repeat_time"))?;
        reader.packed_u32(
            format!("{item}.allow_playing_in_next_segment"),
            PackedLayout::Boolean,
        )?;
    }
    Ok(())
}

fn parse_music_transitions(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    parse_music_common(reader, prefix, version)?;
    let count = reader.u32(format!("{prefix}.transitions.count"))? as usize;
    for index in 0..count {
        let item = format!("{prefix}.transitions.items[{index}]");
        if version.at_least(88) {
            reader.constant_u32(format!("{item}.source_count"), 1)?;
            reader.id(format!("{item}.source"))?;
        } else {
            reader.id(format!("{item}.source"))?;
        }
        if version.at_least(88) {
            reader.constant_u32(format!("{item}.destination_count"), 1)?;
            reader.id(format!("{item}.destination"))?;
        } else {
            reader.id(format!("{item}.destination"))?;
        }
        reader.u32(format!("{item}.source.fade.time"))?;
        reader.u32(format!("{item}.source.fade.curve"))?;
        reader.i32(format!("{item}.source.fade.offset"))?;
        reader.enum_u32(
            format!("{item}.source.exit_at"),
            EnumerationKind::TimePoint,
            version,
        )?;
        reader.u32(format!("{item}.source.cue_filter"))?;
        if version.at_least(140) {
            reader.packed_u8(
                format!("{item}.source.play_post_exit"),
                PackedLayout::Boolean,
            )?;
        } else {
            reader.packed_u8(
                format!("{item}.source.play_post_exit"),
                PackedLayout::BooleanU8IgnoredHigh,
            )?;
        }
        reader.u32(format!("{item}.destination.fade.time"))?;
        reader.u32(format!("{item}.destination.fade.curve"))?;
        reader.i32(format!("{item}.destination.fade.offset"))?;
        reader.u32(format!("{item}.destination.cue_filter"))?;
        reader.id(format!("{item}.u1"))?;
        if version.at_least(134) {
            reader.enum_u16(
                format!("{item}.destination.jump_type"),
                EnumerationKind::MusicJumpMode,
                version,
            )?;
        }
        reader.enum_u16(
            format!("{item}.destination.entry_type"),
            EnumerationKind::MusicSynchronizeMode,
            version,
        )?;
        if version.at_least(140) {
            reader.packed_u8(
                format!("{item}.destination.play_pre_entry"),
                PackedLayout::Boolean,
            )?;
        } else {
            reader.packed_u8(
                format!("{item}.destination.play_pre_entry"),
                PackedLayout::BooleanU8IgnoredHigh,
            )?;
        }
        reader.packed_u8(
            format!("{item}.destination.match_source_cue"),
            PackedLayout::Boolean,
        )?;
        let has_segment = reader.packed_u8(
            format!("{item}.transition_segment.enabled"),
            PackedLayout::Boolean,
        )? != 0;
        if version.before(88) || has_segment {
            reader.id(format!("{item}.transition_segment.segment_id"))?;
            for side in ["fade_in", "fade_out"] {
                reader.u32(format!("{item}.transition_segment.{side}.time"))?;
                reader.u32(format!("{item}.transition_segment.{side}.curve"))?;
                reader.i32(format!("{item}.transition_segment.{side}.offset"))?;
            }
            if version.at_least(140) {
                reader.packed_u8(
                    format!("{item}.transition_segment.play_pre_entry"),
                    PackedLayout::Boolean,
                )?;
                reader.packed_u8(
                    format!("{item}.transition_segment.play_post_exit"),
                    PackedLayout::Boolean,
                )?;
            } else {
                reader.packed_u8(
                    format!("{item}.transition_segment.play_pre_entry"),
                    PackedLayout::BooleanU8IgnoredHigh,
                )?;
                reader.packed_u8(
                    format!("{item}.transition_segment.play_post_exit"),
                    PackedLayout::BooleanU8IgnoredHigh,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn parse_music_segment(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
) -> Result<()> {
    parse_music_common(reader, "music", version)?;
    reader.f64("duration")?;
    let count = reader.u32("cues.count")? as usize;
    for index in 0..count {
        let item = format!("cues.items[{index}]");
        reader.id(format!("{item}.id"))?;
        reader.f64(format!("{item}.position"))?;
        if version.before(140) {
            reader.constant_u32(format!("{item}.reserved"), 0)?;
        } else {
            reader.constant_u8(format!("{item}.reserved_tail"), 0)?;
        }
    }
    Ok(())
}

pub(super) fn parse_music_track(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    if version.at_least(112) {
        reader.packed_u8("midi_flags", PackedLayout::MusicMidi)?;
    }
    let sources = reader.u32("sources.count")? as usize;
    for index in 0..sources {
        parse_audio_source(reader, &format!("sources.items[{index}]"), version)?;
    }
    let clips = reader.u32("clips.count")? as usize;
    for index in 0..clips {
        let item = format!("clips.items[{index}]");
        reader.u32(format!("{item}.u1"))?;
        reader.id(format!("{item}.source_id"))?;
        if version.at_least(140) {
            reader.id(format!("{item}.event_id"))?;
        }
        reader.f64(format!("{item}.play_at"))?;
        reader.f64(format!("{item}.begin_trim"))?;
        reader.f64(format!("{item}.end_trim"))?;
        reader.f64(format!("{item}.source_duration"))?;
    }
    if clips != 0 {
        reader.u32("clips.u1")?;
    }
    let automation = reader.u32("clip_automation.count")? as usize;
    for index in 0..automation {
        let item = format!("clip_automation.items[{index}]");
        reader.u32(format!("{item}.clip_index"))?;
        reader.enum_u32(
            format!("{item}.type"),
            EnumerationKind::MusicTrackClipCurveType,
            version,
        )?;
        let points = reader.u32(format!("{item}.point_count"))? as usize;
        parse_graph_points(reader, &format!("{item}.points"), points, version)?;
    }
    parse_parameter_node(reader, "node", version)?;
    let track_type = if version.before(112) {
        reader.enum_u32("track_type", EnumerationKind::TrackType, version)?
    } else {
        reader.enum_u8("track_type", EnumerationKind::TrackType, version)? as u32
    };
    if version.at_least(112) && track_type == 3 {
        reader.constant_u32("switch.transition.switcher_count", 1)?;
        reader.id("switch.transition.switcher_id")?;
        reader.u32("switch.transition.source.time")?;
        reader.u32("switch.transition.source.curve")?;
        reader.i32("switch.transition.source.offset")?;
        reader.enum_u32(
            "switch.transition.source.exit_at",
            EnumerationKind::TimePoint,
            version,
        )?;
        reader.id("switch.transition.source.cue_filter")?;
        reader.u32("switch.transition.destination.time")?;
        reader.u32("switch.transition.destination.curve")?;
        reader.i32("switch.transition.destination.offset")?;
    }
    reader.u16("look_ahead_time")?;
    reader.constant_u16("look_ahead_reserved", 0)?;
    Ok(())
}

fn parse_playlist_item(
    reader: &mut FieldReader<'_>,
    index: usize,
    version: BankVersion,
) -> Result<()> {
    let item = format!("playlist.items[{index}]");
    reader.id(format!("{item}.item"))?;
    reader.id(format!("{item}.u1"))?;
    reader.u32(format!("{item}.child_count"))?;
    reader.packed_u32(
        format!("{item}.play_type"),
        PackedLayout::MusicPlaylistPlayType,
    )?;
    reader.u16(format!("{item}.loop_count"))?;
    if version.at_least(112) {
        reader.constant_u32(format!("{item}.loop_reserved"), 0)?;
    }
    reader.u32(format!("{item}.weight"))?;
    reader.u16(format!("{item}.avoid_repeat_count"))?;
    reader.packed_u8(format!("{item}.group"), PackedLayout::Boolean)?;
    reader.enum_u8(
        format!("{item}.random_type"),
        EnumerationKind::AudioPlayRandomType,
        version,
    )?;
    Ok(())
}

pub(super) fn parse_music_playlist(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
) -> Result<()> {
    parse_music_transitions(reader, "music", version)?;
    let total = reader.u32("playlist.item_count")? as usize;
    for index in 0..total {
        parse_playlist_item(reader, index, version)?;
    }
    Ok(())
}

pub(super) fn parse_music_switch(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    parse_music_transitions(reader, "music", version)?;
    if version.before(88) {
        reader.packed_u32("switcher.group_type", PackedLayout::Boolean)?;
        reader.id("switcher.group_id")?;
        reader.id("switcher.default_value")?;
        reader.packed_u8("continue_playback", PackedLayout::Boolean)?;
        let count = reader.u32("switcher.association_count")? as usize;
        for index in 0..count {
            reader.id(format!("switcher.associations[{index}].value_id"))?;
            reader.id(format!("switcher.associations[{index}].node_id"))?;
        }
        return Ok(());
    }
    reader.packed_u8("continue_playback", PackedLayout::Boolean)?;
    let depth = reader.u32("association.depth")? as usize;
    for index in 0..depth {
        reader.id(format!("association.arguments[{index}].group_id"))?;
    }
    for index in 0..depth {
        reader.packed_u8(
            format!("association.arguments[{index}].is_state"),
            PackedLayout::Boolean,
        )?;
    }
    let size = reader.u32("association.path_byte_size")? as usize;
    reader.enum_u8(
        "association.mode",
        EnumerationKind::AssociationMode,
        version,
    )?;
    parse_association_paths(reader, "association.paths", size)
}
