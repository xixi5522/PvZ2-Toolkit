use crate::{
    BorrowedRawResourceManifest, NewtonError, OwnedRawResourceManifest, RawResource,
    RawResourceGroup, RawResourceManifest, RawSubgroup, ResourceManifest, Result,
    raw::ensure_semantic_allocation_budget,
};
use std::io::{self, Read};
use std::mem::size_of;

/// Allocation and work limits used while decoding untrusted NEWTON data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_groups: usize,
    pub max_subgroups_per_group: usize,
    pub max_resources_per_group: usize,
    pub max_total_entries: usize,
    pub max_string_bytes: usize,
    pub max_total_string_bytes: usize,
    pub max_allocation_bytes: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_groups: 100_000,
            max_subgroups_per_group: 100_000,
            max_resources_per_group: 1_000_000,
            max_total_entries: 2_000_000,
            max_string_bytes: 1024 * 1024,
            max_total_string_bytes: 512 * 1024 * 1024,
            max_allocation_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Decode a canonical semantic NEWTON document with default limits.
pub fn decode_newton(reader: impl Read) -> Result<ResourceManifest> {
    from_reader_with_limits(reader, DecodeLimits::default())
}

/// Decode a canonical semantic NEWTON document with caller-provided limits.
pub fn from_reader_with_limits(
    reader: impl Read,
    limits: DecodeLimits,
) -> Result<ResourceManifest> {
    let raw = raw_from_reader_with_limits(reader, limits)?;
    ensure_semantic_allocation_budget(&raw, limits.max_allocation_bytes)?;
    ResourceManifest::try_from(raw)
}

/// Decode a wire-faithful owned NEWTON document with default limits.
pub fn decode_raw_newton(reader: impl Read) -> Result<OwnedRawResourceManifest> {
    raw_from_reader_with_limits(reader, DecodeLimits::default())
}

/// Decode a wire-faithful owned NEWTON document with caller-provided limits.
pub fn raw_from_reader_with_limits(
    reader: impl Read,
    limits: DecodeLimits,
) -> Result<OwnedRawResourceManifest> {
    let mut decoder = Decoder::new(StreamSource::new(reader), limits);
    let result = decoder.decode();
    decoder.with_context(result)
}

/// Decode a complete wire-faithful NEWTON document without copying strings.
pub fn raw_from_bytes_borrowed(bytes: &[u8]) -> Result<BorrowedRawResourceManifest<'_>> {
    raw_from_bytes_borrowed_with_limits(bytes, DecodeLimits::default())
}

/// Decode a complete wire-faithful NEWTON document without copying strings,
/// using caller-provided limits.
pub fn raw_from_bytes_borrowed_with_limits(
    bytes: &[u8],
    limits: DecodeLimits,
) -> Result<BorrowedRawResourceManifest<'_>> {
    let mut decoder = Decoder::new(SliceSource::new(bytes), limits);
    let decoded = match decoder.decode() {
        Ok(decoded) => decoded,
        Err(error) => return Err(decoder.contextualize(error)),
    };
    let consumed =
        usize::try_from(decoder.position()).map_err(|_| NewtonError::LengthOverflow {
            field: "document position",
        })?;
    if consumed != bytes.len() {
        decoder.set_context(DecodeEntry::Document, "trailing bytes");
        return Err(decoder.contextualize(NewtonError::TrailingData {
            remaining: bytes.len() - consumed,
        }));
    }
    Ok(decoded)
}

trait Source {
    type Bytes;

    fn position(&self) -> u64;
    fn string_allocation_cost(length: usize) -> usize;
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]>;
    fn read_bytes(&mut self, length: usize) -> Result<Self::Bytes>;
}

struct StreamSource<R> {
    reader: R,
    position: u64,
}

impl<R> StreamSource<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            position: 0,
        }
    }
}

impl<R: Read> Source for StreamSource<R> {
    type Bytes = Vec<u8>;

    fn position(&self) -> u64 {
        self.position
    }

    fn string_allocation_cost(length: usize) -> usize {
        length
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0; N];
        self.reader.read_exact(&mut bytes)?;
        self.position = self
            .position
            .checked_add(N as u64)
            .ok_or(NewtonError::LengthOverflow {
                field: "document position",
            })?;
        Ok(bytes)
    }

    fn read_bytes(&mut self, length: usize) -> Result<Self::Bytes> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|source| NewtonError::AllocationFailed {
                field: "string bytes",
                source,
            })?;
        bytes.resize(length, 0);
        self.reader.read_exact(&mut bytes)?;
        self.position =
            self.position
                .checked_add(length as u64)
                .ok_or(NewtonError::LengthOverflow {
                    field: "document position",
                })?;
        Ok(bytes)
    }
}

struct SliceSource<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> SliceSource<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(NewtonError::LengthOverflow {
                field: "document position",
            })?;
        let bytes = self.bytes.get(self.position..end).ok_or_else(|| {
            NewtonError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "NEWTON input ended inside a field",
            ))
        })?;
        self.position = end;
        Ok(bytes)
    }
}

impl<'a> Source for SliceSource<'a> {
    type Bytes = &'a [u8];

    fn position(&self) -> u64 {
        self.position as u64
    }

    fn string_allocation_cost(_length: usize) -> usize {
        0
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| NewtonError::LengthOverflow {
                field: "fixed-size field",
            })
    }

    fn read_bytes(&mut self, length: usize) -> Result<Self::Bytes> {
        self.take(length)
    }
}

struct Decoder<S> {
    source: S,
    limits: DecodeLimits,
    total_entries: usize,
    total_string_bytes: usize,
    allocation_bytes: usize,
    entry: DecodeEntry,
    field: &'static str,
    field_offset: u64,
}

impl<S: Source> Decoder<S> {
    fn new(source: S, limits: DecodeLimits) -> Self {
        Self {
            source,
            limits,
            total_entries: 0,
            total_string_bytes: 0,
            allocation_bytes: 0,
            entry: DecodeEntry::Document,
            field: "header",
            field_offset: 0,
        }
    }

    fn decode(&mut self) -> Result<RawResourceManifest<S::Bytes>> {
        self.set_context(DecodeEntry::Document, "slot count");
        let slot_count = self.read_i32("slot count")?;
        let group_count = self.read_count("group count", self.limits.max_groups)?;
        self.charge_entries(group_count)?;
        let mut groups = self.allocate_vec(group_count, "group records")?;

        for group_index in 0..group_count {
            self.set_context(DecodeEntry::Group(group_index), "group type");
            let group_type = self.read_u8("group type")?;
            let resolution = self.read_i32("group resolution")?;
            let subgroup_count =
                self.read_count("subgroup count", self.limits.max_subgroups_per_group)?;
            let resource_count =
                self.read_count("resource count", self.limits.max_resources_per_group)?;
            self.charge_entries(subgroup_count)?;
            self.charge_entries(resource_count)?;
            let has_id = self.read_u8("group id flag")?;
            let has_parent = self.read_u8("group parent flag")?;
            let id = self.read_optional_string(has_id, "group id")?;
            let parent = self.read_optional_string(has_parent, "group parent")?;

            let mut subgroups = self.allocate_vec(subgroup_count, "subgroup records")?;
            for subgroup_index in 0..subgroup_count {
                self.set_context(
                    DecodeEntry::Subgroup {
                        group: group_index,
                        subgroup: subgroup_index,
                    },
                    "subgroup resolution",
                );
                subgroups.push(RawSubgroup {
                    resolution: self.read_i32("subgroup resolution")?,
                    id: self.read_string("subgroup id")?,
                });
            }

            let mut resources = self.allocate_vec(resource_count, "resource records")?;
            for resource_index in 0..resource_count {
                self.set_context(
                    DecodeEntry::Resource {
                        group: group_index,
                        resource: resource_index,
                    },
                    "resource type",
                );
                resources.push(self.read_resource()?);
            }

            groups.push(RawResourceGroup {
                group_type,
                resolution,
                has_id,
                has_parent,
                id,
                parent,
                subgroups,
                resources,
            });
        }

        Ok(RawResourceManifest { slot_count, groups })
    }

    fn read_resource(&mut self) -> Result<RawResource<S::Bytes>> {
        let resource_type = self.read_u8("resource type")?;
        let slot = self.read_i32("resource slot")?;
        let width = self.read_i32("resource width")?;
        let height = self.read_i32("resource height")?;
        let x = self.read_i32("resource x")?;
        let y = self.read_i32("resource y")?;
        let ax = self.read_i32("resource atlas x")?;
        let ay = self.read_i32("resource atlas y")?;
        let aw = self.read_i32("resource atlas width")?;
        let ah = self.read_i32("resource atlas height")?;
        let cols = self.read_i32("resource columns")?;
        let rows = self.read_i32("resource rows")?;
        let atlas = self.read_u8("resource atlas flag")?;
        let has_id = self.read_u8("resource id flag")?;
        let has_path = self.read_u8("resource path flag")?;
        let has_parent = self.read_u8("resource parent flag")?;
        let id = self.read_optional_string(has_id, "resource id")?;
        let path = self.read_optional_string(has_path, "resource path")?;
        let parent = self.read_optional_string(has_parent, "resource parent")?;

        Ok(RawResource {
            resource_type,
            slot,
            width,
            height,
            x,
            y,
            ax,
            ay,
            aw,
            ah,
            cols,
            rows,
            atlas,
            has_id,
            has_path,
            has_parent,
            id,
            path,
            parent,
        })
    }

    fn read_optional_string(&mut self, flag: u8, field: &'static str) -> Result<Option<S::Bytes>> {
        (flag == 1).then(|| self.read_string(field)).transpose()
    }

    fn read_string(&mut self, field: &'static str) -> Result<S::Bytes> {
        let raw_length = self.read_i32(field)?;
        let length = usize::try_from(raw_length).map_err(|_| NewtonError::NegativeValue {
            field,
            value: raw_length,
        })?;
        if length > self.limits.max_string_bytes {
            return Err(NewtonError::StringLimitExceeded {
                length,
                limit: self.limits.max_string_bytes,
            });
        }
        self.total_string_bytes =
            self.total_string_bytes
                .checked_add(length)
                .ok_or(NewtonError::LengthOverflow {
                    field: "cumulative string bytes",
                })?;
        if self.total_string_bytes > self.limits.max_total_string_bytes {
            return Err(NewtonError::TotalStringLimitExceeded {
                requested: self.total_string_bytes,
                limit: self.limits.max_total_string_bytes,
            });
        }
        self.charge_allocation(S::string_allocation_cost(length))?;
        self.mark_field(field);
        self.source.read_bytes(length)
    }

    fn read_count(&mut self, field: &'static str, limit: usize) -> Result<usize> {
        let raw = self.read_i32(field)?;
        let count =
            usize::try_from(raw).map_err(|_| NewtonError::NegativeValue { field, value: raw })?;
        if count > limit {
            return Err(NewtonError::CountLimitExceeded {
                field,
                count,
                limit,
            });
        }
        Ok(count)
    }

    fn read_u8(&mut self, field: &'static str) -> Result<u8> {
        self.mark_field(field);
        Ok(self.source.read_array::<1>()?[0])
    }

    fn read_i32(&mut self, field: &'static str) -> Result<i32> {
        self.mark_field(field);
        Ok(i32::from_le_bytes(self.source.read_array::<4>()?))
    }

    fn allocate_vec<T>(&mut self, count: usize, field: &'static str) -> Result<Vec<T>> {
        let bytes = size_of::<T>()
            .checked_mul(count)
            .ok_or(NewtonError::LengthOverflow { field })?;
        self.charge_allocation(bytes)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|source| NewtonError::AllocationFailed { field, source })?;
        Ok(values)
    }

    fn charge_entries(&mut self, count: usize) -> Result<()> {
        self.total_entries =
            self.total_entries
                .checked_add(count)
                .ok_or(NewtonError::LengthOverflow {
                    field: "total entry",
                })?;
        if self.total_entries > self.limits.max_total_entries {
            return Err(NewtonError::CountLimitExceeded {
                field: "total entry",
                count: self.total_entries,
                limit: self.limits.max_total_entries,
            });
        }
        Ok(())
    }

    fn charge_allocation(&mut self, bytes: usize) -> Result<()> {
        self.allocation_bytes =
            self.allocation_bytes
                .checked_add(bytes)
                .ok_or(NewtonError::LengthOverflow {
                    field: "estimated allocation bytes",
                })?;
        if self.allocation_bytes > self.limits.max_allocation_bytes {
            return Err(NewtonError::AllocationLimitExceeded {
                requested: self.allocation_bytes,
                limit: self.limits.max_allocation_bytes,
            });
        }
        Ok(())
    }

    fn mark_field(&mut self, field: &'static str) {
        self.field = field;
        self.field_offset = self.source.position();
    }

    fn set_context(&mut self, entry: DecodeEntry, field: &'static str) {
        self.entry = entry;
        self.mark_field(field);
    }

    fn position(&self) -> u64 {
        self.source.position()
    }

    fn contextualize(&self, source: NewtonError) -> NewtonError {
        NewtonError::DecodeContext {
            offset: self.field_offset,
            context: self.entry.to_string(),
            field: self.field,
            source: Box::new(source),
        }
    }

    fn with_context<T>(&self, result: Result<T>) -> Result<T> {
        result.map_err(|error| self.contextualize(error))
    }
}

#[derive(Debug, Clone, Copy)]
enum DecodeEntry {
    Document,
    Group(usize),
    Subgroup { group: usize, subgroup: usize },
    Resource { group: usize, resource: usize },
}

impl std::fmt::Display for DecodeEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document => formatter.write_str("document"),
            Self::Group(group) => write!(formatter, "group[{group}]"),
            Self::Subgroup { group, subgroup } => {
                write!(formatter, "group[{group}].subgroup[{subgroup}]")
            }
            Self::Resource { group, resource } => {
                write!(formatter, "group[{group}].resource[{resource}]")
            }
        }
    }
}
