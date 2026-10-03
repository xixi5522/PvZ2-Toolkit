//! Shared audio-node, positioning, bus, attenuation, and plug-in layouts.

use crate::error::Result;
use crate::hierarchy::{FieldReader, parse_common_properties, parse_graph_points};
use crate::semantics::{CommonPropertyDomain, EnumerationKind, PackedLayout};
use crate::version::BankVersion;

pub(super) fn parse_rtpc(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    let count = reader.u16(format!("{prefix}.count"))? as usize;
    for index in 0..count {
        let item = format!("{prefix}.items[{index}]");
        reader.id(format!("{item}.parameter_id"))?;
        if version.at_least(112) {
            reader.enum_u8(
                format!("{item}.parameter_category"),
                EnumerationKind::ParameterCategory,
                version,
            )?;
            reader.enum_u8(
                format!("{item}.u1"),
                EnumerationKind::PropertyCategory,
                version,
            )?;
            reader.u8(format!("{item}.property_type"))?;
        } else {
            reader.u32(format!("{item}.property_type"))?;
        }
        reader.id(format!("{item}.u2"))?;
        reader.enum_u8(
            format!("{item}.coordinate_mode"),
            EnumerationKind::CoordinateMode,
            version,
        )?;
        let points = reader.u16(format!("{item}.point_count"))? as usize;
        parse_graph_points(reader, &format!("{item}.points"), points, version)?;
    }
    Ok(())
}

fn parse_state(reader: &mut FieldReader<'_>, prefix: &str, version: BankVersion) -> Result<()> {
    if version.at_least(125) {
        let count = reader.u8(format!("{prefix}.attribute_count"))? as usize;
        for index in 0..count {
            let item = format!("{prefix}.attributes[{index}]");
            reader.u8(format!("{item}.property_type"))?;
            reader.enum_u8(
                format!("{item}.property_category"),
                EnumerationKind::PropertyCategory,
                version,
            )?;
            if version.at_least(128) {
                reader.u8(format!("{item}.u1"))?;
            }
        }
    }
    let count = if version.before(125) {
        reader.u32(format!("{prefix}.group_count"))? as usize
    } else {
        reader.u8(format!("{prefix}.group_count"))? as usize
    };
    for group_index in 0..count {
        let group = format!("{prefix}.groups[{group_index}]");
        reader.id(format!("{group}.group_id"))?;
        reader.enum_u8(
            format!("{group}.change_occurs_at"),
            EnumerationKind::StateChangeOccursAt,
            version,
        )?;
        let applies = if version.before(125) {
            reader.u16(format!("{group}.apply_count"))? as usize
        } else {
            reader.u8(format!("{group}.apply_count"))? as usize
        };
        for apply_index in 0..applies {
            let apply = format!("{group}.applies[{apply_index}]");
            reader.id(format!("{apply}.target_id"))?;
            reader.id(format!("{apply}.setting_id"))?;
        }
    }
    Ok(())
}

fn parse_effects(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
    with_override: bool,
) -> Result<()> {
    if with_override {
        reader.packed_u8(format!("{prefix}.override_parent"), PackedLayout::Boolean)?;
    }
    let count = reader.u8(format!("{prefix}.count"))? as usize;
    if count != 0 {
        reader.packed_u8(
            format!("{prefix}.bypass_mask"),
            if version.before(150) {
                PackedLayout::EffectBypass
            } else {
                PackedLayout::Boolean
            },
        )?;
    }
    for index in 0..count {
        let item = format!("{prefix}.slots[{index}]");
        reader.u8(format!("{item}.index"))?;
        reader.id(format!("{item}.effect_id"))?;
        if version.before(150) {
            reader.packed_u8(format!("{item}.use_share_set"), PackedLayout::Boolean)?;
            reader.packed_u8(format!("{item}.u1"), PackedLayout::Boolean)?;
        } else {
            reader.packed_u8(format!("{item}.flags"), PackedLayout::EffectSlot150)?;
        }
    }
    Ok(())
}

fn parse_metadata(reader: &mut FieldReader<'_>, prefix: &str, with_override: bool) -> Result<()> {
    if with_override {
        reader.packed_u8(format!("{prefix}.override_parent"), PackedLayout::Boolean)?;
    }
    let count = reader.u8(format!("{prefix}.count"))? as usize;
    for index in 0..count {
        let item = format!("{prefix}.slots[{index}]");
        reader.u8(format!("{item}.index"))?;
        reader.id(format!("{item}.metadata_id"))?;
        reader.packed_u8(format!("{item}.use_share_set"), PackedLayout::Boolean)?;
    }
    Ok(())
}

fn parse_automation(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
    legacy_2d: bool,
) -> Result<()> {
    reader.packed_u8(
        format!("{prefix}.path_mode"),
        PackedLayout::PositionAutomation,
    )?;
    if legacy_2d {
        reader.constant_u8(format!("{prefix}.reserved[0]"), 0)?;
        reader.constant_u8(format!("{prefix}.reserved[1]"), 0)?;
        reader.constant_u8(format!("{prefix}.reserved[2]"), 0)?;
        reader.packed_u8(format!("{prefix}.loop"), PackedLayout::Boolean)?;
    }
    reader.u32(format!("{prefix}.transition_time"))?;
    if legacy_2d {
        reader.packed_u8(
            format!("{prefix}.hold_listener_orientation"),
            PackedLayout::Boolean,
        )?;
    }
    let vertices = reader.u32(format!("{prefix}.vertex_count"))? as usize;
    for index in 0..vertices {
        let item = format!("{prefix}.vertices[{index}]");
        reader.f32(format!("{item}.x"))?;
        if legacy_2d {
            reader.constant_u32(format!("{item}.reserved"), 0)?;
            reader.f32(format!("{item}.y"))?;
        } else {
            reader.f32(format!("{item}.z"))?;
            reader.f32(format!("{item}.y"))?;
        }
        reader.u32(format!("{item}.duration"))?;
    }
    let paths = reader.u32(format!("{prefix}.path_count"))? as usize;
    for index in 0..paths {
        let item = format!("{prefix}.paths[{index}]");
        reader.u32(format!("{item}.vertex_begin"))?;
        reader.u32(format!("{item}.vertex_count"))?;
    }
    for index in 0..paths {
        let item = format!("{prefix}.paths[{index}].random_range");
        reader.f32(format!("{item}.left_right"))?;
        reader.f32(format!("{item}.front_back"))?;
        if version.at_least(112) {
            reader.f32(format!("{item}.up_down"))?;
        }
    }
    Ok(())
}

fn parse_positioning(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<bool> {
    if version.before(112) {
        let overridden =
            reader.packed_u8(format!("{prefix}.override_parent"), PackedLayout::Boolean)? != 0;
        if !overridden {
            return Ok(false);
        }
        let discriminator = if version.at_least(88) {
            reader.packed_u8(format!("{prefix}.available_2d"), PackedLayout::Boolean)?
        } else {
            0
        };
        let position_type = reader.enum_u8(
            format!("{prefix}.type"),
            EnumerationKind::PositioningType,
            version,
        )?;
        let mode_flags = reader.packed_u8(
            format!("{prefix}.mode_flags"),
            if version.before(88) {
                PackedLayout::PositioningMode72
            } else {
                PackedLayout::PositioningMode88
            },
        )?;
        if position_type == 0 {
            reader.require(
                if version.before(88) {
                    mode_flags & 0x02 == 0
                } else {
                    discriminator != 0
                },
                "legacy positioning discriminator",
                "Twinning requires the 2D discriminator to match the positioning type",
            )?;
            return Ok(true);
        }
        reader.require(
            if version.before(88) {
                mode_flags & 0x02 != 0
            } else {
                discriminator == 0
            },
            "legacy positioning discriminator",
            "Twinning requires the 3D discriminator to match the positioning type",
        )?;
        reader.constant_u8(format!("{prefix}.reserved[0]"), 0)?;
        reader.constant_u8(format!("{prefix}.reserved[1]"), 0)?;
        reader.constant_u8(format!("{prefix}.reserved[2]"), 0)?;
        reader.id(format!("{prefix}.attenuation_id"))?;
        reader.packed_u8(format!("{prefix}.spatialization"), PackedLayout::Boolean)?;
        let game_defined = mode_flags & 1 != 0;
        if game_defined {
            reader.packed_u8(
                format!("{prefix}.update_at_each_frame"),
                PackedLayout::Boolean,
            )?;
        } else {
            parse_automation(reader, &format!("{prefix}.automation"), version, true)?;
        }
        return Ok(true);
    }

    let positioning_layout = if version.before(125) {
        PackedLayout::Positioning112
    } else if version.before(132) {
        PackedLayout::Positioning125
    } else if version.before(140) {
        PackedLayout::Positioning132
    } else {
        PackedLayout::Positioning140
    };
    let flags = reader.packed_u8(format!("{prefix}.flags"), positioning_layout)?;
    if version.before(132) {
        let is_three_dimensional = if version.before(125) {
            flags & 0x08 != 0
        } else {
            flags & 0x10 != 0
        };
        if !is_three_dimensional {
            return Ok(flags & 1 != 0);
        }
        let listener_layout = if version.before(125) {
            PackedLayout::ListenerRouting112
        } else if version.before(128) {
            PackedLayout::ListenerRouting125
        } else {
            PackedLayout::ListenerRouting128
        };
        let flags_3d =
            reader.packed_u8(format!("{prefix}.listener_routing_flags"), listener_layout)?;
        reader.id(format!("{prefix}.attenuation_id"))?;
        let source_mode = if version.before(125) {
            flags_3d & 0x03
        } else if version.before(128) {
            (flags_3d >> 4) & 0x03
        } else {
            (flags_3d >> 5) & 0x03
        };
        if source_mode != 0 {
            return Ok(flags & 1 != 0);
        }
        parse_automation(reader, &format!("{prefix}.automation"), version, false)?;
        return Ok(flags & 1 != 0);
    }

    let listener_relative = flags & 0x02 != 0;
    if !listener_relative {
        return Ok(flags & 1 != 0);
    }
    let listener_layout = if version.before(134) {
        PackedLayout::ListenerRouting132
    } else if version.before(140) {
        PackedLayout::ListenerRouting134
    } else {
        PackedLayout::ListenerRouting140
    };
    reader.packed_u8(format!("{prefix}.listener_routing_flags"), listener_layout)?;
    let source_mode = if version.before(140) {
        (flags >> 4) & 0x03
    } else {
        (flags >> 5) & 0x03
    };
    if source_mode == 0 {
        return Ok(flags & 1 != 0);
    }
    parse_automation(reader, &format!("{prefix}.automation"), version, false)?;
    Ok(flags & 1 != 0)
}

fn parse_auxiliary_sends(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<[bool; 3]> {
    let (user_enabled, overrides) = if version.before(112) {
        let game_override =
            reader.packed_u8(format!("{prefix}.game_override"), PackedLayout::Boolean)? != 0;
        reader.packed_u8(format!("{prefix}.game_enabled"), PackedLayout::Boolean)?;
        let user_override =
            reader.packed_u8(format!("{prefix}.user_override"), PackedLayout::Boolean)? != 0;
        (
            reader.packed_u8(format!("{prefix}.user_enabled"), PackedLayout::Boolean)? != 0,
            [game_override, user_override, false],
        )
    } else {
        let layout = if version.before(135) {
            PackedLayout::AuxiliarySends112
        } else {
            PackedLayout::AuxiliarySends135
        };
        let flags = reader.packed_u8(format!("{prefix}.flags"), layout)?;
        (
            flags & 0x08 != 0,
            [flags & 0x01 != 0, flags & 0x04 != 0, flags & 0x10 != 0],
        )
    };
    if user_enabled {
        for index in 0..4 {
            reader.id(format!("{prefix}.user_buses[{index}]"))?;
        }
    }
    if version.at_least(135) {
        reader.id(format!("{prefix}.early_reflection_bus"))?;
    }
    Ok(overrides)
}

fn parse_object_playback(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    if version.before(112) {
        reader.enum_u8(
            format!("{prefix}.virtual_voice.on_return"),
            EnumerationKind::VirtualVoiceOnReturn,
            version,
        )?;
        reader.enum_u8(
            format!("{prefix}.limit.priority_equal"),
            EnumerationKind::PlaybackLimitPriority,
            version,
        )?;
        reader.enum_u8(
            format!("{prefix}.limit.reached_behavior"),
            EnumerationKind::PlaybackLimitReached,
            version,
        )?;
        reader.u16(format!("{prefix}.limit.maximum_instances"))?;
        reader.enum_u8(
            format!("{prefix}.limit.scope"),
            EnumerationKind::PlaybackLimitScope,
            version,
        )?;
        reader.enum_u8(
            format!("{prefix}.virtual_voice.behavior"),
            EnumerationKind::VirtualVoiceBehavior,
            version,
        )?;
        reader.packed_u8(
            format!("{prefix}.limit.override_parent"),
            PackedLayout::Boolean,
        )?;
        reader.packed_u8(
            format!("{prefix}.virtual_voice.override_parent"),
            PackedLayout::Boolean,
        )?;
    } else {
        reader.packed_u8(
            format!("{prefix}.limit.flags"),
            PackedLayout::PlaybackObject,
        )?;
        reader.enum_u8(
            format!("{prefix}.virtual_voice.on_return"),
            EnumerationKind::VirtualVoiceOnReturn,
            version,
        )?;
        reader.u16(format!("{prefix}.limit.maximum_instances"))?;
        reader.enum_u8(
            format!("{prefix}.virtual_voice.behavior"),
            EnumerationKind::VirtualVoiceBehavior,
            version,
        )?;
    }
    Ok(())
}

fn parse_volume_hdr_flags(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    if version.before(88) {
        return Ok(());
    }
    if version.before(112) {
        reader.packed_u8(format!("{prefix}.hdr_override"), PackedLayout::Boolean)?;
        reader.packed_u8(
            format!("{prefix}.normalization_override"),
            PackedLayout::Boolean,
        )?;
        reader.packed_u8(
            format!("{prefix}.normalization_enabled"),
            PackedLayout::Boolean,
        )?;
        reader.packed_u8(format!("{prefix}.hdr_enabled"), PackedLayout::Boolean)?;
    } else {
        reader.packed_u8(format!("{prefix}.flags"), PackedLayout::Hdr)?;
    }
    Ok(())
}

pub(super) fn parse_parameter_node(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    parse_effects(reader, &format!("{prefix}.effects"), version, true)?;
    if version.at_least(140) {
        parse_metadata(reader, &format!("{prefix}.metadata"), true)?;
    }
    if version.at_least(112) && version.before(150) {
        reader.packed_u8(
            format!("{prefix}.mixer.override_parent"),
            PackedLayout::Boolean,
        )?;
    }
    reader.id(format!("{prefix}.output_bus"))?;
    reader.id(format!("{prefix}.parent"))?;
    if version.before(112) {
        reader.packed_u8(
            format!("{prefix}.priority.override_parent"),
            PackedLayout::Boolean,
        )?;
        reader.packed_u8(
            format!("{prefix}.priority.use_distance_factor"),
            PackedLayout::Boolean,
        )?;
    } else {
        reader.packed_u8(
            format!("{prefix}.priority_and_midi_flags"),
            PackedLayout::PriorityAndMidi,
        )?;
    }
    parse_common_properties(
        reader,
        &format!("{prefix}.properties"),
        true,
        CommonPropertyDomain::Audio,
        version,
    )?;
    parse_positioning(reader, &format!("{prefix}.positioning"), version)?;
    if version.at_least(72) {
        parse_auxiliary_sends(reader, &format!("{prefix}.auxiliary_sends"), version)?;
    }
    parse_object_playback(reader, &format!("{prefix}.playback"), version)?;
    parse_volume_hdr_flags(reader, &format!("{prefix}.volume_hdr"), version)?;
    parse_state(reader, &format!("{prefix}.state"), version)?;
    parse_rtpc(reader, &format!("{prefix}.rtpc"), version)?;
    Ok(())
}

pub(super) fn parse_bus(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    parse_common_properties(
        reader,
        "properties",
        false,
        CommonPropertyDomain::Audio,
        version,
    )?;
    if version.at_least(88) && version.before(112) {
        reader.packed_u8("positioning.override_parent", PackedLayout::Boolean)?;
        reader.packed_u8("positioning.speaker_panning", PackedLayout::Boolean)?;
    } else if version.at_least(112) && version.before(125) {
        reader.packed_u8("positioning.flags", PackedLayout::BusPositioning112)?;
    } else if version.at_least(125) {
        let positioning_override = parse_positioning(reader, "positioning", version)?;
        reader.require(
            positioning_override,
            "audio bus positioning override",
            "Twinning requires the audio-bus positioning override to be enabled",
        )?;
    }
    if version.at_least(125) {
        let overrides = parse_auxiliary_sends(reader, "auxiliary_sends", version)?;
        let required = overrides[0] && overrides[1] && (version.before(135) || overrides[2]);
        reader.require(
            required,
            "audio bus auxiliary-send overrides",
            if version.before(135) {
                "Twinning requires the game-defined and user-defined overrides"
            } else {
                "Twinning requires the game-defined, user-defined, and early-reflection overrides"
            },
        )?;
    }
    if version.before(112) {
        reader.enum_u8(
            "playback.priority_equal",
            EnumerationKind::PlaybackLimitPriority,
            version,
        )?;
        reader.enum_u8(
            "playback.limit_behavior",
            EnumerationKind::PlaybackLimitReached,
            version,
        )?;
        reader.u16("playback.maximum_instances")?;
        reader.packed_u8("playback.override_parent", PackedLayout::Boolean)?;
    } else {
        reader.packed_u8("playback.flags", PackedLayout::PlaybackBus)?;
        reader.u16("playback.maximum_instances")?;
    }
    if version.at_least(88) {
        reader.u32("bus_configuration.u1")?;
        if version.before(112) {
            reader.packed_u8("hdr.enabled", PackedLayout::Boolean)?;
            reader.enum_u8(
                "hdr.release_mode",
                EnumerationKind::BusHdrReleaseMode,
                version,
            )?;
        } else {
            reader.packed_u8("hdr.flags", PackedLayout::BusHdr)?;
        }
    } else {
        reader.constant_u32("bus_configuration_constant", 63)?;
    }
    reader.u32("automatic_ducking.recovery_time")?;
    reader.f32("automatic_ducking.maximum_volume")?;
    let count = reader.u32("automatic_ducking.count")? as usize;
    for index in 0..count {
        let item = format!("automatic_ducking.items[{index}]");
        reader.id(format!("{item}.bus_id"))?;
        reader.f32(format!("{item}.volume"))?;
        reader.u32(format!("{item}.fade_out_time"))?;
        reader.u32(format!("{item}.fade_in_time"))?;
        reader.enum_u8(format!("{item}.curve"), EnumerationKind::FadeCurve, version)?;
        reader.enum_u8(
            format!("{item}.target_property"),
            EnumerationKind::BusDuckingTarget,
            version,
        )?;
    }
    parse_effects(reader, "effects", version, false)?;
    if version.at_least(112) && version.before(150) {
        reader.id("mixer.id")?;
        reader.constant_u16("mixer.reserved", 0)?;
    }
    if version.at_least(140) {
        parse_metadata(reader, "metadata", false)?;
    }
    parse_rtpc(reader, "rtpc", version)?;
    parse_state(reader, "state", version)
}

pub(super) fn parse_attenuation(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    if version.at_least(140) {
        reader.packed_u8("height_spread", PackedLayout::Boolean)?;
    }
    let cone_enabled = reader.packed_u8("cone.enabled", PackedLayout::Boolean)? != 0;
    if cone_enabled {
        reader.f32("cone.inside_degrees")?;
        reader.f32("cone.outside_degrees")?;
        reader.f32("cone.outside_volume")?;
        reader.f32("cone.low_pass_filter")?;
        if version.at_least(112) {
            reader.f32("cone.high_pass_filter")?;
        }
    }
    let slot_count = if version.before(88) {
        4
    } else if version.before(112) {
        5
    } else if version.before(145) {
        7
    } else {
        19
    };
    for index in 0..slot_count {
        reader.u8(format!("curve_slots[{index}]"))?;
    }
    let count = reader.u8("curves.count")? as usize;
    for index in 0..count {
        let item = format!("curves.items[{index}]");
        reader.enum_u8(
            format!("{item}.scaling"),
            EnumerationKind::CoordinateMode,
            version,
        )?;
        let points = reader.u16(format!("{item}.point_count"))? as usize;
        parse_graph_points(reader, &format!("{item}.points"), points, version)?;
    }
    parse_rtpc(reader, "rtpc", version)
}

pub(super) fn parse_modulator(reader: &mut FieldReader<'_>, version: BankVersion) -> Result<()> {
    parse_common_properties(
        reader,
        "properties",
        true,
        CommonPropertyDomain::Modulator,
        version,
    )?;
    parse_rtpc(reader, "rtpc", version)
}

pub(super) fn parse_plugin_settings(
    reader: &mut FieldReader<'_>,
    version: BankVersion,
    audio_device: bool,
) -> Result<()> {
    reader.constant_u8("reserved", 0)?;
    parse_rtpc(reader, "rtpc", version)?;
    if version.at_least(125) && version.before(128) {
        reader.constant_u16("state.reserved", 0)?;
    }
    if version.at_least(128) {
        parse_state(reader, "state", version)?;
    }
    if version.at_least(112) {
        let value_count = reader.u16("u1.count")? as usize;
        for index in 0..value_count {
            let item = format!("u1.items[{index}]");
            reader.u8(format!("{item}.type"))?;
            if version.at_least(128) {
                reader.enum_u8(
                    format!("{item}.mode"),
                    EnumerationKind::CoordinateMode,
                    version,
                )?;
            }
            reader.f32(format!("{item}.value"))?;
        }
    }
    if audio_device && version.at_least(140) {
        parse_effects(reader, "effects", version, false)?;
    }
    Ok(())
}

pub(super) fn parse_audio_source(
    reader: &mut FieldReader<'_>,
    prefix: &str,
    version: BankVersion,
) -> Result<()> {
    let plugin = reader.id(format!("{prefix}.plugin_id"))?;
    let source_type = if version.before(112) {
        reader.enum_u32(
            format!("{prefix}.source_type"),
            EnumerationKind::AudioSourceType,
            version,
        )?
    } else {
        reader.enum_u8(
            format!("{prefix}.source_type"),
            EnumerationKind::AudioSourceType,
            version,
        )? as u32
    };
    reader.id(format!("{prefix}.resource_id"))?;
    if version.before(113) {
        reader.id(format!("{prefix}.source_id"))?;
        let streamed = if version.before(112) {
            source_type == 1
        } else {
            source_type == 2
        };
        if !streamed {
            reader.u32(format!("{prefix}.resource_offset"))?;
        }
        if version.at_least(112) || !streamed {
            reader.u32(format!("{prefix}.resource_size"))?;
        }
    } else {
        reader.u32(format!("{prefix}.resource_size"))?;
    }
    reader.packed_u8(
        format!("{prefix}.flags"),
        if version.before(112) {
            PackedLayout::Boolean
        } else {
            PackedLayout::Source
        },
    )?;
    if plugin & 0xffff >= 2 {
        reader.constant_u32(format!("{prefix}.plugin_reserved"), 0)?;
    }
    Ok(())
}
