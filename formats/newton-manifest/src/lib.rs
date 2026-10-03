//! Reader and writer for PopCap/PvZ2 NEWTON resource manifests.
//!
//! NEWTON stores the resource groups, slots, paths, and atlas geometry used by
//! Plants vs. Zombies 2. This crate exposes a typed, Serde-ready model and
//! stream-based binary APIs without depending on the Toolkit UI. A separate
//! raw model preserves malformed or non-canonical wire data, while validation
//! profiles check references and known PvZ2 runtime constraints.
//!
//! ```
//! use newton_manifest::{
//!     CompositeGroup, ResourceGroup, ResourceManifest, Subgroup, from_bytes, to_bytes,
//! };
//!
//! let manifest = ResourceManifest {
//!     slot_count: 1,
//!     groups: vec![ResourceGroup::Composite(CompositeGroup {
//!         id: "ManifestGroup".into(),
//!         resolution: None,
//!         parent: None,
//!         subgroups: vec![Subgroup {
//!             id: "ManifestGroup_Common".into(),
//!             resolution: None,
//!         }],
//!     })],
//! };
//! let encoded = to_bytes(&manifest)?;
//! assert_eq!(from_bytes(&encoded)?, manifest);
//! # Ok::<(), newton_manifest::NewtonError>(())
//! ```

mod binary;
pub mod error;
pub mod raw;
pub mod types;
pub mod validation;

use std::io::{Read, Write};

pub use binary::{
    DecodeLimits, decode_newton, decode_raw_newton, encode_newton, encode_raw_newton, encoded_len,
    from_reader_with_limits, raw_encoded_len, raw_from_bytes_borrowed,
    raw_from_bytes_borrowed_with_limits, raw_from_reader_with_limits, validate_encoding,
};
pub use error::{NewtonError, Result};
pub use raw::*;
pub use types::*;
pub use validation::*;

/// Decode one complete NEWTON document from a byte slice.
///
/// Unlike [`decode_newton`], this convenience function rejects trailing bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<ResourceManifest> {
    from_bytes_with_limits(bytes, DecodeLimits::default())
}

/// Decode one complete NEWTON document using caller-provided allocation limits.
pub fn from_bytes_with_limits(bytes: &[u8], limits: DecodeLimits) -> Result<ResourceManifest> {
    let raw = raw_from_bytes_borrowed_with_limits(bytes, limits)?;
    raw::ensure_semantic_allocation_budget(&raw, limits.max_allocation_bytes)?;
    ResourceManifest::try_from(raw)
}

/// Decode a NEWTON document from a stream with default allocation limits.
pub fn from_reader(reader: impl Read) -> Result<ResourceManifest> {
    decode_newton(reader)
}

/// Encode a NEWTON document into a new byte vector.
pub fn to_bytes(manifest: &ResourceManifest) -> Result<Vec<u8>> {
    let length = encoded_len(manifest)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|source| NewtonError::AllocationFailed {
            field: "encoded document",
            source,
        })?;
    encode_newton(manifest, &mut output)?;
    Ok(output)
}

/// Encode a NEWTON document to a stream.
pub fn to_writer(manifest: &ResourceManifest, writer: impl Write) -> Result<()> {
    encode_newton(manifest, writer)
}

/// Decode one complete wire-faithful NEWTON document into owned byte strings.
pub fn raw_from_bytes(bytes: &[u8]) -> Result<OwnedRawResourceManifest> {
    raw_from_bytes_borrowed(bytes)?.try_into_owned()
}

/// Encode a wire-faithful NEWTON document into a new byte vector.
pub fn raw_to_bytes<B: AsRef<[u8]>>(manifest: &RawResourceManifest<B>) -> Result<Vec<u8>> {
    let length = raw_encoded_len(manifest)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|source| NewtonError::AllocationFailed {
            field: "encoded raw document",
            source,
        })?;
    encode_raw_newton(manifest, &mut output)?;
    Ok(output)
}
