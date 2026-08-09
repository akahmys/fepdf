use thiserror::Error;

/// Standard Result type for fepdf Core operations.
pub type PdfResult<T> = Result<T, PdfError>;

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at position {pos}: {message}")]
    Parse { pos: usize, message: std::borrow::Cow<'static, str> },

    #[error("Ingestion error in {context}: {message}")]
    Ingestion { context: std::borrow::Cow<'static, str>, message: std::borrow::Cow<'static, str> },

    #[error("Arena handle error: {0}")]
    Arena(std::borrow::Cow<'static, str>),

    #[error("Filter error ({filter}): {message}")]
    Filter { filter: std::borrow::Cow<'static, str>, message: std::borrow::Cow<'static, str> },

    #[error("Lopdf error: {0}")]
    Lopdf(#[from] lopdf::Error),

    #[error("Recursion depth limit exceeded: {0}")]
    DepthLimitExceeded(usize),

    #[error("ISO 32000-2 Clause violation ({clause}): {message}")]
    ClauseViolation { clause: &'static str, message: std::borrow::Cow<'static, str> },

    #[error("Cryptography error: {0}")]
    Crypto(std::borrow::Cow<'static, str>),

    #[error("Internal consistency error: {0}")]
    Internal(std::borrow::Cow<'static, str>),

    #[error("Linearization hint stream overflow: data at {pos} exceeds reserved size {size}")]
    HintStreamOverflow { pos: usize, size: usize },

    #[error("Linearization parameter synchronization error: {parameter}")]
    LinearizationSyncError { parameter: String },

    #[error("Other error: {0}")]
    Other(std::borrow::Cow<'static, str>),
}
