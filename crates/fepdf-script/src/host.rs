//! The host objects a document script runs against, and the run itself.

use boa_engine::property::Attribute;
use boa_engine::{Context, JsResult, JsValue, NativeFunction, Source, js_string};
use boa_gc::{Finalize, Trace};
use fepdf::PdfDocument;
use std::cell::RefCell;
use std::rc::Rc;

/// What went wrong running a script.
///
/// A typed error and not a `String` (RR-15 Rule 11): a caller distinguishes "the document
/// asked for something this engine does not provide" from "the script threw", and only
/// the first is a statement about the engine.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The script did not parse or did not run to completion.
    #[error("the script did not complete: {0}")]
    DidNotComplete(String),
    /// A host object could not be installed, which is this engine's fault rather than
    /// the document's.
    #[error("the host environment could not be built: {0}")]
    HostUnavailable(String),
}

/// The host properties a script can observe, supplied rather than read from the machine.
///
/// `new Date()`, `Math.random` and `app.viewerVersion` decide output, and RR-15's
/// determinism rules bind anything that does. Injecting them makes the same document
/// produce the same result twice.
///
/// **This is not tidiness.** Measured on the corpus: Adobe's stock file-attachment script
/// branches on `app.viewerVersion`, and at 7 it does nothing while at 6 it reaches
/// `syncAnnotScan` and fails. The injected value decides whether a script completes.
#[derive(Debug, Clone)]
pub struct ScriptEnvironment {
    /// Milliseconds since the Unix epoch, for `new Date()`.
    pub now_ms: f64,
    /// Seed for `Math.random`.
    pub seed: u64,
    /// What `app.viewerVersion` reports.
    pub viewer_version: f64,
}

impl Default for ScriptEnvironment {
    fn default() -> Self {
        // A fixed instant rather than the clock: two runs of the same document agree.
        // 2020-01-01T00:00:00Z, chosen because it is the year ISO 32000-2 was published
        // and because any constant is better than one that moves.
        Self { now_ms: 1_577_836_800_000.0, seed: 0, viewer_version: 7.0 }
    }
}

/// The document, reachable from a boa capture.
///
/// `Rc<RefCell<…>>` is not a workaround for the borrow checker; it is the only shape
/// boa's capture API admits. See the crate documentation.
#[derive(Trace, Finalize, Clone)]
pub struct DocumentHandle {
    #[unsafe_ignore_trace]
    inner: Rc<RefCell<PdfDocument>>,
}

impl DocumentHandle {
    /// Wraps a document so a script can reach it.
    #[must_use]
    pub fn new(document: PdfDocument) -> Self {
        Self { inner: Rc::new(RefCell::new(document)) }
    }

    /// Borrows the document for a query.
    ///
    /// Reads go through the facade's existing queries — there is no third path
    /// (ADR-0025). A script asking `this.numPages` is asking what `page_count` answers.
    pub fn with<T>(&self, f: impl FnOnce(&PdfDocument) -> T) -> T {
        f(&self.inner.borrow())
    }

    /// Borrows the document to apply an `Operation`.
    ///
    /// The document is **not** handed back by value: `#[derive(Finalize)]` gives this
    /// type a `Drop`, so it cannot be moved out of, and a caller keeps its own handle
    /// rather than extracting one. Saving is a query on the document like any other.
    pub fn with_mut<T>(&self, f: impl FnOnce(&mut PdfDocument) -> T) -> T {
        f(&mut self.inner.borrow_mut())
    }
}

/// What a run left behind.
#[derive(Debug, Default, Clone)]
pub struct ScriptOutcome {
    /// Every `app.alert` the script raised, in order.
    pub alerts: Vec<String>,
}

/// Runs document scripts against a document.
pub struct ScriptHost {
    handle: DocumentHandle,
    environment: ScriptEnvironment,
    outcome: Rc<RefCell<ScriptOutcome>>,
}

impl ScriptHost {
    /// Prepares a host for a document.
    #[must_use]
    pub fn new(handle: DocumentHandle, environment: ScriptEnvironment) -> Self {
        Self { handle, environment, outcome: Rc::new(RefCell::new(ScriptOutcome::default())) }
    }

    /// Runs one script and reports what it did.
    ///
    /// # Errors
    /// [`ScriptError::DidNotComplete`] when the script throws or will not parse, which
    /// includes reaching an Acrobat global this engine does not provide.
    pub fn run(&self, source: &str) -> Result<ScriptOutcome, ScriptError> {
        let mut context = Context::default();
        self.install_app(&mut context)?;
        self.install_doc(&mut context)?;
        // The script runs as the body of a function called on the Doc, so `this` is the
        // document. **A global property named `this` does not work**: in a non-strict
        // script `this` *is* `globalThis`, so the identifier never reaches a property
        // called "this" and every `this.x` reads `undefined`. Measured — the first
        // version of this crate did exactly that and `this.numPages` came back undefined.
        //
        // What it costs: a top-level `var` or `function` in the script becomes
        // function-scoped rather than global, so a document-level script cannot define a
        // helper for a *later* script this way. Nothing here runs two scripts yet; when
        // something does, this is the line to revisit.
        let wrapped = format!("(function () {{\n{source}\n}}).call(__fepdf_doc__);");
        context
            .eval(Source::from_bytes(wrapped.as_bytes()))
            .map_err(|e| ScriptError::DidNotComplete(e.to_string()))?;
        Ok(self.outcome.borrow().clone())
    }

    /// `app`: the viewer. Its properties are injected, never read from the machine.
    fn install_app(&self, context: &mut Context) -> Result<(), ScriptError> {
        let alerts = Rc::clone(&self.outcome);
        let alert = NativeFunction::from_copy_closure_with_captures(
            |_t: &JsValue,
             args: &[JsValue],
             sink: &AlertSink,
             ctx: &mut Context|
             -> JsResult<JsValue> {
                let text = args.first().cloned().unwrap_or_default().to_string(ctx)?;
                sink.0.borrow_mut().alerts.push(text.to_std_string_escaped());
                Ok(JsValue::undefined())
            },
            AlertSink(alerts),
        );
        let app = boa_engine::object::ObjectInitializer::new(context)
            .function(alert, js_string!("alert"), 1)
            .property(
                js_string!("viewerVersion"),
                self.environment.viewer_version,
                Attribute::all(),
            )
            .property(js_string!("viewerVariation"), js_string!("Full"), Attribute::all())
            .build();
        context
            .register_global_property(js_string!("app"), app, Attribute::all())
            .map_err(|e| ScriptError::HostUnavailable(e.to_string()))
    }

    /// The document object a script sees as `this` (12.6.4.16).
    ///
    /// Registered under an internal name and bound as `this` by [`ScriptHost::run`],
    /// because `this` is not a name that can be registered.
    fn install_doc(&self, context: &mut Context) -> Result<(), ScriptError> {
        let pages = self.handle.with(|doc| doc.page_count().unwrap_or(0));
        let numeric = u32::try_from(pages).map_or(0.0, f64::from);
        let doc = boa_engine::object::ObjectInitializer::new(context)
            .property(js_string!("numPages"), numeric, Attribute::all())
            .property(js_string!("external"), false, Attribute::all())
            .property(js_string!("dataObjects"), JsValue::null(), Attribute::all())
            .build();
        context
            .register_global_property(js_string!("__fepdf_doc__"), doc, Attribute::all())
            .map_err(|e| ScriptError::HostUnavailable(e.to_string()))
    }
}

/// Where `app.alert` accumulates. A named type because boa's captures must implement
/// `Trace`, and a bare `Rc<RefCell<…>>` does not.
#[derive(Trace, Finalize, Clone)]
struct AlertSink(#[unsafe_ignore_trace] Rc<RefCell<ScriptOutcome>>);
