//! The fifth frontend: ECMAScript, translated into `Operation` (ISO 32000-2 12.6.4.16).
//!
//! `fepdf-cli` translates argv, `fepdf-gui` a button press, `fepdf-mcp` a tool call, and
//! this crate translates `this.getField("x").value = 3`. It holds no arena, defines no
//! mutation type of its own, and is not on the path of any other caller
//! ([ADR-0025](../../../docs/adr/0025-a-script-processor-is-a-frontend-not-a-subsystem.md)).
//!
//! **What a script may do is what the API may do.** That is not a narrowing invented for
//! scripts; it is the bound every frontend already has, and it grows when the API grows.
//!
//! # What the measurement changed
//!
//! ADR-0025 named one unverified risk: whether `&mut Document` sits in a
//! `boa_engine::Context`. It does not, and it does not need to. boa's capture signature is
//! `Fn(&JsValue, &[JsValue], &T, &mut Context)` — the capture arrives by **shared**
//! reference — and `T: Trace + 'static`. So the document is reached through a
//! [`DocumentHandle`], and operations still apply *during* the run, which is what the
//! Keystroke → Validate → Calculate → Format cascade actually requires.
//!
//! # The `unsafe` this crate cannot avoid
//!
//! `boa_gc::Trace` is an **unsafe trait**, so `#[derive(Trace, Finalize)]` emits an
//! `unsafe impl`. RR-15 Rule 3 forbids that, and neither guard sees it:
//! `#![forbid(unsafe_code)]` compiles the derive, and Rule 3's check searches for an
//! unsafe *block*, which an `unsafe impl` is not. It is written down here rather than
//! left to be discovered, and `#[unsafe_ignore_trace]` is at least visible in the source.
//!
//! The check is textual in the other direction too: this paragraph tripped it by quoting
//! the pattern it searches for.

#![forbid(unsafe_code)]

mod host;

pub use host::{DocumentHandle, ScriptEnvironment, ScriptError, ScriptHost, ScriptOutcome};

mod calculate;

pub use calculate::{CalculationReport, run_calculations};
