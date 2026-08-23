//! Acrobat's `AF*` helper functions, and what writing them in JavaScript costs.
//!
//! A form's calculate action is usually one line — `AFSimple_Calculate("SUM", ["a","b"])`
//! — and the body is not in the file. ISO 32000-2 names no `AF*` function anywhere; the
//! API is Adobe's, now ISO/DIS 21757-1, and a processor that runs form scripts has to
//! supply it.
//!
//! # Why `.js`
//!
//! They are specified in JavaScript and every other implementation writes them in it.
//! Mozilla's pdf.js carries the same API under Apache-2.0 and it was read while writing
//! ours; it could not be used directly, being an ES module exporting a class whose
//! constructor takes four host objects where Acrobat exposes globals.
//!
//! # What it costs, stated rather than implied
//!
//! **None of RR-15's fifteen checks reads that file.** Not the function-length limit, not
//! the error types, not determinism, not the `unsafe` ban — `verify_compliance.sh` runs
//! over Rust. So two things stand in for them:
//!
//! * every helper carries a test that fails when the helper is broken, in
//!   `tests/helpers_test.rs`;
//! * `status.sh` counts the lines no audit covers, so the figure cannot grow quietly.
//!
//! `docs/specs/` held twelve false claims for the want of exactly that.
//!
//! # Loaded per document, not once
//!
//! A script may redefine a helper — defining functions is what scripts do — and in a
//! shared context the redefinition reaches the next document. Measured: with one context,
//! a document that redefines `AFSimple_Calculate` leaves the next one computing 999. With
//! a fresh context per run, it does not. The cost of reloading is parsing 73 lines.

/// The helper source, compiled in so a deployment cannot lose it.
///
/// `include_str!` rather than a path read at run time: `VelloBackend::load_system_fonts`
/// spent three months reading a directory that had been renamed, because a path is only
/// checked when someone runs the code that uses it.
pub const AFORM_JS: &str = include_str!("../scripting/aform.js");

/// Lines of JavaScript no RR-15 check reads.
///
/// Counted here so `status.sh` can quote it without knowing where the file is, and so
/// that adding a helper moves a number someone sees.
#[must_use]
pub fn unaudited_lines() -> usize {
    AFORM_JS.lines().count()
}
