use thiserror::Error;

pub type Result<T> = std::result::Result<T, CompiledTextError>;

#[derive(Debug, Error)]
pub enum CompiledTextError {
    #[error("Base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("Rijndael cipher failed: {0}")]
    Cipher(String),
    #[error("decoded Base64 contains no ciphertext")]
    EmptyCiphertext,
    #[error("ciphertext size {actual} is not aligned to the {block_size}-byte Rijndael block size")]
    CiphertextAlignment { actual: usize, block_size: usize },
    #[error("declared output size {declared} exceeds the configured limit {limit}")]
    OutputLimitExceeded { declared: u64, limit: u64 },
    #[error(transparent)]
    Container(#[from] smf_container::SmfError),
}
