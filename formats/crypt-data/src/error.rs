use thiserror::Error;

pub type Result<T> = std::result::Result<T, CryptDataError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CryptDataError {
    #[error("invalid CRYPT_RES magic")]
    InvalidMagic,

    #[error("CRYPT_RES data is too short to contain its {expected}-byte header")]
    HeaderTooShort { expected: usize },

    #[error("the XOR key cannot be empty when the encrypted prefix is non-empty")]
    EmptyKey,

    #[error("the declared raw size {raw_size} is smaller than the encrypted prefix limit {limit}")]
    RawSizeBelowLimit { raw_size: usize, limit: usize },

    #[error(
        "the payload size does not match the declared raw size: expected {expected}, got {actual}"
    )]
    PayloadSizeMismatch { expected: usize, actual: usize },

    #[error("the declared raw size cannot be represented on this platform")]
    RawSizeOverflow,

    #[error("the encoded size overflows usize")]
    EncodedSizeOverflow,
}
