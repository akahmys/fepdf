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

    /// The byte layer -- lexing or decryption -- failed.
    #[error("Syntax error: {0}")]
    Syntax(#[from] fepdf_syntax::SyntaxError),

    /// The operation is part of the vocabulary but has no implementation yet.
    ///
    /// Returned rather than `Ok(())` so that a caller is never told a document was
    /// changed when it was not. The payload names the operation.
    #[error("{0} is not implemented yet")]
    NotImplemented(&'static str),

    /// The caller named something this document does not have
    /// ([ADR-0102](../../../docs/adr/0102-an-error-says-whose-it-is.md)).
    #[error("{0}")]
    NotFound(Missing),

    /// The request is well formed and this document cannot take it: a run that is the
    /// last on its page has nothing to join to, a face does not draw the character, a
    /// scale of zero draws nothing.
    #[error("{why}")]
    Refused {
        /// What was asked, as the caller would name it.
        operation: &'static str,
        /// Why it cannot be done, for a person to read.
        why: std::borrow::Cow<'static, str>,
    },
}

/// What a caller named that is not there.
///
/// **The condition is in the type, not the text.** A page past the end had six spellings
/// across the engine, and only one said how many pages there were; a caller could match
/// on none of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// A page index past the end.
    Page {
        /// The index asked for.
        index: usize,
        /// How many pages the document has.
        count: usize,
    },
    /// A text run past the last on its page.
    Run {
        /// The run asked for.
        index: usize,
        /// How many runs the page has.
        count: usize,
    },
    /// A drawn object — an image or a form — past the last a page draws.
    DrawnObject {
        /// The object asked for.
        index: usize,
        /// How many the page draws.
        count: usize,
    },
    /// A form field, by its name.
    Field(String),
    /// An optional content group, by its name.
    Layer(String),
    /// A marked-content sequence, by its MCID on a page.
    Mark {
        /// The page searched.
        page: usize,
        /// The MCID asked for.
        mcid: i64,
    },
    /// A structure element, by the object number that was given for it.
    StructElement(u32),
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Page { index, count } => {
                write!(f, "this document has {count} pages and no page {index}")
            }
            Self::Run { index, count } => {
                write!(f, "this page has {count} runs and no run {index}")
            }
            Self::DrawnObject { index, count } => {
                write!(f, "this page draws {count} objects and no object {index}")
            }
            Self::Field(name) => write!(f, "the form has no field named {name:?}"),
            Self::Layer(name) => write!(f, "no optional content group is named {name:?}"),
            Self::Mark { page, mcid } => {
                write!(f, "page {page} marks no sequence with MCID {mcid}")
            }
            Self::StructElement(number) => {
                write!(f, "object {number} is not an element of the structure tree")
            }
        }
    }
}

impl PdfError {
    /// A page index past the end of a document of `count` pages.
    #[must_use]
    pub const fn no_page(index: usize, count: usize) -> Self {
        Self::NotFound(Missing::Page { index, count })
    }

    /// `operation` refused, for the reason `why`.
    pub fn refused(
        operation: &'static str,
        why: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self::Refused { operation, why: why.into() }
    }

    /// The document breaks `clause` where an operation needs it.
    pub fn violation(
        clause: &'static str,
        message: impl Into<std::borrow::Cow<'static, str>>,
    ) -> Self {
        Self::ClauseViolation { clause, message: message.into() }
    }

    /// An invariant of this engine broke.
    pub fn internal(message: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self::Internal(message.into())
    }
}

impl From<fepdf_font::FontError> for PdfError {
    fn from(err: fepdf_font::FontError) -> Self {
        // Every message `fepdf-font` carries is about the bytes of a font program: a PFB
        // segment past the end, a CFF table that is not there, an SFNT too short.
        PdfError::violation("9.9", err.to_string())
    }
}
