use thiserror::Error;

#[derive(Error, Debug)]
pub enum RsbError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid magic: expected {0}, found {1}")]
    InvalidMagic(String, String),
    #[error("Invalid version: {0}")]
    InvalidVersion(u32),
    #[error("Invalid compression flag: {0}")]
    InvalidCompression(u32),
    #[error("Missing Part1 metadata for {0}")]
    MissingPart1Info(String),
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Zlib compression level must be between 0 and 9, found {level}")]
    InvalidZlibLevel { level: u32 },
    #[error("Zlib compression failed: {source}")]
    ZlibCompression {
        #[source]
        source: std::io::Error,
    },
    #[error("Zlib decompression failed: {source}")]
    ZlibDecompression {
        #[source]
        source: std::io::Error,
    },
    #[error("Zlib output size mismatch: expected {expected} bytes, found {actual}")]
    ZlibSizeMismatch { expected: usize, actual: usize },
    #[error("Other: {0}")]
    Other(String),
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    #[error("Packet entry {path} points outside its data section")]
    PacketDataOutOfBounds { path: String },
    #[error("Invalid ASTC block footprint: {width}x{height}")]
    InvalidAstcBlockSize { width: u32, height: u32 },
    #[error("ASTC quality must be between 0 and 100, found {0}")]
    InvalidAstcQuality(u8),
    #[error("ASTC data size mismatch: expected {expected} bytes, found {actual}")]
    InvalidAstcDataSize { expected: usize, actual: usize },
    #[error("ASTC codec error: {0}")]
    Astc(String),
}

pub type Result<T> = std::result::Result<T, RsbError>;
