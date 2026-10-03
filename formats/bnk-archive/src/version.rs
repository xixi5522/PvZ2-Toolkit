#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{BnkError, Result};

pub const SUPPORTED_VERSIONS: &[u32] = &[
    72, 88, 112, 113, 118, 120, 125, 128, 132, 134, 135, 140, 145, 150,
];

/// Wwise SoundBank format version stored in `BKHD`.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct BankVersion(u32);

impl BankVersion {
    pub const V72: Self = Self(72);
    pub const V88: Self = Self(88);
    pub const V112: Self = Self(112);
    pub const V113: Self = Self(113);
    pub const V118: Self = Self(118);
    pub const V120: Self = Self(120);
    pub const V125: Self = Self(125);
    pub const V128: Self = Self(128);
    pub const V132: Self = Self(132);
    pub const V134: Self = Self(134);
    pub const V135: Self = Self(135);
    pub const V140: Self = Self(140);
    pub const V145: Self = Self(145);
    pub const V150: Self = Self(150);

    pub const fn new_unchecked(number: u32) -> Self {
        Self(number)
    }

    pub fn new(number: u32) -> Result<Self> {
        let version = Self(number);
        version.ensure_supported()?;
        Ok(version)
    }

    pub const fn number(self) -> u32 {
        self.0
    }

    pub fn is_supported(self) -> bool {
        SUPPORTED_VERSIONS.contains(&self.0)
    }

    pub fn ensure_supported(self) -> Result<()> {
        if self.is_supported() {
            Ok(())
        } else {
            Err(BnkError::UnsupportedVersion { version: self.0 })
        }
    }

    pub const fn at_least(self, version: u32) -> bool {
        self.0 >= version
    }

    pub const fn before(self, version: u32) -> bool {
        self.0 < version
    }
}

impl From<BankVersion> for u32 {
    fn from(value: BankVersion) -> Self {
        value.number()
    }
}
