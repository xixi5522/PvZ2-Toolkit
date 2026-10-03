//! Ordered, editable HIRC field storage and bounded scalar reader.

use std::collections::HashMap;

use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{BnkError, Result};
use crate::limits::{DecodeLimits, ValidationMode};
use crate::semantics::{
    CommonPropertyDescriptor, CommonPropertyValue, CommonPropertyValueKind, EnumerationKind,
    HierarchyFieldSemantic, PackedLayout, common_property_descriptor,
};
use crate::version::BankVersion;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HierarchyFields {
    pub fields: Vec<HierarchyField>,
}

impl HierarchyFields {
    pub fn field(&self, path: &str) -> Option<&HierarchyField> {
        self.fields.iter().find(|field| field.path == path)
    }

    pub fn field_mut(&mut self, path: &str) -> Option<&mut HierarchyField> {
        self.fields.iter_mut().find(|field| field.path == path)
    }

    pub fn get(&self, path: &str) -> Option<&HierarchyFieldValue> {
        self.field(path).map(|field| &field.value)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut HierarchyFieldValue> {
        self.field_mut(path).map(|field| &mut field.value)
    }

    /// Replace one value while preserving its wire type and semantic
    /// constraints. Use this in editors that should not be able to
    /// accidentally change the HIRC field sequence.
    pub fn replace_value(
        &mut self,
        path: &str,
        value: HierarchyFieldValue,
    ) -> Result<HierarchyFieldValue> {
        let field = self.field_mut(path).ok_or_else(|| {
            BnkError::invalid(
                "HIRC field path",
                0,
                format!("field {path:?} does not exist"),
            )
        })?;
        field.replace_value(value)
    }

    pub fn iter_prefix(&self, prefix: &str) -> impl Iterator<Item = &HierarchyField> {
        self.fields
            .iter()
            .filter(move |field| field.path.starts_with(prefix))
    }

    pub fn is_opaque(&self) -> bool {
        self.fields.len() == 1 && self.fields[0].path == "unparsed_version_payload"
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        for field in &self.fields {
            field.value.write_to(&mut output)?;
        }
        Ok(output)
    }

    /// Validate Twinning-defined fixed constants and reserved packed bits
    /// without changing the lossless encoding behavior.
    pub fn validate_semantics(&self) -> Result<()> {
        for field in &self.fields {
            field.validate_semantic()?;
        }
        Ok(())
    }

    pub fn from_opaque_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            fields: vec![HierarchyField {
                path: "unparsed_version_payload".to_owned(),
                value: HierarchyFieldValue::Bytes(bytes.into()),
                semantic: None,
            }],
        }
    }

    /// Build a temporary zero-copy path index for repeated editor lookups.
    pub fn build_index(&self) -> HierarchyFieldIndex<'_> {
        HierarchyFieldIndex::new(self)
    }
}

/// Borrowed lookup index for one immutable [`HierarchyFields`] value.
///
/// Duplicate paths are retained because permissive/future layouts may contain
/// data that cannot yet be assigned a unique semantic path.
#[derive(Debug, Clone)]
pub struct HierarchyFieldIndex<'a> {
    fields: &'a HierarchyFields,
    positions: HashMap<&'a str, Vec<usize>>,
}

impl<'a> HierarchyFieldIndex<'a> {
    pub fn new(fields: &'a HierarchyFields) -> Self {
        let mut positions = HashMap::with_capacity(fields.fields.len());
        for (index, field) in fields.fields.iter().enumerate() {
            positions
                .entry(field.path.as_str())
                .or_insert_with(Vec::new)
                .push(index);
        }
        Self { fields, positions }
    }

    pub fn positions(&self, path: &str) -> &[usize] {
        self.positions
            .get(path)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn first(&self, path: &str) -> Option<&'a HierarchyField> {
        self.fields.fields.get(*self.positions(path).first()?)
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyField {
    pub path: String,
    pub value: HierarchyFieldValue,
    /// Optional Twinning-derived interpretation. It does not participate in
    /// binary encoding, so unknown and future values remain lossless.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub semantic: Option<HierarchyFieldSemantic>,
}

impl HierarchyField {
    pub fn replace_value(&mut self, value: HierarchyFieldValue) -> Result<HierarchyFieldValue> {
        if std::mem::discriminant(&self.value) != std::mem::discriminant(&value)
            || self.value.wire_size() != value.wire_size()
        {
            return Err(BnkError::invalid(
                "HIRC field value",
                0,
                format!(
                    "{} requires wire type {} ({} bytes), found {} ({} bytes)",
                    self.path,
                    self.value.type_name(),
                    self.value.wire_size(),
                    value.type_name(),
                    value.wire_size()
                ),
            ));
        }
        let previous = std::mem::replace(&mut self.value, value);
        if let Err(error) = self.validate_semantic() {
            self.value = previous;
            return Err(error);
        }
        Ok(previous)
    }

    pub fn validate_semantic(&self) -> Result<()> {
        let Some(semantic) = self.semantic else {
            return Ok(());
        };
        let Some(raw) = self.value.unsigned_raw() else {
            return Ok(());
        };
        match semantic {
            HierarchyFieldSemantic::Constant { expected } if raw != expected => {
                Err(BnkError::invalid(
                    "HIRC fixed constant",
                    0,
                    format!("{} must be {expected}, found {raw}", self.path),
                ))
            }
            HierarchyFieldSemantic::MaskedConstant { mask, expected }
                if raw & mask != expected & mask =>
            {
                Err(BnkError::invalid(
                    "HIRC masked constant",
                    0,
                    format!(
                        "{} masked by 0x{mask:x} must be 0x{:x}, found 0x{:x}",
                        self.path,
                        expected & mask,
                        raw & mask
                    ),
                ))
            }
            HierarchyFieldSemantic::Packed { layout } => {
                validate_packed_value(layout, raw, &self.path, 0)
            }
            HierarchyFieldSemantic::Enumeration {
                enumeration,
                version,
            } => {
                let variants = enumeration.variants(version);
                if !variants.is_empty()
                    && !variants
                        .iter()
                        .any(|variant| u64::from(variant.value) == raw)
                {
                    return Err(BnkError::invalid(
                        "HIRC enumeration",
                        0,
                        format!("{} has unknown {enumeration:?} value {raw}", self.path),
                    ));
                }
                Ok(())
            }
            HierarchyFieldSemantic::CommonProperty {
                domain,
                version,
                property_id,
            } => {
                let Some(descriptor) = common_property_descriptor(version, domain, property_id)
                else {
                    return Ok(());
                };
                match descriptor.kind {
                    CommonPropertyValueKind::Boolean if raw > 1 => Err(BnkError::invalid(
                        "HIRC common property",
                        0,
                        format!("{} has non-boolean value {raw}", self.path),
                    )),
                    CommonPropertyValueKind::Enumerated => {
                        let Some(enumeration) = descriptor.enumeration else {
                            return Err(BnkError::invalid(
                                "HIRC common property",
                                0,
                                format!(
                                    "{} is enumerated but has no Twinning enumeration",
                                    self.path
                                ),
                            ));
                        };
                        if enumeration.variant(version, raw as u32).is_none() {
                            Err(BnkError::invalid(
                                "HIRC common property",
                                0,
                                format!("{} has unknown {enumeration:?} value {raw}", self.path),
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    _ => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }

    pub fn common_property_descriptor(&self) -> Option<CommonPropertyDescriptor> {
        let HierarchyFieldSemantic::CommonProperty {
            domain,
            version,
            property_id,
        } = self.semantic?
        else {
            return None;
        };
        common_property_descriptor(version, domain, property_id)
    }

    pub fn common_property_value(&self) -> Option<CommonPropertyValue> {
        let descriptor = self.common_property_descriptor()?;
        let raw = self.value.property_raw()?;
        Some(match descriptor.kind {
            CommonPropertyValueKind::Boolean => CommonPropertyValue::Boolean(raw != 0),
            CommonPropertyValueKind::Integer => CommonPropertyValue::Integer(raw as i32),
            CommonPropertyValueKind::Floater => CommonPropertyValue::Floater(f32::from_bits(raw)),
            CommonPropertyValueKind::Enumerated => CommonPropertyValue::Enumerated(raw),
            CommonPropertyValueKind::Identifier => CommonPropertyValue::Identifier(raw),
        })
    }

    pub fn common_property_enumeration_variant(
        &self,
    ) -> Option<crate::semantics::EnumerationVariant> {
        let descriptor = self.common_property_descriptor()?;
        let enumeration = descriptor.enumeration?;
        let raw = self.value.property_raw()?;
        let HierarchyFieldSemantic::CommonProperty { version, .. } = self.semantic? else {
            return None;
        };
        enumeration.variant(version, raw)
    }

    /// Set a common property only when the supplied value kind matches its
    /// version-resolved Twinning descriptor.
    pub fn set_common_property_value(&mut self, value: CommonPropertyValue) -> bool {
        let Some(descriptor) = self.common_property_descriptor() else {
            return false;
        };
        let HierarchyFieldSemantic::CommonProperty { version, .. } = self
            .semantic
            .expect("descriptor requires common-property semantic")
        else {
            unreachable!("descriptor requires common-property semantic");
        };
        let raw = match (descriptor.kind, value) {
            (CommonPropertyValueKind::Boolean, CommonPropertyValue::Boolean(value)) => {
                u32::from(value)
            }
            (CommonPropertyValueKind::Integer, CommonPropertyValue::Integer(value)) => value as u32,
            (CommonPropertyValueKind::Floater, CommonPropertyValue::Floater(value)) => {
                value.to_bits()
            }
            (CommonPropertyValueKind::Enumerated, CommonPropertyValue::Enumerated(value)) => {
                if descriptor
                    .enumeration
                    .is_none_or(|enumeration| enumeration.variant(version, value).is_none())
                {
                    return false;
                }
                value
            }
            (CommonPropertyValueKind::Identifier, CommonPropertyValue::Identifier(value)) => value,
            _ => return false,
        };
        match &mut self.value {
            HierarchyFieldValue::PropertyValue(current) => {
                *current = raw;
                true
            }
            _ => false,
        }
    }

    pub fn packed_member(&self, name: &str) -> Option<u64> {
        let HierarchyFieldSemantic::Packed { layout } = self.semantic? else {
            return None;
        };
        let member = layout.member(name)?;
        Some(member.get(self.value.unsigned_raw()?))
    }

    pub fn packed_member_enumeration_variant(
        &self,
        name: &str,
        version: BankVersion,
    ) -> Option<crate::semantics::EnumerationVariant> {
        let HierarchyFieldSemantic::Packed { layout } = self.semantic? else {
            return None;
        };
        let enumeration = layout.member_enumeration(name, version.number())?;
        let value = u32::try_from(self.packed_member(name)?).ok()?;
        enumeration.variant(version.number(), value)
    }

    /// Update one named packed member without affecting any other or reserved
    /// bits. Returns `false` if this field/layout/member does not match.
    pub fn set_packed_member(&mut self, name: &str, value: u64) -> bool {
        let Some(HierarchyFieldSemantic::Packed { layout }) = self.semantic else {
            return false;
        };
        let Some(member) = layout.member(name) else {
            return false;
        };
        if value >= (1_u64 << member.bit_width) {
            return false;
        }
        let Some(raw) = self.value.unsigned_raw() else {
            return false;
        };
        self.value.set_unsigned_raw(member.set(raw, value))
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum HierarchyFieldValue {
    U8(u8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    F64(f64),
    Identifier(u32),
    /// Four-byte `AkPropValue` union. Use the accessors below according to
    /// the accompanying property identifier.
    PropertyValue(u32),
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    Bytes(Vec<u8>),
}

impl HierarchyFieldValue {
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::U8(_) => "u8",
            Self::U16(_) => "u16",
            Self::I16(_) => "i16",
            Self::U32(_) => "u32",
            Self::I32(_) => "i32",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::Identifier(_) => "identifier",
            Self::PropertyValue(_) => "property_value",
            Self::Bytes(_) => "bytes",
        }
    }

    pub fn wire_size(&self) -> usize {
        match self {
            Self::U8(_) => 1,
            Self::U16(_) | Self::I16(_) => 2,
            Self::U32(_)
            | Self::I32(_)
            | Self::F32(_)
            | Self::Identifier(_)
            | Self::PropertyValue(_) => 4,
            Self::F64(_) => 8,
            Self::Bytes(value) => value.len(),
        }
    }

    fn unsigned_raw(&self) -> Option<u64> {
        match self {
            Self::U8(value) => Some((*value).into()),
            Self::U16(value) => Some((*value).into()),
            Self::U32(value) | Self::Identifier(value) | Self::PropertyValue(value) => {
                Some((*value).into())
            }
            _ => None,
        }
    }

    fn set_unsigned_raw(&mut self, value: u64) -> bool {
        match self {
            Self::U8(raw) if u8::try_from(value).is_ok() => {
                *raw = value as u8;
                true
            }
            Self::U16(raw) if u16::try_from(value).is_ok() => {
                *raw = value as u16;
                true
            }
            Self::U32(raw) | Self::Identifier(raw) | Self::PropertyValue(raw)
                if u32::try_from(value).is_ok() =>
            {
                *raw = value as u32;
                true
            }
            _ => false,
        }
    }

    fn write_to(&self, output: &mut Vec<u8>) -> Result<()> {
        match self {
            Self::U8(value) => output.push(*value),
            Self::U16(value) => output.write_u16::<LittleEndian>(*value).expect("Vec write"),
            Self::I16(value) => output.write_i16::<LittleEndian>(*value).expect("Vec write"),
            Self::U32(value) | Self::Identifier(value) | Self::PropertyValue(value) => {
                output.write_u32::<LittleEndian>(*value).expect("Vec write")
            }
            Self::I32(value) => output.write_i32::<LittleEndian>(*value).expect("Vec write"),
            Self::F32(value) => output
                .write_u32::<LittleEndian>(value.to_bits())
                .expect("Vec write"),
            Self::F64(value) => output
                .write_u64::<LittleEndian>(value.to_bits())
                .expect("Vec write"),
            Self::Bytes(value) => output.extend_from_slice(value),
        }
        Ok(())
    }
}

impl HierarchyFieldValue {
    pub fn property_as_f32(&self) -> Option<f32> {
        match self {
            Self::PropertyValue(value) => Some(f32::from_bits(*value)),
            _ => None,
        }
    }

    pub fn property_as_i32(&self) -> Option<i32> {
        match self {
            Self::PropertyValue(value) => Some(*value as i32),
            _ => None,
        }
    }

    pub fn property_as_bool(&self) -> Option<bool> {
        match self {
            Self::PropertyValue(0) => Some(false),
            Self::PropertyValue(1) => Some(true),
            _ => None,
        }
    }

    pub fn property_as_identifier(&self) -> Option<u32> {
        match self {
            Self::PropertyValue(value) => Some(*value),
            _ => None,
        }
    }

    pub fn property_raw(&self) -> Option<u32> {
        match self {
            Self::PropertyValue(value) => Some(*value),
            _ => None,
        }
    }

    pub fn set_property_f32(&mut self, value: f32) -> bool {
        match self {
            Self::PropertyValue(raw) => {
                *raw = value.to_bits();
                true
            }
            _ => false,
        }
    }

    pub fn set_property_i32(&mut self, value: i32) -> bool {
        match self {
            Self::PropertyValue(raw) => {
                *raw = value as u32;
                true
            }
            _ => false,
        }
    }

    pub fn set_property_identifier(&mut self, value: u32) -> bool {
        match self {
            Self::PropertyValue(raw) => {
                *raw = value;
                true
            }
            _ => false,
        }
    }
}

pub(crate) struct FieldReader<'a> {
    bytes: &'a [u8],
    position: usize,
    base_offset: usize,
    fields: Vec<HierarchyField>,
    pub(super) validation: ValidationMode,
    limits: DecodeLimits,
    path_bytes: usize,
}

impl<'a> FieldReader<'a> {
    pub(crate) fn new(
        bytes: &'a [u8],
        base_offset: usize,
        validation: ValidationMode,
        limits: DecodeLimits,
    ) -> Self {
        Self {
            bytes,
            position: 0,
            base_offset,
            fields: Vec::new(),
            validation,
            limits,
            path_bytes: 0,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    pub(crate) fn offset(&self) -> usize {
        self.base_offset + self.position
    }

    pub(crate) fn require(
        &self,
        condition: bool,
        context: &'static str,
        message: impl Into<String>,
    ) -> Result<()> {
        if self.validation.is_strict() && !condition {
            return Err(BnkError::invalid(context, self.offset(), message));
        }
        Ok(())
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

    fn push(&mut self, path: impl Into<String>, value: HierarchyFieldValue) -> Result<()> {
        self.push_semantic(path, value, None)
    }

    fn push_semantic(
        &mut self,
        path: impl Into<String>,
        value: HierarchyFieldValue,
        semantic: Option<HierarchyFieldSemantic>,
    ) -> Result<()> {
        if self.fields.len() >= self.limits.max_hierarchy_fields {
            return Err(BnkError::LimitExceeded {
                resource: "HIRC fields",
                requested: self.fields.len() as u64 + 1,
                limit: self.limits.max_hierarchy_fields as u64,
            });
        }
        let path = path.into();
        let requested_path_bytes =
            self.path_bytes
                .checked_add(path.len())
                .ok_or(BnkError::IntegerOverflow {
                    context: "HIRC field path bytes",
                })?;
        if requested_path_bytes > self.limits.max_hierarchy_path_bytes {
            return Err(BnkError::LimitExceeded {
                resource: "HIRC field path bytes",
                requested: requested_path_bytes as u64,
                limit: self.limits.max_hierarchy_path_bytes as u64,
            });
        }
        self.path_bytes = requested_path_bytes;
        self.fields.push(HierarchyField {
            path,
            value,
            semantic,
        });
        Ok(())
    }

    pub(crate) fn u8(&mut self, path: impl Into<String>) -> Result<u8> {
        self.u8_semantic(path, None)
    }

    pub(crate) fn u8_semantic(
        &mut self,
        path: impl Into<String>,
        semantic: Option<HierarchyFieldSemantic>,
    ) -> Result<u8> {
        let path = path.into();
        let value = self.take(1, "HIRC u8")?[0];
        self.push_semantic(path, HierarchyFieldValue::U8(value), semantic)?;
        Ok(value)
    }

    pub(crate) fn packed_u8(
        &mut self,
        path: impl Into<String>,
        layout: PackedLayout,
    ) -> Result<u8> {
        let path = path.into();
        let offset = self.offset();
        let value = self.u8_semantic(
            path.clone(),
            Some(HierarchyFieldSemantic::Packed { layout }),
        )?;
        if self.validation.is_strict() {
            validate_packed_value(layout, value.into(), &path, offset)?;
        }
        Ok(value)
    }

    pub(crate) fn enum_u8(
        &mut self,
        path: impl Into<String>,
        enumeration: EnumerationKind,
        version: BankVersion,
    ) -> Result<u8> {
        let path = path.into();
        let offset = self.offset();
        let value = self.u8_semantic(
            path.clone(),
            Some(HierarchyFieldSemantic::Enumeration {
                enumeration,
                version: version.number(),
            }),
        )?;
        self.validate_enumeration(offset, &path, enumeration, version, value.into())?;
        Ok(value)
    }

    pub(crate) fn u16(&mut self, path: impl Into<String>) -> Result<u16> {
        self.u16_semantic(path, None)
    }

    pub(crate) fn i16(&mut self, path: impl Into<String>) -> Result<i16> {
        let path = path.into();
        let value = LittleEndian::read_i16(self.take(2, "HIRC i16")?);
        self.push(path, HierarchyFieldValue::I16(value))?;
        Ok(value)
    }

    fn u16_semantic(
        &mut self,
        path: impl Into<String>,
        semantic: Option<HierarchyFieldSemantic>,
    ) -> Result<u16> {
        let path = path.into();
        let value = LittleEndian::read_u16(self.take(2, "HIRC u16")?);
        self.push_semantic(path, HierarchyFieldValue::U16(value), semantic)?;
        Ok(value)
    }

    pub(crate) fn u32(&mut self, path: impl Into<String>) -> Result<u32> {
        self.u32_semantic(path, None)
    }

    fn u32_semantic(
        &mut self,
        path: impl Into<String>,
        semantic: Option<HierarchyFieldSemantic>,
    ) -> Result<u32> {
        let path = path.into();
        let value = LittleEndian::read_u32(self.take(4, "HIRC u32")?);
        self.push_semantic(path, HierarchyFieldValue::U32(value), semantic)?;
        Ok(value)
    }

    pub(crate) fn enum_u16(
        &mut self,
        path: impl Into<String>,
        enumeration: EnumerationKind,
        version: BankVersion,
    ) -> Result<u16> {
        let path = path.into();
        let offset = self.offset();
        let value = self.u16_semantic(
            path.clone(),
            Some(HierarchyFieldSemantic::Enumeration {
                enumeration,
                version: version.number(),
            }),
        )?;
        self.validate_enumeration(offset, &path, enumeration, version, value.into())?;
        Ok(value)
    }

    pub(crate) fn enum_u32(
        &mut self,
        path: impl Into<String>,
        enumeration: EnumerationKind,
        version: BankVersion,
    ) -> Result<u32> {
        let path = path.into();
        let offset = self.offset();
        let value = self.u32_semantic(
            path.clone(),
            Some(HierarchyFieldSemantic::Enumeration {
                enumeration,
                version: version.number(),
            }),
        )?;
        self.validate_enumeration(offset, &path, enumeration, version, value.into())?;
        Ok(value)
    }

    pub(crate) fn packed_u32(
        &mut self,
        path: impl Into<String>,
        layout: PackedLayout,
    ) -> Result<u32> {
        let path = path.into();
        let offset = self.offset();
        let value = self.u32_semantic(
            path.clone(),
            Some(HierarchyFieldSemantic::Packed { layout }),
        )?;
        if self.validation.is_strict() {
            validate_packed_value(layout, value.into(), &path, offset)?;
        }
        Ok(value)
    }

    pub(crate) fn constant_u8(&mut self, path: impl Into<String>, expected: u8) -> Result<u8> {
        let path = path.into();
        let offset = self.offset();
        let value = self.take(1, "HIRC constant u8")?[0];
        self.push_semantic(
            path,
            HierarchyFieldValue::U8(value),
            Some(HierarchyFieldSemantic::Constant {
                expected: expected.into(),
            }),
        )?;
        self.validate_constant(offset, value.into(), expected.into())?;
        Ok(value)
    }

    pub(crate) fn constant_u16(&mut self, path: impl Into<String>, expected: u16) -> Result<u16> {
        let path = path.into();
        let offset = self.offset();
        let value = LittleEndian::read_u16(self.take(2, "HIRC constant u16")?);
        self.push_semantic(
            path,
            HierarchyFieldValue::U16(value),
            Some(HierarchyFieldSemantic::Constant {
                expected: expected.into(),
            }),
        )?;
        self.validate_constant(offset, value.into(), expected.into())?;
        Ok(value)
    }

    pub(crate) fn constant_u32(&mut self, path: impl Into<String>, expected: u32) -> Result<u32> {
        let path = path.into();
        let offset = self.offset();
        let value = LittleEndian::read_u32(self.take(4, "HIRC constant u32")?);
        self.push_semantic(
            path,
            HierarchyFieldValue::U32(value),
            Some(HierarchyFieldSemantic::Constant {
                expected: expected.into(),
            }),
        )?;
        self.validate_constant(offset, value.into(), expected.into())?;
        Ok(value)
    }

    fn validate_constant(&self, offset: usize, actual: u64, expected: u64) -> Result<()> {
        if self.validation.is_strict() && actual != expected {
            return Err(BnkError::invalid(
                "HIRC fixed constant",
                offset,
                format!("expected {expected}, found {actual}"),
            ));
        }
        Ok(())
    }

    pub(super) fn validate_enumeration(
        &self,
        offset: usize,
        path: &str,
        enumeration: EnumerationKind,
        version: BankVersion,
        value: u64,
    ) -> Result<()> {
        let variants = enumeration.variants(version.number());
        if self.validation.is_strict()
            && !variants.is_empty()
            && !variants
                .iter()
                .any(|variant| u64::from(variant.value) == value)
        {
            return Err(BnkError::invalid(
                "HIRC enumeration",
                offset,
                format!("{path} has unknown {enumeration:?} value {value}"),
            ));
        }
        Ok(())
    }

    pub(crate) fn property_semantic(
        &mut self,
        path: impl Into<String>,
        semantic: Option<HierarchyFieldSemantic>,
    ) -> Result<u32> {
        let path = path.into();
        let value = LittleEndian::read_u32(self.take(4, "HIRC property value")?);
        self.push_semantic(path, HierarchyFieldValue::PropertyValue(value), semantic)?;
        Ok(value)
    }

    pub(crate) fn i32(&mut self, path: impl Into<String>) -> Result<i32> {
        let path = path.into();
        let value = LittleEndian::read_i32(self.take(4, "HIRC i32")?);
        self.push(path, HierarchyFieldValue::I32(value))?;
        Ok(value)
    }

    pub(crate) fn f32(&mut self, path: impl Into<String>) -> Result<f32> {
        let path = path.into();
        let value = f32::from_bits(LittleEndian::read_u32(self.take(4, "HIRC f32")?));
        self.push(path, HierarchyFieldValue::F32(value))?;
        Ok(value)
    }

    pub(crate) fn f64(&mut self, path: impl Into<String>) -> Result<f64> {
        let path = path.into();
        let value = f64::from_bits(LittleEndian::read_u64(self.take(8, "HIRC f64")?));
        self.push(path, HierarchyFieldValue::F64(value))?;
        Ok(value)
    }

    pub(crate) fn id(&mut self, path: impl Into<String>) -> Result<u32> {
        let path = path.into();
        let value = LittleEndian::read_u32(self.take(4, "HIRC identifier")?);
        self.push(path, HierarchyFieldValue::Identifier(value))?;
        Ok(value)
    }

    pub(crate) fn bytes(&mut self, path: impl Into<String>, length: usize) -> Result<Vec<u8>> {
        let path = path.into();
        let value = self.take(length, "HIRC bytes")?.to_vec();
        self.push(path, HierarchyFieldValue::Bytes(value.clone()))?;
        Ok(value)
    }

    pub(crate) fn finish(self) -> Result<HierarchyFields> {
        if self.position != self.bytes.len() {
            return Err(BnkError::invalid(
                "HIRC structured payload",
                self.offset(),
                format!("{} trailing bytes", self.remaining()),
            ));
        }
        Ok(HierarchyFields {
            fields: self.fields,
        })
    }
}

fn validate_packed_value(layout: PackedLayout, raw: u64, path: &str, offset: usize) -> Result<()> {
    let invalid = raw & (!layout.used_mask() | layout.reserved_mask());
    if !layout.is_canonical(raw) {
        return Err(BnkError::invalid(
            "HIRC packed field",
            offset,
            format!("{path} has non-canonical value 0x{raw:02x} (reserved bits 0x{invalid:02x})"),
        ));
    }
    Ok(())
}

pub(super) fn indexed(prefix: &str, index: usize, field: &str) -> String {
    format!("{prefix}[{index}].{field}")
}
