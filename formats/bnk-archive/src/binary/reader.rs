//! Container and chunk decoding.

use std::io::Read;

use byteorder::{ByteOrder, LittleEndian};

use crate::error::{BnkError, Result};
use crate::hierarchy::{decode_dialogue_fields, decode_event_action_fields, decode_fields};
use crate::limits::{DecodeLimits, DecodeOptions, ValidationMode};
use crate::semantics::{EnumerationKind, PackedLayout};
use crate::types::*;
use crate::version::BankVersion;

struct SliceReader<'a> {
    bytes: &'a [u8],
    position: usize,
    base_offset: usize,
}

impl<'a> SliceReader<'a> {
    fn new(bytes: &'a [u8], base_offset: usize) -> Self {
        Self {
            bytes,
            position: 0,
            base_offset,
        }
    }

    fn offset(&self) -> usize {
        self.base_offset + self.position
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    fn is_empty(&self) -> bool {
        self.position == self.bytes.len()
    }

    fn take(&mut self, length: usize, context: &'static str) -> Result<&'a [u8]> {
        if length > self.remaining() {
            return Err(BnkError::Truncated {
                context,
                offset: self.offset() as u64,
                needed: length,
                remaining: self.remaining(),
            });
        }
        let start = self.position;
        self.position += length;
        Ok(&self.bytes[start..self.position])
    }

    fn u8(&mut self, context: &'static str) -> Result<u8> {
        Ok(self.take(1, context)?[0])
    }

    fn u16(&mut self, context: &'static str) -> Result<u16> {
        Ok(LittleEndian::read_u16(self.take(2, context)?))
    }

    fn u32(&mut self, context: &'static str) -> Result<u32> {
        Ok(LittleEndian::read_u32(self.take(4, context)?))
    }

    fn f32(&mut self, context: &'static str) -> Result<f32> {
        Ok(f32::from_bits(self.u32(context)?))
    }

    fn four_cc(&mut self, context: &'static str) -> Result<[u8; 4]> {
        let bytes = self.take(4, context)?;
        Ok(bytes.try_into().expect("four-byte slice"))
    }

    fn count(
        &mut self,
        context: &'static str,
        width: CountWidth,
        limits: DecodeLimits,
    ) -> Result<usize> {
        let count = match width {
            CountWidth::U8 => self.u8(context)? as usize,
            CountWidth::U16 => self.u16(context)? as usize,
            CountWidth::U32 => self.u32(context)? as usize,
        };
        check_limit("entry count", count as u64, limits.max_entries as u64)?;
        Ok(count)
    }

    fn length_prefixed_string(
        &mut self,
        width: CountWidth,
        nul_terminated: bool,
        context: &'static str,
        limits: DecodeLimits,
    ) -> Result<String> {
        let length = self.count(context, width, limits)?;
        check_limit(
            "string bytes",
            length as u64,
            limits.max_string_bytes as u64,
        )?;
        let bytes = self.take(length, context)?;
        let bytes = if nul_terminated {
            match bytes.split_last() {
                Some((&0, content)) => content,
                _ => {
                    return Err(BnkError::invalid(
                        context,
                        self.offset().saturating_sub(length),
                        "length-prefixed string is not NUL terminated",
                    ));
                }
            }
        } else {
            bytes
        };
        string_from_bytes(bytes, context, self.offset().saturating_sub(length))
    }

    fn c_string(&mut self, context: &'static str, limits: DecodeLimits) -> Result<String> {
        let tail = &self.bytes[self.position..];
        let Some(length) = tail.iter().position(|byte| *byte == 0) else {
            return Err(BnkError::invalid(
                context,
                self.offset(),
                "missing NUL terminator",
            ));
        };
        check_limit(
            "string bytes",
            length as u64,
            limits.max_string_bytes as u64,
        )?;
        let bytes = self.take(length, context)?;
        let value = string_from_bytes(bytes, context, self.offset().saturating_sub(length))?;
        self.take(1, context)?;
        Ok(value)
    }
}

#[derive(Clone, Copy)]
enum CountWidth {
    U8,
    U16,
    U32,
}

fn string_from_bytes(bytes: &[u8], context: &'static str, offset: usize) -> Result<String> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| BnkError::invalid(context, offset, format!("invalid UTF-8: {error}")))
}

fn check_limit(resource: &'static str, requested: u64, limit: u64) -> Result<()> {
    if requested > limit {
        Err(BnkError::LimitExceeded {
            resource,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}

pub(super) fn check_count_fit(count: usize, maximum: usize, context: &'static str) -> Result<()> {
    if count > maximum {
        Err(BnkError::LimitExceeded {
            resource: context,
            requested: count as u64,
            limit: maximum as u64,
        })
    } else {
        Ok(())
    }
}

pub fn from_bytes_with_options(bytes: &[u8], options: DecodeOptions) -> Result<SoundBank> {
    check_limit(
        "file bytes",
        bytes.len() as u64,
        options.limits.max_file_bytes,
    )?;
    let mut reader = SliceReader::new(bytes, 0);

    let magic = reader.four_cc("BKHD chunk identifier")?;
    if magic != ChunkId::BKHD.0 {
        return Err(BnkError::InvalidMagic {
            offset: 0,
            found: magic,
        });
    }
    let header_size = reader.u32("BKHD chunk size")? as usize;
    check_limit(
        "chunk bytes",
        header_size as u64,
        options.limits.max_chunk_bytes,
    )?;
    let header_offset = reader.offset();
    let header_bytes = reader.take(header_size, "BKHD chunk")?;
    let header = parse_header(header_bytes, header_offset)?;
    if options.validation.is_strict() {
        header.version.ensure_supported()?;
    }

    let mut chunks = Vec::new();
    while !reader.is_empty() {
        check_limit(
            "chunk count",
            (chunks.len() + 1) as u64,
            options.limits.max_chunks as u64,
        )?;
        let id = ChunkId(reader.four_cc("chunk identifier")?);
        let size = reader.u32("chunk size")? as usize;
        check_limit("chunk bytes", size as u64, options.limits.max_chunk_bytes)?;
        let offset = reader.offset();
        let data = reader.take(size, "chunk body")?;
        chunks.push(parse_chunk(id, data, offset, header.version, options)?);
    }

    if options.validation.is_strict() {
        validate_media_pairs(&chunks)?;
    }
    if options.validation.is_twinning_compatible() {
        validate_twinning_chunks(header.version, &chunks)?;
    }
    Ok(SoundBank { header, chunks })
}

pub fn from_reader_with_options(
    mut reader: impl Read,
    options: DecodeOptions,
) -> Result<SoundBank> {
    let maximum =
        options
            .limits
            .max_file_bytes
            .checked_add(1)
            .ok_or(BnkError::IntegerOverflow {
                context: "reader limit",
            })?;
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(maximum)
        .read_to_end(&mut bytes)
        .map_err(|source| BnkError::io(bytes.len(), source))?;
    check_limit(
        "file bytes",
        bytes.len() as u64,
        options.limits.max_file_bytes,
    )?;
    from_bytes_with_options(&bytes, options)
}

pub(crate) fn parse_header(bytes: &[u8], offset: usize) -> Result<BankHeader> {
    let mut reader = SliceReader::new(bytes, offset);
    if bytes.len() < 12 {
        return Err(BnkError::Truncated {
            context: "BKHD fields",
            offset: offset as u64,
            needed: 12,
            remaining: bytes.len(),
        });
    }
    let version = BankVersion::new_unchecked(reader.u32("BKHD version")?);
    let id = reader.u32("BKHD bank identifier")?;
    let language = reader.u32("BKHD language")?;
    let header_expand = reader
        .take(reader.remaining(), "BKHD header expand")?
        .to_vec();
    Ok(BankHeader {
        version,
        id,
        language,
        header_expand,
    })
}

pub(crate) fn parse_chunk(
    id: ChunkId,
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<BankChunk> {
    if !version.is_supported() && !matches!(id, ChunkId::DIDX | ChunkId::DATA) {
        return Ok(BankChunk::Unknown(RawChunk {
            id,
            data: bytes.to_vec(),
        }));
    }

    let parsed = match id {
        ChunkId::DIDX => parse_media_index(bytes, offset, options).map(BankChunk::MediaIndex),
        ChunkId::DATA => Ok(BankChunk::MediaData(bytes.to_vec())),
        ChunkId::INIT => parse_plugins(bytes, offset, version, options).map(BankChunk::Plugins),
        ChunkId::STMG => parse_game_synchronization(bytes, offset, version, options)
            .map(BankChunk::GameSynchronization),
        ChunkId::HIRC => parse_hierarchy(bytes, offset, version, options).map(BankChunk::Hierarchy),
        ChunkId::STID => parse_references(bytes, offset, options).map(BankChunk::References),
        ChunkId::ENVS => {
            parse_environments(bytes, offset, version, options).map(BankChunk::Environments)
        }
        ChunkId::PLAT => parse_platform(bytes, offset, version, options).map(BankChunk::Platform),
        _ => {
            return Ok(BankChunk::Unknown(RawChunk {
                id,
                data: bytes.to_vec(),
            }));
        }
    };

    match parsed {
        Ok(value) => Ok(value),
        Err(_) if options.validation.is_permissive() => Ok(BankChunk::Unknown(RawChunk {
            id,
            data: bytes.to_vec(),
        })),
        Err(error) => Err(error),
    }
}

fn ensure_empty(reader: &SliceReader<'_>, context: &'static str) -> Result<()> {
    if reader.is_empty() {
        Ok(())
    } else {
        Err(BnkError::invalid(
            context,
            reader.offset(),
            format!("{} trailing bytes", reader.remaining()),
        ))
    }
}

pub(super) fn ensure_enumeration(
    enumeration: EnumerationKind,
    version: BankVersion,
    value: u32,
    context: &'static str,
    offset: usize,
) -> Result<()> {
    if enumeration.variant(version.number(), value).is_some() {
        Ok(())
    } else {
        Err(BnkError::invalid(
            context,
            offset,
            format!(
                "unknown {enumeration:?} value {value} for Wwise {}",
                version.number()
            ),
        ))
    }
}

fn parse_media_index(
    bytes: &[u8],
    offset: usize,
    options: DecodeOptions,
) -> Result<Vec<MediaIndexEntry>> {
    if !bytes.len().is_multiple_of(12) {
        return Err(BnkError::invalid(
            "DIDX",
            offset,
            format!("chunk length {} is not divisible by 12", bytes.len()),
        ));
    }
    let count = bytes.len() / 12;
    check_limit(
        "DIDX entries",
        count as u64,
        options.limits.max_entries as u64,
    )?;
    let mut reader = SliceReader::new(bytes, offset);
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| BnkError::LimitExceeded {
            resource: "DIDX allocation",
            requested: count as u64,
            limit: options.limits.max_entries as u64,
        })?;
    for _ in 0..count {
        entries.push(MediaIndexEntry {
            id: reader.u32("DIDX media identifier")?,
            offset: reader.u32("DIDX media offset")?,
            size: reader.u32("DIDX media size")?,
        });
    }
    Ok(entries)
}

pub(crate) fn validate_media_pairs(chunks: &[BankChunk]) -> Result<()> {
    for (index, chunk) in chunks.iter().enumerate() {
        let BankChunk::MediaIndex(entries) = chunk else {
            continue;
        };
        let Some(BankChunk::MediaData(data)) = chunks.get(index + 1) else {
            return Err(BnkError::MissingChunk {
                chunk: ChunkId::DATA.0,
                context: "DIDX must be immediately followed by DATA",
            });
        };
        for entry in entries {
            if entry.id == 0 {
                if entry.offset != 1 || entry.size != 0 {
                    return Err(BnkError::invalid(
                        "DIDX reserved media entry",
                        0,
                        format!(
                            "identifier zero requires offset 1 and size 0, found offset {} and size {}",
                            entry.offset, entry.size
                        ),
                    ));
                }
                continue;
            }
            let begin = entry.offset as usize;
            let end = begin
                .checked_add(entry.size as usize)
                .ok_or(BnkError::IntegerOverflow {
                    context: "DIDX media range",
                })?;
            if end > data.len() {
                return Err(BnkError::MediaOutOfBounds {
                    id: entry.id,
                    offset: entry.offset,
                    size: entry.size,
                    data_len: data.len(),
                });
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_twinning_chunks(version: BankVersion, chunks: &[BankChunk]) -> Result<()> {
    if let Some(BankChunk::Unknown(raw)) = chunks
        .iter()
        .find(|chunk| matches!(chunk, BankChunk::Unknown(_)))
    {
        return Err(BnkError::invalid(
            "Twinning chunk sequence",
            0,
            format!("unknown chunk {:?} is not part of Twinning's model", raw.id),
        ));
    }
    crate::container::validate_twinning_chunk_ids(version, chunks.iter().map(BankChunk::id))
}

fn parse_plugins(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<Vec<PluginReference>> {
    if version.before(118) {
        return Err(BnkError::invalid(
            "INIT",
            offset,
            format!("INIT is not defined for version {}", version.number()),
        ));
    }
    let mut reader = SliceReader::new(bytes, offset);
    let count = reader.count("INIT plugin count", CountWidth::U32, options.limits)?;
    let mut plugins = Vec::with_capacity(count);
    for _ in 0..count {
        let id = reader.u32("INIT plugin identifier")?;
        let library = if version.before(140) {
            reader.length_prefixed_string(
                CountWidth::U32,
                true,
                "INIT plugin library",
                options.limits,
            )?
        } else {
            reader.c_string("INIT plugin library", options.limits)?
        };
        plugins.push(PluginReference { id, library });
    }
    ensure_empty(&reader, "INIT")?;
    Ok(plugins)
}

fn parse_game_synchronization(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<GameSynchronization> {
    let mut reader = SliceReader::new(bytes, offset);
    let voice_filter_behavior = if version.at_least(145) {
        let value_offset = reader.offset();
        let value = reader.u16("STMG voice filter behavior")?;
        if options.validation.is_strict() {
            ensure_enumeration(
                EnumerationKind::VoiceFilterBehavior,
                version,
                value.into(),
                "STMG voice filter behavior",
                value_offset,
            )?;
        }
        Some(VoiceFilterBehavior::from_raw(value))
    } else {
        None
    };
    let volume_threshold = reader.f32("STMG volume threshold")?;
    let maximum_voice_instances = reader.u16("STMG maximum voice instances")?;
    let compatibility_value = version
        .at_least(128)
        .then(|| reader.u16("STMG compatibility value"))
        .transpose()?;
    if options.validation.is_strict()
        && let Some(value) = compatibility_value
        && value != 50
    {
        return Err(BnkError::invalid(
            "STMG compatibility value",
            reader.offset().saturating_sub(2),
            format!("expected 50, found {value}"),
        ));
    }

    let state_count = reader.count("STMG state group count", CountWidth::U32, options.limits)?;
    let mut state_groups = Vec::with_capacity(state_count);
    for _ in 0..state_count {
        let id = reader.u32("STMG state group identifier")?;
        let default_transition_time = reader.u32("STMG default transition time")?;
        let transition_count = reader.count(
            "STMG custom transition count",
            CountWidth::U32,
            options.limits,
        )?;
        let mut custom_transitions = Vec::with_capacity(transition_count);
        for _ in 0..transition_count {
            custom_transitions.push(StateTransition {
                from: reader.u32("STMG transition source")?,
                to: reader.u32("STMG transition target")?,
                time: reader.u32("STMG transition time")?,
            });
        }
        state_groups.push(StateGroup {
            id,
            default_transition_time,
            custom_transitions,
        });
    }

    let switch_count = reader.count("STMG switch group count", CountWidth::U32, options.limits)?;
    let mut switch_groups = Vec::with_capacity(switch_count);
    for _ in 0..switch_count {
        let id = reader.u32("STMG switch group identifier")?;
        let parameter_id = reader.u32("STMG switch parameter identifier")?;
        let category = if version.at_least(112) {
            let value_offset = reader.offset();
            let value = reader.u8("STMG switch parameter category")?;
            if options.validation.is_strict() {
                ensure_enumeration(
                    EnumerationKind::ParameterCategory,
                    version,
                    value.into(),
                    "STMG switch parameter category",
                    value_offset,
                )?;
            }
            Some(ParameterCategory::from_raw(version, value))
        } else {
            None
        };
        let point_count =
            reader.count("STMG switch point count", CountWidth::U32, options.limits)?;
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            let x = reader.f32("STMG switch point x")?;
            let y = reader.u32("STMG switch point y")?;
            let curve_offset = reader.offset();
            let curve = reader.u32("STMG switch point curve")?;
            if options.validation.is_strict() {
                ensure_enumeration(
                    EnumerationKind::FadeCurve,
                    version,
                    curve,
                    "STMG switch point curve",
                    curve_offset,
                )?;
            }
            points.push(IdentifierGraphPoint {
                x,
                y,
                curve: Curve(curve),
            });
        }
        switch_groups.push(SwitchGroup {
            id,
            parameter: ParameterReference {
                id: parameter_id,
                category,
            },
            points,
        });
    }

    let parameter_count =
        reader.count("STMG game parameter count", CountWidth::U32, options.limits)?;
    let mut game_parameters = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        let id = reader.u32("STMG game parameter identifier")?;
        let default_value = reader.f32("STMG game parameter default")?;
        let interpolation = if version.at_least(112) {
            let mode_offset = reader.offset();
            let mode = reader.u32("STMG game parameter interpolation mode")?;
            if options.validation.is_strict() {
                ensure_enumeration(
                    EnumerationKind::GameParameterInterpolation,
                    version,
                    mode,
                    "STMG game parameter interpolation mode",
                    mode_offset,
                )?;
            }
            let attack = reader.f32("STMG game parameter interpolation attack")?;
            let release = reader.f32("STMG game parameter interpolation release")?;
            let binding_offset = reader.offset();
            let binding = reader.u8("STMG game parameter built-in binding")?;
            if options.validation.is_strict() {
                ensure_enumeration(
                    EnumerationKind::GameParameterBuiltIn,
                    version,
                    binding.into(),
                    "STMG game parameter built-in binding",
                    binding_offset,
                )?;
            }
            Some(GameParameterInterpolation {
                mode: InterpolationMode::from_raw(mode),
                attack,
                release,
                built_in_parameter: BuiltInParameter::from_raw(version, binding),
            })
        } else {
            None
        };
        game_parameters.push(GameParameter {
            id,
            default_value,
            interpolation,
        });
    }

    let mut u1 = Vec::new();
    if version.at_least(120) && version.before(125) {
        for context in ["STMG reserved u32 1", "STMG reserved u32 2"] {
            let value = reader.u32(context)?;
            if value != 0 {
                return Err(BnkError::invalid(
                    context,
                    reader.offset().saturating_sub(4),
                    format!("expected 0, found {value}"),
                ));
            }
        }
    } else if version.at_least(125) && version.before(140) {
        let value = reader.u32("STMG reserved u32")?;
        if value != 0 {
            return Err(BnkError::invalid(
                "STMG reserved u32",
                reader.offset().saturating_sub(4),
                format!("expected 0, found {value}"),
            ));
        }
    } else if version.at_least(140) {
        let count = reader.count("STMG u1 count", CountWidth::U32, options.limits)?;
        u1.reserve(count);
        for _ in 0..count {
            u1.push(GameSynchronizationU1 {
                identifier: reader.u32("STMG u1 identifier")?,
                u1: reader.f32("STMG u1 value 1")?,
                u2: reader.f32("STMG u1 value 2")?,
                u3: reader.f32("STMG u1 value 3")?,
                u4: reader.f32("STMG u1 value 4")?,
                u5: reader.f32("STMG u1 value 5")?,
                u6: reader.f32("STMG u1 value 6")?,
            });
        }
    }

    ensure_empty(&reader, "STMG")?;
    Ok(GameSynchronization {
        voice_filter_behavior,
        volume_threshold,
        maximum_voice_instances,
        compatibility_value,
        state_groups,
        switch_groups,
        game_parameters,
        u1,
    })
}

fn parse_environments(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<EnvironmentSettings> {
    let mut reader = SliceReader::new(bytes, offset);
    let obstruction = parse_environment_bundle(&mut reader, version, options)?;
    let occlusion = parse_environment_bundle(&mut reader, version, options)?;
    ensure_empty(&reader, "ENVS")?;
    Ok(EnvironmentSettings {
        obstruction,
        occlusion,
    })
}

fn parse_environment_bundle(
    reader: &mut SliceReader<'_>,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<EnvironmentBundle> {
    Ok(EnvironmentBundle {
        volume: parse_environment_curve(reader, version, options)?,
        low_pass_filter: parse_environment_curve(reader, version, options)?,
        high_pass_filter: if version.at_least(112) {
            Some(parse_environment_curve(reader, version, options)?)
        } else {
            None
        },
    })
}

fn parse_environment_curve(
    reader: &mut SliceReader<'_>,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<EnvironmentCurve> {
    let enabled = match reader.u8("ENVS curve enabled")? {
        0 => false,
        1 => true,
        value => {
            return Err(BnkError::invalid(
                "ENVS curve enabled",
                reader.offset().saturating_sub(1),
                format!("expected 0 or 1, found {value}"),
            ));
        }
    };
    let mode_offset = reader.offset();
    let mode_raw = reader.u8("ENVS coordinate mode")?;
    if options.validation.is_strict() {
        ensure_enumeration(
            EnumerationKind::CoordinateMode,
            version,
            mode_raw.into(),
            "ENVS coordinate mode",
            mode_offset,
        )?;
    }
    let mode = CoordinateMode::from_raw(mode_raw);
    let count = reader.count("ENVS point count", CountWidth::U16, options.limits)?;
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let x = reader.f32("ENVS point x")?;
        let y = reader.f32("ENVS point y")?;
        let curve_offset = reader.offset();
        let curve = reader.u32("ENVS point curve")?;
        if options.validation.is_strict() {
            ensure_enumeration(
                EnumerationKind::FadeCurve,
                version,
                curve,
                "ENVS point curve",
                curve_offset,
            )?;
        }
        points.push(GraphPoint {
            x,
            y,
            curve: Curve(curve),
        });
    }
    Ok(EnvironmentCurve {
        enabled,
        mode,
        points,
    })
}

fn parse_references(bytes: &[u8], offset: usize, options: DecodeOptions) -> Result<BankReferences> {
    let mut reader = SliceReader::new(bytes, offset);
    let marker = reader.u32("STID marker")?;
    if options.validation.is_strict() && marker != 1 {
        return Err(BnkError::invalid(
            "STID marker",
            offset,
            format!("expected 1, found {marker}"),
        ));
    }
    let count = reader.count("STID reference count", CountWidth::U32, options.limits)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(BankReference {
            id: reader.u32("STID reference identifier")?,
            name: reader.length_prefixed_string(
                CountWidth::U8,
                false,
                "STID reference name",
                options.limits,
            )?,
        });
    }
    ensure_empty(&reader, "STID")?;
    Ok(BankReferences { marker, entries })
}

fn parse_platform(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<PlatformSetting> {
    if version.before(113) {
        return Err(BnkError::invalid(
            "PLAT",
            offset,
            format!("PLAT is not defined for version {}", version.number()),
        ));
    }
    let mut reader = SliceReader::new(bytes, offset);
    let name = if version.before(118) {
        reader.length_prefixed_string(CountWidth::U32, false, "PLAT platform", options.limits)?
    } else if version.before(140) {
        reader.length_prefixed_string(CountWidth::U32, true, "PLAT platform", options.limits)?
    } else {
        reader.c_string("PLAT platform", options.limits)?
    };
    ensure_empty(&reader, "PLAT")?;
    Ok(PlatformSetting { name })
}

fn parse_hierarchy(
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<Vec<HierarchyObject>> {
    let mut reader = SliceReader::new(bytes, offset);
    let count = reader.count("HIRC object count", CountWidth::U32, options.limits)?;
    let mut objects = Vec::with_capacity(count);
    for _ in 0..count {
        let type_code = reader.u8("HIRC object type")?;
        let length = reader.u32("HIRC object length")? as usize;
        if length < 4 {
            return Err(BnkError::invalid(
                "HIRC object length",
                reader.offset().saturating_sub(4),
                format!("length {length} is smaller than the four-byte identifier"),
            ));
        }
        check_limit(
            "HIRC object bytes",
            length as u64,
            options.limits.max_hierarchy_object_bytes as u64,
        )?;
        let object_offset = reader.offset();
        let object_bytes = reader.take(length, "HIRC object")?;
        let mut object_reader = SliceReader::new(object_bytes, object_offset);
        let id = object_reader.u32("HIRC object identifier")?;
        let body_bytes = object_reader.take(object_reader.remaining(), "HIRC object body")?;
        let kind = HierarchyKind::from_code(version, type_code);
        let body = parse_hierarchy_body(kind, body_bytes, object_offset + 4, version, options)?;
        objects.push(HierarchyObject {
            kind,
            type_code,
            id,
            body,
        });
    }
    ensure_empty(&reader, "HIRC")?;
    Ok(objects)
}

fn parse_hierarchy_body(
    kind: HierarchyKind,
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    options: DecodeOptions,
) -> Result<HierarchyBody> {
    match kind {
        HierarchyKind::StatefulPropertySetting => {
            let mut reader = SliceReader::new(bytes, offset);
            let count_width = if version.before(128) {
                CountWidth::U8
            } else {
                CountWidth::U16
            };
            let count =
                reader.count("stateful property value count", count_width, options.limits)?;
            let mut types = Vec::with_capacity(count);
            for _ in 0..count {
                types.push(if version.before(128) {
                    reader.u8("stateful property type")? as u16
                } else {
                    reader.u16("stateful property type")?
                });
            }
            let mut values = Vec::with_capacity(count);
            for property_type in types {
                values.push(StatefulPropertyValue {
                    property_type,
                    value: reader.f32("stateful property value")?,
                });
            }
            ensure_empty(&reader, "stateful property setting")?;
            Ok(HierarchyBody::StatefulPropertySetting(
                StatefulPropertySetting { values },
            ))
        }
        HierarchyKind::Event => {
            let mut reader = SliceReader::new(bytes, offset);
            let count_width = if version.before(125) {
                CountWidth::U32
            } else {
                CountWidth::U8
            };
            let count = reader.count("event action count", count_width, options.limits)?;
            let mut actions = Vec::with_capacity(count);
            for _ in 0..count {
                actions.push(reader.u32("event action identifier")?);
            }
            ensure_empty(&reader, "event")?;
            Ok(HierarchyBody::Event(Event { actions }))
        }
        HierarchyKind::EventAction => {
            let mut reader = SliceReader::new(bytes, offset);
            let scope_offset = reader.offset();
            let scope_and_mode = reader.u8("event action scope and mode")?;
            let scope_layout = if version.before(125) {
                PackedLayout::EventActionScopeAndMode72
            } else {
                PackedLayout::EventActionScopeAndMode125
            };
            if options.validation.is_strict() && !scope_layout.is_canonical(scope_and_mode.into()) {
                return Err(BnkError::invalid(
                    "event action scope and mode",
                    scope_offset,
                    format!("non-zero reserved bits in 0x{scope_and_mode:02x}"),
                ));
            }
            let mode = if version.before(125) {
                (scope_and_mode >> 1) & 0x07
            } else {
                (scope_and_mode >> 1) & 0x03
            };
            let valid_mode = if version.before(125) {
                matches!(mode, 0 | 1 | 2 | 4)
            } else {
                matches!(mode, 0..=2)
            };
            if options.validation.is_strict() && !valid_mode {
                return Err(BnkError::invalid(
                    "event action mode",
                    scope_offset,
                    format!("unknown packed mode {mode}"),
                ));
            }
            let action_type = reader.u8("event action type")?;
            if options.validation.is_strict()
                && EnumerationKind::EventActionType
                    .variant(version.number(), action_type.into())
                    .is_none()
            {
                return Err(BnkError::invalid(
                    "event action type",
                    reader.offset() - 1,
                    format!("unknown value {action_type} for Wwise {}", version.number()),
                ));
            }
            let target = reader.u32("event action target")?;
            let u1 = reader.u8("event action u1")?;
            let payload_offset = reader.offset();
            let payload_bytes = reader.take(reader.remaining(), "event action payload")?;
            let payload = decode_event_action_fields(
                payload_bytes,
                payload_offset,
                version,
                action_type,
                options.validation,
                options.limits,
            )?;
            Ok(HierarchyBody::EventAction(EventAction {
                scope_and_mode,
                action_type,
                target,
                u1,
                payload,
            }))
        }
        HierarchyKind::DialogueEvent => {
            let mut reader = SliceReader::new(bytes, offset);
            let probability = version
                .at_least(88)
                .then(|| reader.u8("dialogue event probability"))
                .transpose()?;
            let association_offset = reader.offset();
            let association_bytes =
                reader.take(reader.remaining(), "dialogue event association")?;
            let association = decode_dialogue_fields(
                association_bytes,
                association_offset,
                version,
                options.validation,
                options.limits,
            )?;
            Ok(HierarchyBody::DialogueEvent(DialogueEvent {
                probability,
                association,
            }))
        }
        HierarchyKind::Sound => {
            let mut reader = SliceReader::new(bytes, offset);
            let source = parse_audio_source(&mut reader, version, options.validation)?;
            let settings_offset = reader.offset();
            let settings_bytes = reader.take(reader.remaining(), "sound hierarchy settings")?;
            let settings = decode_fields(
                HierarchyKind::Sound,
                settings_bytes,
                settings_offset,
                version,
                options.validation,
                options.limits,
            )?;
            Ok(HierarchyBody::Sound(SoundHierarchyObject {
                source,
                settings,
            }))
        }
        HierarchyKind::Effect | HierarchyKind::Source | HierarchyKind::AudioDevice => {
            let mut reader = SliceReader::new(bytes, offset);
            let plugin = reader.u32("HIRC plug-in identifier")?;
            let expand_length = reader.count(
                "HIRC plug-in expand length",
                CountWidth::U32,
                options.limits,
            )?;
            let expand = reader.take(expand_length, "HIRC plug-in expand")?.to_vec();
            let settings_offset = reader.offset();
            let settings_bytes = reader.take(reader.remaining(), "HIRC plug-in settings")?;
            let settings = decode_fields(
                kind,
                settings_bytes,
                settings_offset,
                version,
                options.validation,
                options.limits,
            )?;
            let value = PluginHierarchyObject {
                plugin,
                expand,
                settings,
            };
            Ok(match kind {
                HierarchyKind::Effect => HierarchyBody::Effect(value),
                HierarchyKind::Source => HierarchyBody::Source(value),
                HierarchyKind::AudioDevice => HierarchyBody::AudioDevice(value),
                _ => unreachable!(),
            })
        }
        HierarchyKind::AudioBus | HierarchyKind::AuxiliaryAudioBus => {
            let mut reader = SliceReader::new(bytes, offset);
            let parent = reader.u32("audio bus parent")?;
            let audio_device = if version.at_least(128) && parent == 0 {
                Some(reader.u32("audio bus audio device")?)
            } else {
                None
            };
            let settings_offset = reader.offset();
            let settings_bytes = reader.take(reader.remaining(), "audio bus settings")?;
            let settings = decode_fields(
                kind,
                settings_bytes,
                settings_offset,
                version,
                options.validation,
                options.limits,
            )?;
            let value = BusHierarchyObject {
                parent,
                audio_device,
                settings,
            };
            Ok(match kind {
                HierarchyKind::AudioBus => HierarchyBody::AudioBus(value),
                HierarchyKind::AuxiliaryAudioBus => HierarchyBody::AuxiliaryAudioBus(value),
                _ => unreachable!(),
            })
        }
        HierarchyKind::Attenuation
        | HierarchyKind::LowFrequencyOscillatorModulator
        | HierarchyKind::EnvelopeModulator
        | HierarchyKind::TimeModulator
        | HierarchyKind::SoundPlaylistContainer
        | HierarchyKind::SoundSwitchContainer
        | HierarchyKind::SoundBlendContainer
        | HierarchyKind::ActorMixer
        | HierarchyKind::MusicTrack
        | HierarchyKind::MusicSegment
        | HierarchyKind::MusicPlaylistContainer
        | HierarchyKind::MusicSwitchContainer => {
            let fields = decode_fields(
                kind,
                bytes,
                offset,
                version,
                options.validation,
                options.limits,
            )?;
            Ok(match kind {
                HierarchyKind::Attenuation => HierarchyBody::Attenuation(fields),
                HierarchyKind::LowFrequencyOscillatorModulator => {
                    HierarchyBody::LowFrequencyOscillatorModulator(fields)
                }
                HierarchyKind::EnvelopeModulator => HierarchyBody::EnvelopeModulator(fields),
                HierarchyKind::TimeModulator => HierarchyBody::TimeModulator(fields),
                HierarchyKind::SoundPlaylistContainer => {
                    HierarchyBody::SoundPlaylistContainer(fields)
                }
                HierarchyKind::SoundSwitchContainer => HierarchyBody::SoundSwitchContainer(fields),
                HierarchyKind::SoundBlendContainer => HierarchyBody::SoundBlendContainer(fields),
                HierarchyKind::ActorMixer => HierarchyBody::ActorMixer(fields),
                HierarchyKind::MusicTrack => HierarchyBody::MusicTrack(fields),
                HierarchyKind::MusicSegment => HierarchyBody::MusicSegment(fields),
                HierarchyKind::MusicPlaylistContainer => {
                    HierarchyBody::MusicPlaylistContainer(fields)
                }
                HierarchyKind::MusicSwitchContainer => HierarchyBody::MusicSwitchContainer(fields),
                _ => unreachable!(),
            })
        }
        _ => Ok(HierarchyBody::Raw(bytes.to_vec())),
    }
}

fn parse_audio_source(
    reader: &mut SliceReader<'_>,
    version: BankVersion,
    validation: ValidationMode,
) -> Result<AudioSourceSetting> {
    let plugin = reader.u32("audio source plug-in identifier")?;
    let source_type_offset = reader.offset();
    let source_type_raw = if version.before(112) {
        reader.u32("audio source type")?
    } else {
        reader.u8("audio source type")? as u32
    };
    if validation.is_strict() {
        ensure_enumeration(
            EnumerationKind::AudioSourceType,
            version,
            source_type_raw,
            "audio source type",
            source_type_offset,
        )?;
    }
    let source_type = AudioSourceType::from_raw(version, source_type_raw);
    let resource = reader.u32("audio source resource identifier")?;
    let source = version
        .before(113)
        .then(|| reader.u32("audio source identifier"))
        .transpose()?;
    let resource_offset = (version.before(113) && source_type != AudioSourceType::Streamed)
        .then(|| reader.u32("audio source resource offset"))
        .transpose()?;
    let resource_size = (version.at_least(112) || source_type != AudioSourceType::Streamed)
        .then(|| reader.u32("audio source resource size"))
        .transpose()?;
    let flags_offset = reader.offset();
    let flags = reader.u8("audio source flags")?;
    let flags_layout = if version.before(112) {
        PackedLayout::Boolean
    } else {
        PackedLayout::Source
    };
    if validation.is_strict() && !flags_layout.is_canonical(flags.into()) {
        return Err(BnkError::invalid(
            "audio source flags",
            flags_offset,
            format!("non-zero reserved bits in 0x{flags:02x}"),
        ));
    }
    let plugin_reserved = ((plugin & 0xffff) >= 2)
        .then(|| reader.u32("audio source plug-in reserved value"))
        .transpose()?;
    if validation.is_strict() && plugin_reserved.is_some_and(|value| value != 0) {
        return Err(BnkError::invalid(
            "audio source plug-in reserved value",
            reader.offset().saturating_sub(4),
            format!("expected 0, found {}", plugin_reserved.unwrap_or_default()),
        ));
    }
    Ok(AudioSourceSetting {
        plugin,
        source_type,
        resource,
        source,
        resource_offset,
        resource_size,
        flags,
        plugin_reserved,
    })
}
