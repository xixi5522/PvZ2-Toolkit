use thiserror::Error;

/// Errors returned while reading or writing a NEWTON resource manifest.
#[derive(Debug, Error)]
pub enum NewtonError {
    #[error("NEWTON I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid NEWTON group type byte {0}")]
    InvalidGroupType(u8),

    #[error("invalid NEWTON resource type byte {0}")]
    InvalidResourceType(u8),

    #[error("invalid boolean byte {value} for {field}")]
    InvalidBoolean { field: &'static str, value: u8 },

    #[error("required NEWTON string field {field} is not present")]
    MissingRequiredString { field: &'static str },

    #[error(
        "NEWTON presence flag for {field} is {flag}, but the associated value presence is {has_value}"
    )]
    InconsistentPresence {
        field: &'static str,
        flag: u8,
        has_value: bool,
    },

    #[error("negative value {value} for unsigned NEWTON field {field}")]
    NegativeValue { field: &'static str, value: i32 },

    #[error("value {value} for NEWTON field {field} exceeds signed 32-bit storage")]
    IntegerOutOfRange { field: &'static str, value: u32 },

    #[error("NEWTON {field} count {count} exceeds configured limit {limit}")]
    CountLimitExceeded {
        field: &'static str,
        count: usize,
        limit: usize,
    },

    #[error("NEWTON string length {length} exceeds configured limit {limit}")]
    StringLimitExceeded { length: usize, limit: usize },

    #[error("NEWTON cumulative string bytes {requested} exceed configured limit {limit}")]
    TotalStringLimitExceeded { requested: usize, limit: usize },

    #[error("NEWTON estimated allocation bytes {requested} exceed configured limit {limit}")]
    AllocationLimitExceeded { requested: usize, limit: usize },

    #[error("NEWTON allocation failed for {field}: {source}")]
    AllocationFailed {
        field: &'static str,
        #[source]
        source: std::collections::TryReserveError,
    },

    #[error("NEWTON string field {field} is not valid UTF-8")]
    InvalidUtf8 { field: &'static str },

    #[error("NEWTON {field} length cannot be represented by the binary format")]
    LengthOverflow { field: &'static str },

    #[error("composite group {group:?} contains {resources} resource records")]
    CompositeContainsResources { group: String, resources: usize },

    #[error("simple group {group:?} contains {subgroups} subgroup records")]
    SimpleContainsSubgroups { group: String, subgroups: usize },

    #[error("NEWTON document has {remaining} trailing bytes")]
    TrailingData { remaining: usize },

    #[error("failed to decode NEWTON {context}.{field} at byte offset {offset:#x}: {source}")]
    DecodeContext {
        offset: u64,
        context: String,
        field: &'static str,
        #[source]
        source: Box<NewtonError>,
    },

    #[error("invalid NEWTON semantic record {context}: {source}")]
    SemanticContext {
        context: String,
        #[source]
        source: Box<NewtonError>,
    },
}

impl NewtonError {
    /// Innermost structured cause, without its decode location wrapper.
    pub fn root_cause(&self) -> &Self {
        match self {
            Self::DecodeContext { source, .. } | Self::SemanticContext { source, .. } => {
                source.root_cause()
            }
            error => error,
        }
    }
}

pub type Result<T> = std::result::Result<T, NewtonError>;
