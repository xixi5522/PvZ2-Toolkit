//! Version-aware semantic catalog for Wwise hierarchy fields.
//!
//! The lossless wire model is supplemented by common-property, packed-bit,
//! and enumeration catalogs used by editors and strict validation.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

mod enumerations;
mod packed;
mod properties;

pub use enumerations::{EnumerationKind, EnumerationVariant};
pub use packed::{PackedLayout, PackedMember};
pub use properties::{
    CommonPropertyDefault, CommonPropertyDescriptor, CommonPropertyDomain, CommonPropertyValue,
    CommonPropertyValueKind, common_property_descriptor,
};

/// Semantic information attached to a lossless hierarchy field.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum HierarchyFieldSemantic {
    CommonProperty {
        domain: CommonPropertyDomain,
        version: u32,
        property_id: u8,
    },
    Packed {
        layout: PackedLayout,
    },
    Enumeration {
        enumeration: EnumerationKind,
        version: u32,
    },
    Constant {
        expected: u64,
    },
    MaskedConstant {
        mask: u64,
        expected: u64,
    },
}
