//! Byte-preserving archive paths and safe extraction helpers.

use std::borrow::Borrow;
use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

use crate::options::PathSeparator;

#[cfg(feature = "serde")]
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{SeqAccess, Visitor},
};

/// A PAK path stored exactly as it appears on the wire.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PakPath(Vec<u8>);

impl PakPath {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }

    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn normalized(&self, separator: PathSeparator) -> Cow<'_, [u8]> {
        let (from, to) = match separator {
            PathSeparator::Preserve => return Cow::Borrowed(&self.0),
            PathSeparator::ForwardSlash => (b'\\', b'/'),
            PathSeparator::Backslash => (b'/', b'\\'),
        };
        if !self.0.contains(&from) {
            return Cow::Borrowed(&self.0);
        }
        Cow::Owned(
            self.0
                .iter()
                .map(|byte| if *byte == from { to } else { *byte })
                .collect(),
        )
    }

    /// Convert to a safe relative host path, rejecting traversal and absolute paths.
    pub fn to_safe_relative_path(&self) -> Result<PathBuf, PakPathError> {
        let text = self.as_str().map_err(|_| PakPathError::NonUtf8)?;
        if text.is_empty() {
            return Err(PakPathError::Empty);
        }
        let normalized = text.replace('\\', "/");
        if normalized.starts_with('/') || normalized.starts_with("//") {
            return Err(PakPathError::Absolute);
        }
        let mut output = PathBuf::new();
        for (index, component) in normalized.split('/').enumerate() {
            if component.is_empty() || component == "." {
                continue;
            }
            if component == ".." {
                return Err(PakPathError::Traversal);
            }
            if index == 0 && component.as_bytes().get(1) == Some(&b':') {
                return Err(PakPathError::Absolute);
            }
            if component.contains('\0') {
                return Err(PakPathError::Nul);
            }
            output.push(component);
        }
        if output.as_os_str().is_empty() {
            return Err(PakPathError::Empty);
        }
        Ok(output)
    }
}

impl fmt::Debug for PakPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PakPath")
            .field(&self.to_string_lossy())
            .finish()
    }
}

impl fmt::Display for PakPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string_lossy().fmt(formatter)
    }
}

impl AsRef<[u8]> for PakPath {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Borrow<[u8]> for PakPath {
    fn borrow(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<Vec<u8>> for PakPath {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<String> for PakPath {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

impl From<&str> for PakPath {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum PakPathError {
    #[error("archive path is empty")]
    Empty,
    #[error("archive path is not UTF-8 and cannot be mapped portably")]
    NonUtf8,
    #[error("archive path is absolute")]
    Absolute,
    #[error("archive path contains a parent-directory component")]
    Traversal,
    #[error("archive path contains a NUL byte")]
    Nul,
}

#[cfg(feature = "serde")]
impl Serialize for PakPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable()
            && let Ok(text) = self.as_str()
        {
            serializer.serialize_str(text)
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PakPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PakPathVisitor;

        impl<'de> Visitor<'de> for PakPathVisitor {
            type Value = PakPath;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a PAK path string or byte sequence")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(PakPath::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(PakPath::from(value))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
                Ok(PakPath::from(value.to_vec()))
            }

            fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
                Ok(PakPath::from(value))
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut bytes = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
                while let Some(byte) = sequence.next_element::<u8>()? {
                    bytes.push(byte);
                }
                Ok(PakPath::from(bytes))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_any(PakPathVisitor)
        } else {
            deserializer.deserialize_byte_buf(PakPathVisitor)
        }
    }
}
