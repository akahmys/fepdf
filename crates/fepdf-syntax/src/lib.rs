//! The byte layer: turning PDF bytes into tokens, and decrypting them.
//!
//! Everything here works on bytes alone. It has no knowledge of the object model,
//! which is what allows the cryptography in [`security`] to be reviewed without
//! reasoning about document structure, and the lexer to be tested against raw input.
//!
//! Parsing and stream filters are deliberately *not* here: both resolve their inputs
//! through the arena — a filter reads its own decode parameters from a PDF dictionary
//! — so they belong with the model. See `ARCHITECTURE.md` §4.

pub mod lexer;
pub mod security;
pub mod xref;

use thiserror::Error;

/// A failure at the byte layer.
#[derive(Error, Debug)]
pub enum SyntaxError {
    /// Decryption failed, or the encryption dictionary could not be honoured.
    #[error("Crypto error: {0}")]
    Crypto(std::borrow::Cow<'static, str>),
}

/// Result type for the byte layer.
pub type SyntaxResult<T> = Result<T, SyntaxError>;
