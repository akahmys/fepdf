use thiserror::Error;

/// Standard Result type for fepdf Core operations.
pub type PdfResult<T> = Result<T, PdfError>;

#[derive(Error, Debug)]
/// Every failure this engine reports.
pub enum PdfError {
    #[error("IO error: {0}")]
    /// Reading or writing the underlying file failed.
    Io(#[from] std::io::Error),

    #[error("Parse error at position {pos}: {message}")]
    /// The byte stream did not match the grammar at `pos`.
    Parse {
        /// Byte offset at which parsing failed.
        pos: usize,
        /// What was expected there.
        message: std::borrow::Cow<'static, str>,
    },

    #[error("Ingestion error in {context}: {message}")]
    /// A document could not be brought into the arena.
    Ingestion {
        /// The ingestion stage that failed.
        context: std::borrow::Cow<'static, str>,
        /// What went wrong there.
        message: std::borrow::Cow<'static, str>,
    },

    #[error("Arena handle error: {0}")]
    /// An arena handle was invalid or pointed at the wrong pool.
    Arena(std::borrow::Cow<'static, str>),

    #[error("Filter error ({filter}): {message}")]
    /// A stream filter could not decode its input.
    Filter {
        /// Name of the filter that failed, such as `FlateDecode`.
        filter: std::borrow::Cow<'static, str>,
        /// What went wrong while decoding.
        message: std::borrow::Cow<'static, str>,
    },

    #[error("Lopdf error: {0}")]
    /// The `lopdf` ingestion stage reported an error.
    Lopdf(#[from] lopdf::Error),

    #[error("Recursion depth limit exceeded: {0}")]
    /// Object resolution nested deeper than the configured limit.
    DepthLimitExceeded(usize),

    #[error("ISO 32000-2 Clause violation ({clause}): {message}")]
    /// The document violates the named ISO 32000-2 clause.
    ClauseViolation {
        /// The ISO 32000-2 clause that was violated.
        clause: &'static str,
        /// How the document violates it.
        message: std::borrow::Cow<'static, str>,
    },

    #[error("Cryptography error: {0}")]
    /// Decryption or signature handling failed.
    Crypto(std::borrow::Cow<'static, str>),

    #[error("Internal consistency error: {0}")]
    /// An invariant of this engine was broken; a bug, not bad input.
    Internal(std::borrow::Cow<'static, str>),

    #[error("Linearization hint stream overflow: data at {pos} exceeds reserved size {size}")]
    /// A linearisation hint stream exceeded the space reserved for it.
    HintStreamOverflow {
        /// Offset at which the overflow was detected.
        pos: usize,
        /// Space that had been reserved.
        size: usize,
    },

    #[error("Linearization parameter synchronization error: {parameter}")]
    /// A linearisation parameter disagreed with the written file.
    LinearizationSyncError {
        /// The parameter that disagreed with the written file.
        parameter: String,
    },

    #[error("Other error: {0}")]
    /// A failure with no more specific variant.
    Other(std::borrow::Cow<'static, str>),
}
