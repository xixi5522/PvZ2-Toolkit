//! Internal compression engines used by the Dzip archive facade.

#[cfg(feature = "bzip")]
#[allow(dead_code)]
pub(crate) mod bzip;
#[cfg(feature = "dz")]
#[allow(dead_code)]
pub(crate) mod dz;
#[cfg(feature = "lzma")]
#[allow(dead_code)]
pub(crate) mod lzma;
#[cfg(feature = "zlib")]
#[allow(dead_code)]
pub(crate) mod zlib;
