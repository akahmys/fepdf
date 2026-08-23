//! The locale-sensitive methods, which this engine does not have and used to pretend to.
//!
//! ECMA-402 is the internationalisation half of the language a document script is written
//! in, and a form that formats currency reaches it. boa can build it — the `intl` feature
//! — and this engine does not, so the question is only what the absence looks like from
//! inside a script. **Measured, it looked like three different things:**
//!
//! | called | answered |
//! |---|---|
//! | `new Intl.NumberFormat('de-DE')` | `ReferenceError: Intl is not defined` |
//! | `new Date(0).toLocaleDateString('de-DE')` | `Function Unimplemented` |
//! | `(1234567.891).toLocaleString('de-DE')` | `"1234567.891"` |
//!
//! The third is the defect. `toLocaleString` lives on `Number.prototype` and exists with
//! or without ECMA-402, so it **took the locale, ignored it, and returned success**: a
//! German invoice reading `1234567.891` where `1.234.567,891` was asked for, and a script
//! that believes it formatted the number.
//!
//! ECMA-262 permits the ignoring — §21.1.3.4 calls the result implementation-dependent
//! and allows returning what `toString` returns. **That is why this is recorded rather
//! than reported as a bug in boa.** What no clause permits is the engine knowing it
//! answered a different question than the one asked and saying nothing (Rule 20).
//!
//! # What replaces them
//!
//! * **A locale argument** — the script named a locale this engine does not carry.
//!   Unlocalised digits answer a different question, so the call is raised into the
//!   script and a `Violation` records what was asked for. `Array.prototype.toLocaleString`
//!   is covered by this without being touched, because it delegates to each element's.
//! * **No locale argument** — the script asked for *this host's default*, and this host's
//!   default really is unlocalised digits. The answer is true, so it is returned, and an
//!   `Ambiguity` records which reading was taken — **once per script execution**, not once
//!   per call, because a loop formatting a column would otherwise write a thousand
//!   identical decisions. Not once per *document*: `run_calculations` builds a fresh
//!   context per field per pass, so a form whose calculation order takes two passes over
//!   two formatting fields records four. That is the shape 12.6.3's own `Violation`s
//!   already have, and bounding it further would mean scanning the log on every call.
//! * `Date`'s three threw already. They still throw, now with a sentence naming the
//!   clause instead of `Function Unimplemented`, and they record. There is no unlocalised
//!   date to fall back to the way there is an unlocalised number.
//!
//! # What is measured and left
//!
//! Three more methods ignore a locale silently, and are left alone because collation and
//! Turkish casing are not what a form's calculate action does. Measured, and pinned by
//! `locale_test.rs` so the gap stays a fact rather than an assumption:
//!
//! | called | answers | a viewer with ECMA-402 |
//! |---|---|---|
//! | `'ä'.localeCompare('z', 'de')` | `1` | `-1` — German sorts `ä` with `a` |
//! | `'i'.toLocaleUpperCase('tr')` | `"I"` | `"İ"` |
//! | `'I'.toLocaleLowerCase('tr')` | `"i"` | `"ı"` |
//!
//! The day a corpus script sorts or upper-cases by locale, this is the file.

use crate::host::{DocumentHandle, ScriptError};
use boa_engine::{Context, JsResult, JsValue, NativeFunction, js_string};
use std::cell::Cell;
use std::rc::Rc;

/// The clause that puts ECMAScript inside a PDF, and therefore governs its absence.
const CLAUSE: &str = "12.6.4.16";

/// The methods that fall back to an unlocalised answer, by builtin and qualified name.
const FORMATTERS: [(&str, &str, &str); 2] = [
    ("Number", "toLocaleString", "Number.prototype.toLocaleString"),
    ("BigInt", "toLocaleString", "BigInt.prototype.toLocaleString"),
];

/// The methods that have no unlocalised answer to fall back to.
const DATES: [(&str, &str); 3] = [
    ("toLocaleString", "Date.prototype.toLocaleString"),
    ("toLocaleDateString", "Date.prototype.toLocaleDateString"),
    ("toLocaleTimeString", "Date.prototype.toLocaleTimeString"),
];

/// Replaces every locale-sensitive method this engine cannot honour.
///
/// Installed per context, before the document's script runs, so the script sees these and
/// not the originals.
pub fn install(handle: &DocumentHandle, context: &mut Context) -> Result<(), ScriptError> {
    for (builtin, method, qualified) in FORMATTERS {
        set_method(context, builtin, method, formatter(Method::new(handle, qualified)))?;
    }
    for (method, qualified) in DATES {
        set_method(context, "Date", method, refuser(Method::new(handle, qualified)))?;
    }
    Ok(())
}

/// `Number` and `BigInt`: the plain digits when no locale was named, an error when one
/// was.
fn formatter(method: Method) -> NativeFunction {
    NativeFunction::from_copy_closure_with_captures(
        |this: &JsValue,
         args: &[JsValue],
         method: &Method,
         ctx: &mut Context|
         -> JsResult<JsValue> {
            // `to_string` and not `to_number`, so the same closure serves a BigInt, whose
            // value does not survive an f64.
            let plain = this.to_string(ctx)?;
            let Some(asked) = requested_locale(args, ctx)? else {
                method.record(Ask::Default);
                return Ok(plain.into());
            };
            method.record(Ask::Locale(&asked));
            Err(method.refusal(&asked))
        },
        method,
    )
}

/// `Date`'s three, which have nothing unlocalised to answer with.
fn refuser(method: Method) -> NativeFunction {
    NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue,
         args: &[JsValue],
         method: &Method,
         ctx: &mut Context|
         -> JsResult<JsValue> {
            let asked = requested_locale(args, ctx)?.unwrap_or_else(|| "the host default".into());
            method.record(Ask::Locale(&asked));
            Err(method.refusal(&asked))
        },
        method,
    )
}

/// The locale a call named, or `None` when it named none.
///
/// `undefined` counts as none: that is what an omitted argument becomes when a wrapper
/// forwards it, and ECMA-402 treats the two the same.
fn requested_locale(args: &[JsValue], ctx: &mut Context) -> JsResult<Option<String>> {
    let Some(first) = args.first() else { return Ok(None) };
    if first.is_undefined() {
        return Ok(None);
    }
    Ok(Some(first.to_string(ctx)?.to_std_string_escaped()))
}

/// What a script asked for, which decides both the severity and the sentence.
enum Ask<'a> {
    /// No locale named: the host's own default was wanted.
    Default,
    /// A named locale this engine does not carry.
    Locale(&'a str),
}

/// One replaced method: which document to record against, and whether it already has.
#[derive(boa_gc::Trace, boa_gc::Finalize, Clone)]
struct Method {
    handle: DocumentHandle,
    #[unsafe_ignore_trace]
    name: &'static str,
    #[unsafe_ignore_trace]
    recorded: Rc<Cell<bool>>,
}

impl Method {
    /// One method's replacement, fresh for this context and therefore for this execution.
    fn new(handle: &DocumentHandle, name: &'static str) -> Self {
        Self { handle: handle.clone(), name, recorded: Rc::new(Cell::new(false)) }
    }

    /// Writes the decision onto the document, where `inspect structure` prints it.
    ///
    /// The first call in *this context* writes; the rest are the same sentence about the
    /// same method and are dropped. One context is one script execution, so a loop
    /// records once and a second calculation pass records again — see the module note.
    fn record(&self, ask: Ask<'_>) {
        if self.recorded.replace(true) {
            return;
        }
        let name = self.name;
        let decision = match ask {
            Ask::Default => fepdf::Decision::ambiguity(
                CLAUSE,
                format!("a script called {name} with no locale"),
                "returned unlocalised digits; ECMA-262 permits it and this engine carries \
                 no ECMA-402 default locale to prefer instead"
                    .to_string(),
            ),
            Ask::Locale(asked) => fepdf::Decision::violation(
                CLAUSE,
                format!("a script called {name} for locale {asked}"),
                "raised into the script; this engine carries no ECMA-402, and an \
                 unlocalised answer would answer a different question than the one asked"
                    .to_string(),
            ),
        };
        self.handle.with(|doc| doc.inner().record(decision));
    }

    /// The error a refused call throws, naming the clause rather than the missing
    /// function.
    fn refusal(&self, asked: &str) -> boa_engine::JsError {
        let text = format!(
            "{} was asked for locale {asked}; ISO 32000-2 {CLAUSE} admits ECMA-402 and this \
             engine does not carry it, so the request is refused rather than answered without \
             the locale",
            self.name
        );
        boa_engine::JsError::from_opaque(js_string!(text.as_str()).into())
    }
}

/// Overwrites one method on a builtin's prototype.
///
/// Through `globalThis` and not the intrinsics, because this runs before the document's
/// script does and the globals are still the ones boa built.
fn set_method(
    context: &mut Context,
    builtin: &str,
    method: &str,
    native: NativeFunction,
) -> Result<(), ScriptError> {
    let function = native.to_js_function(context.realm());
    let global = context.global_object();
    let constructor = global
        .get(js_string!(builtin), context)
        .map_err(|e| ScriptError::HostUnavailable(e.to_string()))?;
    let constructor = constructor
        .as_object()
        .ok_or_else(|| ScriptError::HostUnavailable(format!("{builtin} is not an object")))?;
    let prototype = constructor
        .get(js_string!("prototype"), context)
        .map_err(|e| ScriptError::HostUnavailable(e.to_string()))?;
    let prototype = prototype
        .as_object()
        .ok_or_else(|| ScriptError::HostUnavailable(format!("{builtin}.prototype is missing")))?;
    prototype
        .set(js_string!(method), function, true, context)
        .map(|_| ())
        .map_err(|e| ScriptError::HostUnavailable(e.to_string()))
}
