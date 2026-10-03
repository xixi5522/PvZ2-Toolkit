use crate::{
    CompositeGroup, NewtonError, Resource, ResourceGroup, ResourceManifest, ResourceType, Result,
    SimpleGroup, Subgroup,
};
use std::mem::size_of;

const ABSENT_COORDINATE: i32 = i32::MAX;

/// Wire-faithful NEWTON document.
///
/// Unlike [`ResourceManifest`], this type preserves raw type and flag bytes,
/// optional ID/path fields, non-UTF-8 strings, mixed group contents, and signed
/// integer values. Use it for inspection, repair, and lossless round-tripping
/// of non-canonical files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResourceManifest<B = Vec<u8>> {
    pub slot_count: i32,
    pub groups: Vec<RawResourceGroup<B>>,
}

/// Wire-faithful group record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResourceGroup<B = Vec<u8>> {
    pub group_type: u8,
    pub resolution: i32,
    pub has_id: u8,
    pub has_parent: u8,
    pub id: Option<B>,
    pub parent: Option<B>,
    pub subgroups: Vec<RawSubgroup<B>>,
    pub resources: Vec<RawResource<B>>,
}

/// Wire-faithful composite subgroup record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSubgroup<B = Vec<u8>> {
    pub resolution: i32,
    pub id: B,
}

/// Wire-faithful resource record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResource<B = Vec<u8>> {
    pub resource_type: u8,
    pub slot: i32,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub ax: i32,
    pub ay: i32,
    pub aw: i32,
    pub ah: i32,
    pub cols: i32,
    pub rows: i32,
    pub atlas: u8,
    pub has_id: u8,
    pub has_path: u8,
    pub has_parent: u8,
    pub id: Option<B>,
    pub path: Option<B>,
    pub parent: Option<B>,
}

pub type OwnedRawResourceManifest = RawResourceManifest<Vec<u8>>;
pub type BorrowedRawResourceManifest<'a> = RawResourceManifest<&'a [u8]>;

pub(crate) fn ensure_semantic_allocation_budget<B: AsRef<[u8]>>(
    raw: &RawResourceManifest<B>,
    limit: usize,
) -> Result<()> {
    let mut bytes = allocation_size::<RawResourceGroup<B>>(raw.groups.len())?;
    bytes = checked_allocation_add(bytes, allocation_size::<ResourceGroup>(raw.groups.len())?)?;
    for group in &raw.groups {
        bytes = checked_allocation_add(
            bytes,
            allocation_size::<RawSubgroup<B>>(group.subgroups.len())?,
        )?;
        bytes = checked_allocation_add(bytes, allocation_size::<Subgroup>(group.subgroups.len())?)?;
        bytes = checked_allocation_add(
            bytes,
            allocation_size::<RawResource<B>>(group.resources.len())?,
        )?;
        bytes = checked_allocation_add(bytes, allocation_size::<Resource>(group.resources.len())?)?;
        for value in [&group.id, &group.parent].into_iter().flatten() {
            bytes = checked_allocation_add(bytes, value.as_ref().len())?;
        }
        for subgroup in &group.subgroups {
            bytes = checked_allocation_add(bytes, subgroup.id.as_ref().len())?;
        }
        for resource in &group.resources {
            for value in [&resource.id, &resource.path, &resource.parent]
                .into_iter()
                .flatten()
            {
                bytes = checked_allocation_add(bytes, value.as_ref().len())?;
            }
        }
    }
    if bytes > limit {
        return Err(NewtonError::AllocationLimitExceeded {
            requested: bytes,
            limit,
        });
    }
    Ok(())
}

impl<'a> BorrowedRawResourceManifest<'a> {
    /// Copy a borrowed raw document into independently owned byte buffers.
    pub fn try_into_owned(self) -> Result<OwnedRawResourceManifest> {
        let mut groups = Vec::new();
        reserve_exact(&mut groups, self.groups.len(), "owned raw groups")?;
        for group in self.groups {
            let mut subgroups = Vec::new();
            reserve_exact(&mut subgroups, group.subgroups.len(), "owned raw subgroups")?;
            for subgroup in group.subgroups {
                subgroups.push(RawSubgroup {
                    resolution: subgroup.resolution,
                    id: copy_bytes(subgroup.id, "subgroup id")?,
                });
            }

            let mut resources = Vec::new();
            reserve_exact(&mut resources, group.resources.len(), "owned raw resources")?;
            for resource in group.resources {
                resources.push(RawResource {
                    resource_type: resource.resource_type,
                    slot: resource.slot,
                    width: resource.width,
                    height: resource.height,
                    x: resource.x,
                    y: resource.y,
                    ax: resource.ax,
                    ay: resource.ay,
                    aw: resource.aw,
                    ah: resource.ah,
                    cols: resource.cols,
                    rows: resource.rows,
                    atlas: resource.atlas,
                    has_id: resource.has_id,
                    has_path: resource.has_path,
                    has_parent: resource.has_parent,
                    id: resource
                        .id
                        .map(|value| copy_bytes(value, "resource id"))
                        .transpose()?,
                    path: resource
                        .path
                        .map(|value| copy_bytes(value, "resource path"))
                        .transpose()?,
                    parent: resource
                        .parent
                        .map(|value| copy_bytes(value, "resource parent"))
                        .transpose()?,
                });
            }

            groups.push(RawResourceGroup {
                group_type: group.group_type,
                resolution: group.resolution,
                has_id: group.has_id,
                has_parent: group.has_parent,
                id: group
                    .id
                    .map(|value| copy_bytes(value, "group id"))
                    .transpose()?,
                parent: group
                    .parent
                    .map(|value| copy_bytes(value, "group parent"))
                    .transpose()?,
                subgroups,
                resources,
            });
        }
        Ok(RawResourceManifest {
            slot_count: self.slot_count,
            groups,
        })
    }
}

impl TryFrom<OwnedRawResourceManifest> for ResourceManifest {
    type Error = NewtonError;

    fn try_from(raw: OwnedRawResourceManifest) -> Result<Self> {
        convert_raw(raw, |value, field| {
            String::from_utf8(value).map_err(|_| NewtonError::InvalidUtf8 { field })
        })
    }
}

impl<'a> TryFrom<BorrowedRawResourceManifest<'a>> for ResourceManifest {
    type Error = NewtonError;

    fn try_from(raw: BorrowedRawResourceManifest<'a>) -> Result<Self> {
        convert_raw(raw, |value, field| {
            let value =
                std::str::from_utf8(value).map_err(|_| NewtonError::InvalidUtf8 { field })?;
            let mut owned = String::new();
            owned
                .try_reserve_exact(value.len())
                .map_err(|source| NewtonError::AllocationFailed { field, source })?;
            owned.push_str(value);
            Ok(owned)
        })
    }
}

fn convert_raw<B>(
    raw: RawResourceManifest<B>,
    mut text: impl FnMut(B, &'static str) -> Result<String>,
) -> Result<ResourceManifest> {
    let slot_count = nonnegative(raw.slot_count, "slot count")?;
    let mut groups = Vec::new();
    reserve_exact(&mut groups, raw.groups.len(), "group")?;

    for (group_index, raw_group) in raw.groups.into_iter().enumerate() {
        let group =
            convert_group(raw_group, &mut text).map_err(|source| NewtonError::SemanticContext {
                context: format!("group[{group_index}]"),
                source: Box::new(source),
            })?;
        groups.push(group);
    }

    Ok(ResourceManifest { slot_count, groups })
}

fn convert_group<B>(
    raw: RawResourceGroup<B>,
    text: &mut impl FnMut(B, &'static str) -> Result<String>,
) -> Result<ResourceGroup> {
    let has_id = strict_bool(raw.has_id, "group id flag")?;
    let has_parent = strict_bool(raw.has_parent, "group parent flag")?;
    let id = required_text(has_id, raw.id, "group id", text)?;
    let parent = optional_text(has_parent, raw.parent, "group parent", text)?;
    let resolution = optional_nonzero(raw.resolution, "group resolution")?;

    match raw.group_type {
        1 => {
            if !raw.resources.is_empty() {
                return Err(NewtonError::CompositeContainsResources {
                    group: id,
                    resources: raw.resources.len(),
                });
            }
            let mut subgroups = Vec::new();
            reserve_exact(&mut subgroups, raw.subgroups.len(), "subgroup")?;
            for (subgroup_index, subgroup) in raw.subgroups.into_iter().enumerate() {
                let converted = (|| {
                    Ok(Subgroup {
                        resolution: optional_nonzero(subgroup.resolution, "subgroup resolution")?,
                        id: text(subgroup.id, "subgroup id")?,
                    })
                })()
                .map_err(|source| NewtonError::SemanticContext {
                    context: format!("subgroup[{subgroup_index}]"),
                    source: Box::new(source),
                })?;
                subgroups.push(converted);
            }
            Ok(ResourceGroup::Composite(CompositeGroup {
                id,
                resolution,
                parent,
                subgroups,
            }))
        }
        2 => {
            if !raw.subgroups.is_empty() {
                return Err(NewtonError::SimpleContainsSubgroups {
                    group: id,
                    subgroups: raw.subgroups.len(),
                });
            }
            let mut resources = Vec::new();
            reserve_exact(&mut resources, raw.resources.len(), "resource")?;
            for (resource_index, resource) in raw.resources.into_iter().enumerate() {
                let converted = convert_resource(resource, text).map_err(|source| {
                    NewtonError::SemanticContext {
                        context: format!("resource[{resource_index}]"),
                        source: Box::new(source),
                    }
                })?;
                resources.push(converted);
            }
            Ok(ResourceGroup::Simple(SimpleGroup {
                id,
                resolution,
                parent,
                resources,
            }))
        }
        value => Err(NewtonError::InvalidGroupType(value)),
    }
}

fn convert_resource<B>(
    raw: RawResource<B>,
    text: &mut impl FnMut(B, &'static str) -> Result<String>,
) -> Result<Resource> {
    let has_id = strict_bool(raw.has_id, "resource id flag")?;
    let has_path = strict_bool(raw.has_path, "resource path flag")?;
    let has_parent = strict_bool(raw.has_parent, "resource parent flag")?;

    Ok(Resource {
        resource_type: ResourceType::try_from(raw.resource_type)?,
        slot: nonnegative(raw.slot, "resource slot")?,
        id: required_text(has_id, raw.id, "resource id", text)?,
        path: required_text(has_path, raw.path, "resource path", text)?,
        width: optional_nonzero(raw.width, "resource width")?,
        height: optional_nonzero(raw.height, "resource height")?,
        x: optional_coordinate(raw.x),
        y: optional_coordinate(raw.y),
        ax: optional_nonzero(raw.ax, "resource atlas x")?,
        ay: optional_nonzero(raw.ay, "resource atlas y")?,
        aw: optional_nonzero(raw.aw, "resource atlas width")?,
        ah: optional_nonzero(raw.ah, "resource atlas height")?,
        cols: optional_default_one(raw.cols, "resource columns")?,
        rows: optional_default_one(raw.rows, "resource rows")?,
        atlas: strict_bool(raw.atlas, "resource atlas flag")?,
        parent: optional_text(has_parent, raw.parent, "resource parent", text)?,
    })
}

fn required_text<B>(
    present: bool,
    value: Option<B>,
    field: &'static str,
    text: &mut impl FnMut(B, &'static str) -> Result<String>,
) -> Result<String> {
    if !present {
        return Err(NewtonError::MissingRequiredString { field });
    }
    text(
        value.ok_or(NewtonError::InconsistentPresence {
            field,
            flag: 1,
            has_value: false,
        })?,
        field,
    )
}

fn optional_text<B>(
    present: bool,
    value: Option<B>,
    field: &'static str,
    text: &mut impl FnMut(B, &'static str) -> Result<String>,
) -> Result<Option<String>> {
    match (present, value) {
        (true, Some(value)) => text(value, field).map(Some),
        (false, None) => Ok(None),
        (present, value) => Err(NewtonError::InconsistentPresence {
            field,
            flag: u8::from(present),
            has_value: value.is_some(),
        }),
    }
}

fn strict_bool(value: u8, field: &'static str) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(NewtonError::InvalidBoolean { field, value }),
    }
}

fn nonnegative(value: i32, field: &'static str) -> Result<u32> {
    u32::try_from(value).map_err(|_| NewtonError::NegativeValue { field, value })
}

fn optional_nonzero(value: i32, field: &'static str) -> Result<Option<u32>> {
    Ok(match nonnegative(value, field)? {
        0 => None,
        value => Some(value),
    })
}

fn optional_default_one(value: i32, field: &'static str) -> Result<Option<u32>> {
    Ok(match nonnegative(value, field)? {
        1 => None,
        value => Some(value),
    })
}

fn optional_coordinate(value: i32) -> Option<i32> {
    (value != ABSENT_COORDINATE).then_some(value)
}

fn reserve_exact<T>(values: &mut Vec<T>, count: usize, field: &'static str) -> Result<()> {
    values
        .try_reserve_exact(count)
        .map_err(|source| NewtonError::AllocationFailed { field, source })
}

fn copy_bytes(value: &[u8], field: &'static str) -> Result<Vec<u8>> {
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|source| NewtonError::AllocationFailed { field, source })?;
    owned.extend_from_slice(value);
    Ok(owned)
}

fn allocation_size<T>(count: usize) -> Result<usize> {
    size_of::<T>()
        .checked_mul(count)
        .ok_or(NewtonError::LengthOverflow {
            field: "semantic allocation estimate",
        })
}

fn checked_allocation_add(left: usize, right: usize) -> Result<usize> {
    left.checked_add(right).ok_or(NewtonError::LengthOverflow {
        field: "semantic allocation estimate",
    })
}
