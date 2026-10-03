use crate::{ResourceGroup, ResourceManifest, ResourceType};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationProfile {
    /// Constraints required to avoid the known PvZ2 runtime truncation,
    /// duplicate registration, and broken reference behavior.
    Pvz2Runtime,
    /// Runtime constraints plus the normalized invariants used by official
    /// generated manifests.
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationCode {
    DuplicateGroupId,
    DuplicateResourceId,
    DuplicateSlot,
    SlotOutOfRange,
    IncompleteSlotTable,
    MissingGroupReference,
    WrongGroupReferenceKind,
    MissingAtlasReference,
    InvalidAtlasReference,
    InvalidAtlasImage,
    InvalidSprite,
    GeometryOutOfRange,
    InvalidGridSize,
    UnexpectedResourceMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub code: ValidationCode,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ValidationSeverity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ValidationSeverity::Warning)
    }

    fn error(&mut self, code: ValidationCode, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(ValidationIssue {
            severity: ValidationSeverity::Error,
            code,
            path: path.into(),
            message: message.into(),
        });
    }

    fn warning(
        &mut self,
        code: ValidationCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.issues.push(ValidationIssue {
            severity: ValidationSeverity::Warning,
            code,
            path: path.into(),
            message: message.into(),
        });
    }
}

impl ResourceManifest {
    /// Validate cross-record and PvZ2 runtime invariants without modifying the
    /// manifest.
    pub fn validate(&self, profile: ValidationProfile) -> ValidationReport {
        validate_manifest(self, profile)
    }
}

fn validate_manifest(manifest: &ResourceManifest, profile: ValidationProfile) -> ValidationReport {
    let canonical = profile == ValidationProfile::Canonical;
    let mut report = ValidationReport::default();
    let mut groups = HashMap::<&str, (usize, bool)>::new();
    let mut resource_ids = HashMap::<&str, Vec<ResourceLocation<'_>>>::new();
    let mut slots = HashMap::<u32, Vec<ResourceLocation<'_>>>::new();

    for (group_index, group) in manifest.groups.iter().enumerate() {
        let is_composite = matches!(group, ResourceGroup::Composite(_));
        if let Some((previous, _)) = groups.insert(group.id(), (group_index, is_composite)) {
            report.error(
                ValidationCode::DuplicateGroupId,
                format!("groups[{group_index}].id"),
                format!(
                    "group ID {:?} was already defined by groups[{previous}]",
                    group.id()
                ),
            );
        }

        if let ResourceGroup::Simple(group) = group {
            for (resource_index, resource) in group.resources.iter().enumerate() {
                let path = format!("groups[{group_index}].resources[{resource_index}]");
                let location = ResourceLocation {
                    group_index,
                    resource_index,
                    resolution: group.resolution,
                    parent: group.parent.as_deref(),
                };
                let id_locations = resource_ids.entry(&resource.id).or_default();
                if let Some(previous) = id_locations
                    .iter()
                    .copied()
                    .find(|previous| !groups_are_mutually_exclusive(*previous, location))
                {
                    report.error(
                        ValidationCode::DuplicateResourceId,
                        format!("{path}.id"),
                        format!(
                            "resource ID {:?} was already defined by groups[{}].resources[{}]",
                            resource.id, previous.group_index, previous.resource_index
                        ),
                    );
                }
                id_locations.push(location);
                if resource.slot >= manifest.slot_count {
                    report.error(
                        ValidationCode::SlotOutOfRange,
                        format!("{path}.slot"),
                        format!(
                            "slot {} is outside slot_count {}",
                            resource.slot, manifest.slot_count
                        ),
                    );
                }
                let slot_locations = slots.entry(resource.slot).or_default();
                if let Some(previous) = slot_locations
                    .iter()
                    .copied()
                    .find(|previous| !groups_are_mutually_exclusive(*previous, location))
                {
                    report.error(
                        ValidationCode::DuplicateSlot,
                        format!("{path}.slot"),
                        format!(
                            "slot {} was already used by groups[{}].resources[{}]",
                            resource.slot, previous.group_index, previous.resource_index
                        ),
                    );
                }
                slot_locations.push(location);
                validate_resource(&mut report, resource, &path, canonical);
            }
        }
    }

    for (group_index, group) in manifest.groups.iter().enumerate() {
        if let Some(parent) = group.parent() {
            match groups.get(parent) {
                None => report.error(
                    ValidationCode::MissingGroupReference,
                    format!("groups[{group_index}].parent"),
                    format!("parent group {parent:?} does not exist"),
                ),
                Some((_, is_composite)) if !is_composite => report.error(
                    ValidationCode::WrongGroupReferenceKind,
                    format!("groups[{group_index}].parent"),
                    format!("parent group {parent:?} is not composite"),
                ),
                Some(_) => {}
            }
        }

        match group {
            ResourceGroup::Composite(group) => {
                for (subgroup_index, subgroup) in group.subgroups.iter().enumerate() {
                    match groups.get(subgroup.id.as_str()) {
                        None => report.error(
                            ValidationCode::MissingGroupReference,
                            format!("groups[{group_index}].subgroups[{subgroup_index}].id"),
                            format!("subgroup {:?} does not exist", subgroup.id),
                        ),
                        Some((_, true)) => report.error(
                            ValidationCode::WrongGroupReferenceKind,
                            format!("groups[{group_index}].subgroups[{subgroup_index}].id"),
                            format!("subgroup {:?} is composite rather than simple", subgroup.id),
                        ),
                        Some(_) => {}
                    }
                }
            }
            ResourceGroup::Simple(group) => {
                let local_resources: HashMap<_, _> = group
                    .resources
                    .iter()
                    .map(|resource| (resource.id.as_str(), resource))
                    .collect();
                for (resource_index, resource) in group.resources.iter().enumerate() {
                    let Some(parent) = resource.parent.as_deref() else {
                        continue;
                    };
                    let path = format!("groups[{group_index}].resources[{resource_index}].parent");
                    match local_resources.get(parent) {
                        None => report.error(
                            ValidationCode::MissingAtlasReference,
                            path,
                            format!("atlas resource {parent:?} does not exist in the same group"),
                        ),
                        Some(parent_resource)
                            if parent_resource.resource_type != ResourceType::Image
                                || !parent_resource.atlas =>
                        {
                            report.error(
                                ValidationCode::InvalidAtlasReference,
                                path,
                                format!(
                                    "parent resource {parent:?} is not an Image atlas resource"
                                ),
                            );
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }

    if canonical && slots.len() != manifest.slot_count as usize {
        report.error(
            ValidationCode::IncompleteSlotTable,
            "slot_count",
            format!(
                "canonical slot table contains {} unique resources but slot_count is {}",
                slots.len(),
                manifest.slot_count
            ),
        );
    }

    report
}

#[derive(Clone, Copy)]
struct ResourceLocation<'a> {
    group_index: usize,
    resource_index: usize,
    resolution: Option<u32>,
    parent: Option<&'a str>,
}

/// Resolution-specific children of the same composite group are alternatives:
/// PvZ2 selects one resolution at runtime, so they may intentionally reuse the
/// same resource IDs and slots. Common children (`resolution == None`) remain
/// simultaneously loadable with the selected resolution and are not exempt.
fn groups_are_mutually_exclusive(left: ResourceLocation<'_>, right: ResourceLocation<'_>) -> bool {
    left.group_index != right.group_index
        && matches!(
            (left.parent, right.parent),
            (Some(left_parent), Some(right_parent)) if left_parent == right_parent
        )
        && matches!(
            (left.resolution, right.resolution),
            (Some(left_resolution), Some(right_resolution))
                if left_resolution != right_resolution
        )
}

fn validate_resource(
    report: &mut ValidationReport,
    resource: &crate::Resource,
    path: &str,
    canonical: bool,
) {
    for (field, value) in [
        ("ax", resource.ax),
        ("ay", resource.ay),
        ("aw", resource.aw),
        ("ah", resource.ah),
    ] {
        if value.is_some_and(|value| value > u16::MAX.into()) {
            report.error(
                ValidationCode::GeometryOutOfRange,
                format!("{path}.{field}"),
                format!(
                    "{field} exceeds 16-bit storage and would be truncated by the PvZ2 runtime"
                ),
            );
        }
    }
    for (field, value) in [("cols", resource.cols), ("rows", resource.rows)] {
        if value == Some(0) {
            report.error(
                ValidationCode::InvalidGridSize,
                format!("{path}.{field}"),
                format!("{field} must be at least 1"),
            );
        }
    }

    match resource.resource_type {
        ResourceType::Image if resource.atlas => {
            if resource.parent.is_some()
                || resource.width.is_none()
                || resource.height.is_none()
                || resource.width == Some(0)
                || resource.height == Some(0)
            {
                report.error(
                    ValidationCode::InvalidAtlasImage,
                    path,
                    "atlas image must have non-zero width/height and no parent",
                );
            }
        }
        ResourceType::Image if resource.parent.is_some() => {
            if resource.atlas
                || resource.x.is_none()
                || resource.y.is_none()
                || resource.aw.is_none()
                || resource.ah.is_none()
                || resource.aw == Some(0)
                || resource.ah == Some(0)
            {
                report.error(
                    ValidationCode::InvalidSprite,
                    path,
                    "sprite image must have x/y/aw/ah, a parent, and atlas=false",
                );
            }
            if canonical && (resource.width.is_some() || resource.height.is_some()) {
                report.error(
                    ValidationCode::UnexpectedResourceMetadata,
                    path,
                    "canonical sprite image does not store width/height",
                );
            }
        }
        ResourceType::Image => {}
        _ => {
            let has_image_metadata = resource.width.is_some()
                || resource.height.is_some()
                || resource.x.is_some()
                || resource.y.is_some()
                || resource.ax.is_some()
                || resource.ay.is_some()
                || resource.aw.is_some()
                || resource.ah.is_some()
                || resource.cols.is_some()
                || resource.rows.is_some()
                || resource.atlas
                || resource.parent.is_some();
            if has_image_metadata {
                let message = "non-Image resource contains image-only metadata";
                if canonical {
                    report.error(ValidationCode::UnexpectedResourceMetadata, path, message);
                } else {
                    report.warning(ValidationCode::UnexpectedResourceMetadata, path, message);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompositeGroup, Resource, SimpleGroup, Subgroup};

    fn file_resource(slot: u32, id: &str) -> Resource {
        Resource {
            resource_type: ResourceType::File,
            slot,
            id: id.into(),
            path: id.into(),
            width: None,
            height: None,
            x: None,
            y: None,
            ax: None,
            ay: None,
            aw: None,
            ah: None,
            cols: None,
            rows: None,
            atlas: false,
            parent: None,
        }
    }

    #[test]
    fn reports_duplicate_slots_and_ids() {
        let manifest = ResourceManifest {
            slot_count: 2,
            groups: vec![ResourceGroup::Simple(SimpleGroup {
                id: "Simple".into(),
                resolution: None,
                parent: None,
                resources: vec![file_resource(0, "A"), file_resource(0, "A")],
            })],
        };
        let report = manifest.validate(ValidationProfile::Canonical);
        assert!(!report.is_valid());
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == ValidationCode::DuplicateSlot)
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == ValidationCode::DuplicateResourceId)
        );
    }

    #[test]
    fn reports_geometry_that_the_runtime_would_truncate() {
        let mut resource = file_resource(0, "SPRITE");
        resource.resource_type = ResourceType::Image;
        resource.parent = Some("ATLAS".into());
        resource.x = Some(0);
        resource.y = Some(0);
        resource.ax = Some(u16::MAX as u32 + 1);
        resource.aw = Some(1);
        resource.ah = Some(1);
        let manifest = ResourceManifest {
            slot_count: 1,
            groups: vec![ResourceGroup::Simple(SimpleGroup {
                id: "Simple".into(),
                resolution: None,
                parent: None,
                resources: vec![resource],
            })],
        };

        let report = manifest.validate(ValidationProfile::Pvz2Runtime);
        assert!(report.issues.iter().any(|issue| {
            issue.code == ValidationCode::GeometryOutOfRange
                && issue.severity == ValidationSeverity::Error
        }));
    }

    #[test]
    fn allows_ids_and_slots_reused_by_resolution_alternatives() {
        let manifest = resolution_variants(&[("Group_1536", Some(1536)), ("Group_768", Some(768))]);

        let report = manifest.validate(ValidationProfile::Canonical);

        assert!(report.is_valid(), "{:#?}", report.issues);
    }

    #[test]
    fn rejects_reuse_by_simultaneously_loadable_or_same_resolution_groups() {
        let manifest = resolution_variants(&[
            ("Group_1536_A", Some(1536)),
            ("Group_Common", None),
            ("Group_768", Some(768)),
            ("Group_1536_B", Some(1536)),
        ]);

        let report = manifest.validate(ValidationProfile::Canonical);

        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == ValidationCode::DuplicateResourceId)
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == ValidationCode::DuplicateSlot)
        );
    }

    fn resolution_variants(groups: &[(&str, Option<u32>)]) -> ResourceManifest {
        let parent = "Group";
        let mut manifest_groups = vec![ResourceGroup::Composite(CompositeGroup {
            id: parent.into(),
            resolution: None,
            parent: None,
            subgroups: groups
                .iter()
                .map(|(id, resolution)| Subgroup {
                    id: (*id).into(),
                    resolution: *resolution,
                })
                .collect(),
        })];
        manifest_groups.extend(groups.iter().map(|(id, resolution)| {
            ResourceGroup::Simple(SimpleGroup {
                id: (*id).into(),
                resolution: *resolution,
                parent: Some(parent.into()),
                resources: vec![file_resource(0, "SHARED")],
            })
        }));
        ResourceManifest {
            slot_count: 1,
            groups: manifest_groups,
        }
    }
}
