//! PopCap particle and trail binary/XML encoder and decoder.
//!
//! The crate supports PC, 32-bit mobile and 64-bit mobile particle layouts,
//! the PopCap zlib wrapper, compact XML track syntax and compiled trails.

#![forbid(unsafe_code)]

mod particles;

pub use particles::*;
