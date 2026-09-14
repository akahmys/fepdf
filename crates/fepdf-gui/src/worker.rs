//! Off-thread document worker and request/response dispatch loop.

use bytes::Bytes;
use fepdf::{FallbackFontType, VelloBackend};
use fepdf::{Operation, OutlineTree, PageSelection, PdfDocument};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use vello::Scene;

pub enum WorkerRequest {
    Open {
        data: Bytes,
        name: Option<String>,
        /// What to try as the user or owner password (7.6.4.4).
        ///
        /// **`None` is not "no password"**, it is "none offered yet". A document that
        /// stays locked comes back as `NeedsPassword` rather than as an error, because
        /// the engine opens it either way: its structure is readable and its content is
        /// not, which is a document to ask about rather than one to refuse.
        password: Option<String>,
    },
    RenderPage {
        index: usize,
        scale: f64,
    },
    UpdateNode {
        handle_id: u32,
        tag: String,
        alt_text: Option<String>,
    },
    Save {
        path: std::path::PathBuf,
        /// `/U` (7.6.4.4). `None` writes the document unprotected.
        password: Option<String>,
        /// `/O`, which is meaningless without a user password and is ignored then.
        owner_password: Option<String>,
        compress: bool,
        linearize: bool,
        upgrade_pdf20: bool,
        redaction_zones: Vec<crate::redaction::RedactionZone>,
        cert_path: Option<std::path::PathBuf>,
        key_path: Option<std::path::PathBuf>,
        signature_position: Option<(usize, [f32; 4])>,
    },
    /// An operation the reader asked for, already translated (Rule D).
    ///
    /// **One variant rather than one per operation.** The vocabulary has thirty and this
    /// crate reached five of them; giving each a request would have made the worker the
    /// place operations are enumerated, which is `fepdf-doc`'s job. `fepdf-mcp` sends
    /// them the same way. `Box` because the enum is large and this message is rare.
    Apply {
        operation: Box<fepdf::Operation>,
        /// What to say when it worked, in the reader's language.
        done: String,
    },
    /// What this document is and what it does, computed when the panel asks.
    ///
    /// **On demand rather than at open.** `Coverage::of` reads the file a second time,
    /// which on the larger samples is as much again as opening it; a reader who never
    /// opens the panel should not pay for it.
    Survey,
    Audit,
    /// 6.3.2.3: a person turning a layer on or off. Not a document edit — the worker
    /// re-renders and the saved bytes are unchanged.
    SetLayerVisible {
        layer: fepdf::LayerId,
        on: bool,
    },
    /// Move a structure element beside or inside another (14.7.4).
    MoveNode(fepdf::StructElemMove),
    ReorderPagesBatch {
        source_indices: Vec<usize>,
        target_insert_pos: usize,
    },
    RemovePages {
        indices: Vec<usize>,
    },
    /// Take the named pages out into a document of their own.
    ///
    /// **The result goes to a file this thread names, not one the reader chose.** A
    /// window holds one document, so the extracted pages arrive as a second window —
    /// and a reader who is shown the pages can then export them wherever they like,
    /// with every option the wizard has. Asking for a path first would be asking before
    /// they have seen what they are saving.
    ExtractPages {
        indices: Vec<usize>,
        /// Whether the pages also leave this document.
        ///
        /// The removal is an `Operation` and is recorded, so it can be undone here; the
        /// extraction is not, because it changes nothing about this document.
        remove: bool,
        /// What to call the file, without its extension.
        ///
        /// **Chosen by the window, because the name is in the reader's language** — this
        /// thread holds the document and not the language, which is the same reason
        /// `Busy` carries a key rather than a sentence. It is also the window that knows
        /// what the open document is called.
        name: String,
    },
    DuplicatePage {
        index: usize,
    },
    RotatePages {
        indices: Vec<usize>,
        delta: fepdf::Quarter,
    },
    /// Take back the last operation, and the one before it, and so on.
    Undo,
    /// Put back the last operation `Undo` took.
    Redo,
}

/// The document as it was opened, and every operation applied to it since.
///
/// **Recorded and replayed, because inverted is not available.** `ARCHITECTURE.md` §4.1
/// lists undo as a consequence that falls out of operations being values — "recorded,
/// inverted and replayed" — and of those three verbs the engine implements none:
/// `grep -rn "fn invert\|fn inverse\|fn undo"` over `fepdf-doc`, `fepdf-model` and
/// `fepdf` returns nothing. Two of the three are had cheaply anyway, and the third is not
/// merely unwritten: `Retag` rebuilds the structure tree from heuristics and
/// `ApplyBatesNumbering` draws into content streams, so their inverses do not exist to be
/// written.
///
/// **Replaying costs one open.** Measured on 2026-09-10 over the samples: 28ms for
/// `constitution.pdf`, 37ms for `fugaku.pdf`, 251ms for `volvo_xc90.pdf` at 27MB, and
/// 1.7s for `intel_sdm.pdf` at 24MB. The first three are imperceptible and the last is
/// why an undo says that it is happening.
struct History {
    /// What `Open` was given, kept so the document can be rebuilt from it. `Bytes` is
    /// refcounted and the arena already points into this buffer.
    origin: Option<(Bytes, Option<String>, Option<String>)>,
    /// Applied, in order.
    applied: Vec<Operation>,
    /// Taken back, most recent last. Emptied by any new operation, because a branch in
    /// the history is a second thing to explain.
    undone: Vec<Operation>,
}

impl History {
    const fn new() -> Self {
        Self { origin: None, applied: Vec::new(), undone: Vec::new() }
    }

    /// Whether the document differs from the file it was opened from.
    fn edited(&self) -> bool {
        !self.applied.is_empty()
    }
}

/// Everything the UI needs after a document finishes loading.
///
/// Kept behind a `Box` in [`WorkerResponse`] so that the far more frequent
/// `PageRendered` messages are not padded out to this variant's size.
pub struct LoadedDocument {
    pub name: Option<String>,
    pub num_pages: usize,
    pub page_sizes: Vec<(f64, f64)>, // (width, height)
    pub ust_root: Option<crate::sidebar::USTNode>,
    pub file_size: usize,
    pub version: String,
    pub metadata: fepdf::MetadataInfo,
    pub security_method: String,
    pub permissions: Option<i32>,
    pub fonts: Vec<fepdf::FontSummary>,
    pub viewer_direction: Option<String>,
    /// What to present for optional content, per `/Order` (8.11.4.3). Empty when the
    /// document has no layers *or* when its configuration lists none — the clause makes
    /// those the same answer.
    pub layers: Vec<fepdf::LayerRow>,
    /// Reading decisions recorded by the engine while opening or repairing the document (6.3.2.3).
    pub decisions: Vec<fepdf::Decision>,
    /// The bookmark tree as the file holds it (12.3.3), and what the read cost.
    pub outlines: (OutlineTree, fepdf::OutlineReport),
}

pub enum WorkerResponse {
    DocumentLoaded(Box<LoadedDocument>),
    /// Something long is running, and what it is. **The worker names the work and the
    /// window says it**: this thread holds the document, not the reader's language.
    Busy {
        key: &'static str,
    },
    /// It finished, whatever it was.
    Idle,
    /// What the history can do now, and whether the document differs from its file.
    HistoryChanged {
        can_undo: bool,
        can_redo: bool,
        edited: bool,
    },
    LoadingProgress {
        message: String,
    },
    PageRendered {
        index: usize,
        _scale: f64,
        scene: Arc<Scene>,
        text: Option<String>,
        spans: Option<Vec<crate::interaction::TextSpan>>,
    },
    AuditFindings {
        findings: Vec<(String, String, String, Option<u32>)>,
    },
    /// The structure tree has changed shape, and here it is as the file now holds it.
    ///
    /// **Sent rather than letting the window keep its own arrangement.** A move can be
    /// refused — a cycle, an element the tree does not hold — and a window that had
    /// already rearranged itself would show the reader an order the file does not have.
    /// The tree is re-read from the document, so what is on screen is what would be
    /// saved.
    StructTreeChanged {
        root: Option<Box<crate::sidebar::USTNode>>,
    },
    /// The extracted pages are on disk here, ready to be opened in a window of their own.
    PagesExtracted {
        path: std::path::PathBuf,
    },
    /// The pages as the document now holds them, after an operation changed how many
    /// there are.
    ///
    /// **Sent only when the count changed.** The window keeps its own count and sizes so
    /// it can lay out a frame without asking, and every operation that alters them
    /// adjusts them before sending — except the ones that cannot: `InsertFrom` adds as
    /// many pages as the file it is given holds, which this thread learns by opening it
    /// and the window cannot know at all.
    PagesChanged {
        page_sizes: Vec<(f64, f64)>,
    },
    /// The bookmark tree as the file now holds it, after an operation changed something.
    ///
    /// **Sent after every operation, not only after `UpdateOutlines`.** A bookmark names
    /// a page, so removing, reordering, duplicating or inserting pages changes what the
    /// existing bookmarks point at — and a panel keeping its own copy would go on showing
    /// the old answer. Deciding here which operations can move a page would put the
    /// vocabulary's business in the worker, which is `fepdf-doc`'s.
    OutlinesChanged {
        tree: Box<OutlineTree>,
        report: fepdf::OutlineReport,
    },
    /// A layer was toggled: the panel's states have moved and the page needs redrawing.
    LayersChanged {
        layers: Vec<fepdf::LayerRow>,
    },
    /// The document is encrypted and the password offered did not unlock it.
    ///
    /// Carries the bytes back so the app can retry without reading the file again, and
    /// says whether a password had been tried — the difference between "this is locked"
    /// and "that was the wrong one", which are different things to put in front of
    /// someone.
    NeedsPassword {
        data: Bytes,
        name: Option<String>,
        method: String,
        retried: bool,
    },
    /// An `Apply` succeeded, with what to tell the reader.
    OperationApplied {
        message: String,
    },
    /// The answer to `Survey`.
    Surveyed {
        /// What runs without the reader doing anything, and what the document can do.
        actions: Box<fepdf::ActionReport>,
        /// The share of what the file presents whose contents the engine reads.
        coverage: Option<fepdf::Coverage>,
    },
    DocumentSaved {
        path: std::path::PathBuf,
        /// What the write cost, in the document's own terms. Empty for most files;
        /// non-empty when the source declared restrictions the output cannot carry,
        /// which the user is about to hand to someone else (7.6.4.2).
        notices: Vec<String>,
    },
    /// Something did not happen. `key` frames it and `detail` is the engine's own
    /// sentence, which names an ISO clause and has no translation.
    Failed {
        key: &'static str,
        detail: Option<String>,
    },
}

pub fn run_worker(rx: Receiver<WorkerRequest>, tx: Sender<WorkerResponse>, ctx: egui::Context) {
    // RR-15 Limit: GUI - main routing message loop dispatcher for background worker thread
    let mut current_doc: Option<PdfDocument> = None;
    // The bytes the open read. `Bytes` is refcounted and the arena already points into
    // this buffer, so holding it costs a pointer rather than the file.
    let mut current_bytes: Option<Bytes> = None;
    let mut history = History::new();
    let system_fonts = VelloBackend::load_system_fonts();
    let mut text_cache = std::collections::BTreeMap::new();
    let mut spans_cache = std::collections::BTreeMap::new();

    for request in rx {
        match request {
            WorkerRequest::Open { data, name, password } => {
                text_cache.clear();
                spans_cache.clear();
                current_bytes = Some(data.clone());
                history = History::new();
                history.origin = Some((data.clone(), name.clone(), password.clone()));
                current_doc = handle_open(data, name, password, &[], &tx);
                ctx.request_repaint();
            }
            WorkerRequest::RenderPage { index, scale } => {
                handle_render(
                    current_doc.as_ref(),
                    index,
                    scale,
                    &tx,
                    Arc::clone(&system_fonts),
                    &mut text_cache,
                    &mut spans_cache,
                );
                ctx.request_repaint();
            }
            WorkerRequest::MoveNode(move_) => {
                // The tree is re-read afterwards, and re-reading it places every element
                // from its marked content — 438ms over `volvo_xc90.pdf`'s 415 pages.
                let _ = tx.send(WorkerResponse::Busy { key: "busy_retagging" });
                handle_move_node(&mut current_doc, &mut history, move_, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::UpdateNode { handle_id, tag, alt_text } => {
                text_cache.clear();
                spans_cache.clear();
                // Retagging re-runs the whole PDF/UA audit, which is the long half.
                let _ = tx.send(WorkerResponse::Busy { key: "busy_auditing" });
                handle_update_node(&mut current_doc, &mut history, handle_id, tag, alt_text, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Save {
                path,
                password,
                owner_password,
                compress,
                linearize,
                upgrade_pdf20,
                redaction_zones,
                cert_path,
                key_path,
                signature_position,
            } => {
                text_cache.clear();
                spans_cache.clear();
                let _ = tx.send(WorkerResponse::Busy { key: "busy_saving" });
                handle_save(
                    current_doc.as_ref(),
                    path,
                    password,
                    owner_password,
                    compress,
                    linearize,
                    upgrade_pdf20,
                    redaction_zones,
                    cert_path,
                    key_path,
                    signature_position,
                    &tx,
                );
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Apply { operation, done } => {
                text_cache.clear();
                spans_cache.clear();
                // `Retag` rebuilds the structure tree from heuristics; the others are
                // quick, and one arm cannot tell which it was handed.
                let _ = tx.send(WorkerResponse::Busy { key: "busy_applying" });
                handle_apply(&mut current_doc, &mut history, *operation, done, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::ExtractPages { indices, remove, name } => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_exporting" });
                handle_extract(&mut current_doc, &mut history, (&indices, remove, &name), &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Survey => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_surveying" });
                if let Some(doc) = current_doc.as_ref() {
                    let actions = fepdf::ActionReport::of(doc.inner()).unwrap_or_default();
                    // Recorded as absent rather than as zero: a coverage this could not
                    // compute and a document that presents nothing are different answers.
                    let coverage = current_bytes.as_ref().and_then(|b| fepdf::Coverage::of(b).ok());
                    let _ =
                        tx.send(WorkerResponse::Surveyed { actions: Box::new(actions), coverage });
                }
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Audit => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_auditing" });
                handle_audit(current_doc.as_ref(), &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::SetLayerVisible { layer, on } => {
                if let Some(ref doc) = current_doc {
                    // The panel is re-read rather than cached: it carries `/Locked` and
                    // `/RBGroups`, and it is what refuses a locked group rather than the
                    // UI being trusted to have disabled the row.
                    let panel = doc.layers();
                    if doc.set_layer_visible(&panel, layer, on) {
                        // What is drawn changed, so every cached page is stale.
                        text_cache.clear();
                        spans_cache.clear();
                        let _ =
                            tx.send(WorkerResponse::LayersChanged { layers: doc.layers().rows });
                    }
                }
                ctx.request_repaint();
            }
            WorkerRequest::ReorderPagesBatch { source_indices, target_insert_pos } => {
                text_cache.clear();
                spans_cache.clear();
                apply_recorded(
                    &mut current_doc,
                    &mut history,
                    Operation::ReorderBatch { sources: source_indices, target: target_insert_pos },
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::RemovePages { mut indices } => {
                text_cache.clear();
                spans_cache.clear();
                // One operation, not a descending loop. Sorting the indices so that
                // removing one did not move the next was the frontend doing the engine's
                // arithmetic; `RemovePages` takes the set and owns the order.
                indices.sort_unstable();
                indices.dedup();
                apply_recorded(
                    &mut current_doc,
                    &mut history,
                    Operation::RemovePages(PageSelection::Indices(indices)),
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::DuplicatePage { index } => {
                text_cache.clear();
                spans_cache.clear();
                apply_recorded(
                    &mut current_doc,
                    &mut history,
                    Operation::DuplicatePages(PageSelection::Single(index)),
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::RotatePages { indices, delta } => {
                text_cache.clear();
                spans_cache.clear();
                apply_recorded(
                    &mut current_doc,
                    &mut history,
                    Operation::Rotate {
                        pages: PageSelection::Indices(indices),
                        mode: fepdf::RotateMode::Relative(delta),
                    },
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::Undo => {
                text_cache.clear();
                spans_cache.clear();
                let _ = tx.send(WorkerResponse::Busy { key: "history_undoing" });
                if let Some(taken) = history.applied.pop() {
                    history.undone.push(taken);
                    current_doc = rebuild(&history, &tx);
                }
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Redo => {
                text_cache.clear();
                spans_cache.clear();
                let _ = tx.send(WorkerResponse::Busy { key: "history_redoing" });
                if let Some(back) = history.undone.pop() {
                    history.applied.push(back);
                    current_doc = rebuild(&history, &tx);
                }
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
        }
    }
}

/// Applies `operation`, records it, and says what happened.
///
/// **One path for all six mutations.** The five page operations logged their failures
/// with `log::error!` — to a terminal the reader is not looking at — while `Apply` beside
/// them reported its own, which is what §4.3's rule asks for. And a journal that recorded
/// an operation the document refused would replay a different document than the one on
/// screen, so the record has to hang off the same `Ok`.
fn apply_recorded(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    operation: Operation,
    done: Option<String>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc.as_mut() else { return };
    match doc.apply(operation.clone()) {
        Ok(()) => {
            history.applied.push(operation);
            // A new operation after an undo abandons what was undone: a branch in the
            // history is a second thing the window would have to explain.
            history.undone.clear();
            if let Some(message) = done {
                let _ = tx.send(WorkerResponse::OperationApplied { message });
            }
            let (tree, report) = doc.outlines();
            let _ = tx.send(WorkerResponse::OutlinesChanged { tree: Box::new(tree), report });
            let _ = tx.send(WorkerResponse::HistoryChanged {
                can_undo: !history.applied.is_empty(),
                can_redo: !history.undone.is_empty(),
                edited: history.edited(),
            });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_operation_failed",
                detail: Some(format!("{e:?}")),
            });
        }
    }
}

/// Opens the original bytes again and replays what is still in the history onto them.
fn rebuild(history: &History, tx: &Sender<WorkerResponse>) -> Option<PdfDocument> {
    let (data, name, password) = history.origin.as_ref()?;
    let _ = tx.send(WorkerResponse::HistoryChanged {
        can_undo: !history.applied.is_empty(),
        can_redo: !history.undone.is_empty(),
        edited: history.edited(),
    });
    handle_open(data.clone(), name.clone(), password.clone(), &history.applied, tx)
}

/// The document's structure tree, with every element placed on the page it drew on.
///
/// The placement is a second call and it is made here, once, when the document opens:
/// the reading-order overlay and the element outlines are both on by default, and both
/// draw a rectangle per element. Without this they drew nothing at all — no element in
/// any of the nine samples declares a `/BBox`, so the tree arrived with no geometry.
///
fn resolve_struct_tree_root(
    doc: &PdfDocument,
    _next_id: &mut usize,
) -> Option<crate::sidebar::USTNode> {
    let mut root = doc.extract_struct_tree()?;
    doc.fill_structure_boxes(&mut root);
    Some(root)
}

/// Whether a document with no declared reading direction is a vertically set CJK one.
///
/// **Table 30 gives `/Direction` a default of L2R, and almost nothing declares it** — 2
/// files of 524, `samples/bokutokitan.pdf` saying `R2L` and one external file `L2R`. So a
/// document that is set vertically and says nothing gets bound the wrong way round unless
/// something looks at it, and `samples/fy05.pdf` is exactly that: six fonts in a vertical
/// writing mode and no declaration.
///
/// It is a guess and is recorded as one. [ADR-0041] settled the shape: where a file
/// declares, obey it; where it declares nothing, a heuristic is what there is to go on,
/// and it says so. `-V` is the writing-mode suffix Adobe's CMap names carry for vertical
/// forms (9.7.5.2), which is why a font name is evidence at all.
///
/// [ADR-0041]: ../../docs/adr/0041-a-character-collection-is-declared-not-guessed.md
fn infer_binding(fonts: &[fepdf::FontSummary], lang: Option<&str>) -> Option<String> {
    let _ = lang;
    let vertical = fonts.iter().filter(|f| f.is_vertical).count();
    let share = vertical as f32 / fonts.len().max(1) as f32;
    (share >= VERTICAL_SHARE).then(|| "R2L".to_string())
}

/// What proportion of a document's fonts must be set vertically before it is taken to be a
/// vertically set book.
///
/// **Presence was not enough.** The test was whether *any* font carried the writing-mode
/// suffix, and `samples/fy05.pdf` — a horizontally set government report — has 6 of them
/// among **316**, in a table or on its cover. 1.9% was enough to bind it right to left.
/// `samples/bokutokitan.pdf`, which is a vertically set book, is 4 of 18: **22%**.
///
/// **The threshold is fitted to those two documents and one of them is unreachable.**
/// `bokutokitan.pdf` declares `R2L` itself, so the guess is never asked about it; the only
/// document in either corpus that this function is consulted for is `fy05.pdf`, where the
/// answer is that it is not vertical. There is no case here where the heuristic is known to
/// be right, only one where it is now known not to be wrong.
const VERTICAL_SHARE: f32 = 0.10;

/// The handler named by an open that failed because nothing could be read without a key.
///
/// **A locked document reaches the reader two ways.** When its structure sits in a plain
/// cross-reference table, 7.6 leaves that outside the encryption and the document opens
/// with a 7.6.1 `Violation` — [`still_locked`] reads that one. When the structure is in
/// object streams (7.5.7), which is what this engine writes by default, there is nothing
/// to read at all and `open_with_options` returns an error instead. Both are the same
/// question to a reader, and only the second was ever going to be the common case.
fn locked_by_error(error: &fepdf::PdfError) -> Option<String> {
    let text = format!("{error:?}");
    if !text.contains("was not unlocked") {
        return None;
    }
    // "Password Security (AES-256) was not unlocked, and ..." — the same phrase the other
    // path puts in its decision, so the prompt reads the same either way.
    text.split(" was not unlocked").next().and_then(|head| {
        head.rfind('"').map(|at| head[at + 1..].to_string()).or_else(|| Some(head.to_string()))
    })
}

/// Whether the document opened but stayed encrypted, and under which handler.
///
/// **The engine does not refuse a locked document**, and should not: 7.6 leaves the file
/// structure outside the encryption, so the page count, the catalogue and the security
/// handler are all readable while the content is not. It records a 7.6.1 `Violation`
/// saying so, and this is the frontend reading it — the point at which "structure yes,
/// content no" has to become a question the reader can answer.
fn still_locked(doc: &PdfDocument) -> Option<String> {
    doc.decisions().iter().find_map(|d| {
        (d.clause == "7.6.1" && d.found.contains("could not be unlocked"))
            .then(|| d.found.split(" could not be").next().unwrap_or("This document").to_string())
    })
}

/// Opens `data`, replays `history` onto it, and announces what came out.
///
/// **One function rather than an open and a separate announce**, because the packaging
/// below describes the document *after* the replay — a page count taken before it would
/// describe the file rather than the state the reader is looking at.
fn handle_open(
    // RR-15 Limit: Dispatcher - handles open document worker requests and packages file properties
    data: Bytes,
    name: Option<String>,
    password: Option<String>,
    history: &[Operation],
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    let file_size = data.len();
    let tx_clone = tx.clone();
    let retried = password.is_some();
    let options = fepdf::IngestionOptions {
        password,
        progress_callback: Some(Arc::new(move |msg| {
            let _ = tx_clone.send(WorkerResponse::LoadingProgress { message: msg });
        })),
        ..fepdf::IngestionOptions::default()
    };
    let bytes_back = data.clone();
    match PdfDocument::open_with_options(data, &options) {
        Ok(mut doc) => {
            if let Some(method) = still_locked(&doc) {
                let _ = tx.send(WorkerResponse::NeedsPassword {
                    data: bytes_back,
                    name,
                    method,
                    retried,
                });
                return None;
            }
            // **A replay that fails leaves nothing open.** Half a history is a document
            // that matches neither the file nor the screen, and the reader has no way to
            // tell which they have.
            for operation in history {
                if let Err(e) = doc.apply(operation.clone()) {
                    let _ = tx.send(WorkerResponse::Failed {
                        key: "notice_replay_failed",
                        detail: Some(format!("{e:?}")),
                    });
                    return None;
                }
            }
            let num_pages = doc.page_count().unwrap_or(0);
            let mut page_sizes = Vec::with_capacity(num_pages);
            for i in 0..num_pages {
                page_sizes.push(doc.get_page_size(i).unwrap_or((595.0, 842.0)));
            }

            let mut next_id = 0;
            let mut ust_root = resolve_struct_tree_root(&doc, &mut next_id);

            if ust_root.is_none() {
                ust_root = Some(crate::sidebar::USTNode {
                    id: 0,
                    tag: "Document".to_string(),
                    title: "PDF Document Catalog (Untagged)".to_string(),
                    alt_text: None,
                    rect: None,
                    page_index: None,
                    handle_index: None,
                    mcids: Vec::new(),
                    lang: None,
                    role: None,
                    children: Vec::new(),
                });
            }

            let version = doc.get_summary().ok().map_or_else(|| "1.7".to_string(), |s| s.version);
            let metadata = doc.metadata();
            let security_method = doc.security_method();
            let permissions = doc.permissions();
            let fonts = doc.fonts();
            let mut decisions = doc.decisions();
            let mut viewer_direction = doc.viewer_direction();
            if viewer_direction.is_none()
                && let Some(inferred) = infer_binding(&fonts, doc.language().as_deref())
            {
                decisions.push(fepdf::Decision::ambiguity(
                    "12.2",
                    "the document declares no /ViewerPreferences /Direction and is set in \
                     vertical CJK: its fonts name a `-V` writing mode, or its /Lang is \
                     Japanese",
                    "bound it right to left rather than taking Table 30's default of L2R, \
                     which opens a vertically set book at the wrong end",
                ));
                viewer_direction = Some(inferred);
            }

            let _ = tx.send(WorkerResponse::DocumentLoaded(Box::new(LoadedDocument {
                name,
                num_pages,
                page_sizes,
                ust_root,
                file_size,
                version,
                metadata,
                security_method,
                permissions,
                fonts,
                viewer_direction,
                layers: doc.layers().rows,
                decisions,
                outlines: doc.outlines(),
            })));
            Some(doc)
        }
        Err(e) if locked_by_error(&e).is_some() => {
            let method = locked_by_error(&e).unwrap_or_else(|| "This document".to_string());
            let _ =
                tx.send(WorkerResponse::NeedsPassword { data: bytes_back, name, method, retried });
            None
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_open_failed",
                detail: Some(e.to_string()),
            });
            None
        }
    }
}

fn get_or_extract_text(
    doc: &PdfDocument,
    index: usize,
    cache: &mut std::collections::BTreeMap<usize, String>,
) -> Option<String> {
    if let Some(cached) = cache.get(&index) {
        return Some(cached.clone());
    }
    let text = doc.extract_text(index).ok();
    if let Some(ref t) = text {
        cache.insert(index, t.clone());
    }
    text
}

fn get_or_extract_spans(
    doc: &PdfDocument,
    index: usize,
    cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
) -> Option<Vec<crate::interaction::TextSpan>> {
    if let Some(cached) = cache.get(&index) {
        return Some(cached.clone());
    }
    let spans: Option<Vec<crate::interaction::TextSpan>> =
        doc.extract_spans(index).ok().map(|sdk_spans| {
            sdk_spans
                .into_iter()
                .map(|s| crate::interaction::TextSpan {
                    text: s.text,
                    rect: egui::Rect::from_two_pos(
                        egui::pos2(s.x as f32, s.y as f32),
                        egui::pos2((s.x + s.width) as f32, (s.y + s.font_size) as f32),
                    ),
                })
                .collect()
        });
    if let Some(ref s) = spans {
        cache.insert(index, s.clone());
    }
    spans
}

fn handle_render(
    doc_opt: Option<&PdfDocument>,
    index: usize,
    scale: f64,
    tx: &Sender<WorkerResponse>,
    system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    text_cache: &mut std::collections::BTreeMap<usize, String>,
    spans_cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
) {
    let Some(doc) = doc_opt else { return };
    let r = doc.get_page_box(index).unwrap_or_else(|_| fepdf::Rect::new(0.0, 0.0, 595.0, 842.0));
    let w = (r.x2 - r.x1).abs();
    let h = (r.y2 - r.y1).abs();
    let rot = doc.get_page_rotation(index).unwrap_or(0);

    let initial_transform = match rot {
        90 => kurbo::Affine::new([0.0, scale, -scale, 0.0, h * scale, 0.0]),
        180 => kurbo::Affine::new([-scale, 0.0, 0.0, scale, w * scale, 0.0]),
        270 => kurbo::Affine::new([0.0, -scale, scale, 0.0, 0.0, w * scale]),
        _ => kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, h * scale]),
    };
    let mut backend = VelloBackend::new(system_fonts);

    let text = get_or_extract_text(doc, index, text_cache);
    let spans = get_or_extract_spans(doc, index, spans_cache);

    match doc.render_page(index, &mut backend, initial_transform) {
        Ok(()) => {
            let scene = Arc::new(backend.scene().clone());
            let _ =
                tx.send(WorkerResponse::PageRendered { index, _scale: scale, scene, text, spans });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_render_failed",
                detail: Some(format!("{index}: {e}")),
            });
        }
    }
}

fn handle_audit(doc_opt: Option<&PdfDocument>, tx: &Sender<WorkerResponse>) {
    let Some(doc) = doc_opt else { return };
    let audit_findings = doc
        .audit_ua2()
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.checkpoint, f.severity, f.message, f.handle_id))
        .collect();
    let _ = tx.send(WorkerResponse::AuditFindings { findings: audit_findings });
}

/// Applies one operation and tells the window everything that changed with it.
///
/// `Busy` and `Idle` stay in the arm that calls this rather than moving in with the work:
/// `scripts/audit/progress.py` reads the arms, and an arm whose saying has been factored
/// out is an arm it cannot see saying anything.
///
/// **The page count is compared rather than derived from the operation.** Most of the
/// vocabulary leaves it alone, several arms change it and the window adjusts its own
/// count before sending those — but `InsertFrom` adds as many pages as the file it is
/// given holds, which is not knowable until it has been opened here. Reading the count
/// on both sides of the apply covers that without this thread having to know which
/// operations can move a page.
fn handle_apply(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    operation: Operation,
    done: String,
    tx: &Sender<WorkerResponse>,
) {
    let before = page_count_of(doc.as_ref());
    apply_recorded(doc, history, operation, Some(done), tx);
    if page_count_of(doc.as_ref()) != before {
        send_page_sizes(doc.as_ref(), tx);
    }
}

/// How many pages the document has, or none when there is no document.
fn page_count_of(doc: Option<&PdfDocument>) -> Option<usize> {
    doc.and_then(|d| d.page_count().ok())
}

/// Sends every page's size, which is also how the window learns the new count.
fn send_page_sizes(doc: Option<&PdfDocument>, tx: &Sender<WorkerResponse>) {
    let Some(doc) = doc else { return };
    let Ok(count) = doc.page_count() else { return };
    let mut page_sizes = Vec::with_capacity(count);
    for index in 0..count {
        page_sizes.push(doc.get_page_size(index).unwrap_or((595.0, 842.0)));
    }
    let _ = tx.send(WorkerResponse::PagesChanged { page_sizes });
}

/// Extracts `indices` into a document of its own, and takes them out of this one when
/// asked.
///
/// **Written before removed, and the removal is skipped if the write failed.** The two
/// halves of "move these pages out" are a write and a delete, and a delete whose write
/// did not happen is pages gone with nothing to show for them. The removal is recorded
/// like every other operation, so it is also undoable.
fn handle_extract(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    what: (&[usize], bool, &str),
    tx: &Sender<WorkerResponse>,
) {
    let (indices, remove, name) = what;
    let Some(source) = doc.as_ref() else { return };
    let extracted = match source.extract_pages(indices.to_vec()) {
        Ok(out) => out,
        Err(why) => return fail(tx, "notice_operation_failed", format!("{why:?}")),
    };
    let path = extraction_path(name);
    if let Err(why) = extracted.save_as_version(&path, "2.0") {
        return fail(tx, "notice_export_failed", format!("{why:?}"));
    }
    let _ = tx.send(WorkerResponse::PagesExtracted { path });

    if remove {
        let taken = PageSelection::Indices(indices.to_vec());
        apply_recorded(doc, history, Operation::RemovePages(taken), None, tx);
        send_page_sizes(doc.as_ref(), tx);
    }
}

/// Where an extracted document is written: in the temporary directory, under the
/// plainest name not already taken.
///
/// **The name carries the uniqueness, and only when it has to.** The window's title is
/// its file's name, so a timestamp added to keep names apart would be a timestamp in the
/// title; a suffix that appears only on the second extraction of the same pages keeps
/// the common case plain. Nothing is deleted to make room — the first extraction's
/// window may still be showing that file.
fn extraction_path(stem: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let plain = dir.join(format!("{stem}.pdf"));
    if !plain.exists() {
        return plain;
    }
    // Two is where a person starts counting copies. The ceiling is a ceiling rather than
    // a belief about how many a reader might make: past it the plain name is reused.
    (2..1000)
        .map(|n| dir.join(format!("{stem} ({n}).pdf")))
        .find(|taken| !taken.exists())
        .unwrap_or(plain)
}

/// Reports a failure the reader is waiting on.
fn fail(tx: &Sender<WorkerResponse>, key: &'static str, detail: String) {
    let _ = tx.send(WorkerResponse::Failed { key, detail: Some(detail) });
}

/// Retags an element, and re-reads what that did to the document's compliance.
///
/// **Recorded like every other mutation.** It reached `doc.apply` directly until UI-6's
/// check went looking: editing a tag changes the document, so an undo that did not cover
/// it left the reader with a history that was silently incomplete — and nothing said
/// which edits it held.
/// Applies a move and sends the tree back as the file now holds it.
///
/// **The tree is re-read rather than rearranged in place.** A move can be refused — a
/// cycle, an element the tree does not hold — and a window that had already rearranged
/// itself would be showing an order the file does not have.
fn handle_move_node(
    doc_opt: &mut Option<PdfDocument>,
    history: &mut History,
    move_: fepdf::StructElemMove,
    tx: &Sender<WorkerResponse>,
) {
    apply_recorded(doc_opt, history, Operation::MoveStructElem(move_), None, tx);
    let root = doc_opt.as_ref().and_then(|doc| resolve_struct_tree_root(doc, &mut 0));
    let _ = tx.send(WorkerResponse::StructTreeChanged { root: root.map(Box::new) });
}

fn handle_update_node(
    doc_opt: &mut Option<PdfDocument>,
    history: &mut History,
    handle_id: u32,
    tag: String,
    alt_text: Option<String>,
    tx: &Sender<WorkerResponse>,
) {
    let before = history.applied.len();
    // Reported, not discarded. The audit below reads the tree as it now stands, so a
    // failed edit sent the user a fresh set of findings for the *unchanged* document —
    // the one screen that would have told them the edit did not take was the screen
    // that showed the old tree as if it were the new one.
    apply_recorded(
        doc_opt,
        history,
        Operation::UpdateStructElem(fepdf::StructElemUpdate {
            handle_index: handle_id,
            new_tag: Some(tag),
            new_alt: alt_text,
        }),
        None,
        tx,
    );
    if history.applied.len() == before {
        return;
    }
    let Some(doc) = doc_opt else { return };

    // Run Matterhorn compliance audit on updated tree
    let findings = doc
        .audit_ua2()
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f.checkpoint, f.severity, f.message, f.handle_id))
        .collect();
    let _ = tx.send(WorkerResponse::AuditFindings { findings });
}

fn handle_save(
    // RR-15 Limit: Dispatcher - Thread pool worker saving request routing dispatcher handling signatures, redactions and compression saving options
    doc_opt: Option<&PdfDocument>,
    path: std::path::PathBuf,
    password: Option<String>,
    owner_password: Option<String>,
    compress: bool,
    linearize: bool,
    upgrade_pdf20: bool,
    redaction_zones: Vec<crate::redaction::RedactionZone>,
    cert_path: Option<std::path::PathBuf>,
    key_path: Option<std::path::PathBuf>,
    signature_position: Option<(usize, [f32; 4])>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc_opt else {
        let _ = tx.send(WorkerResponse::Failed { key: "notice_save_nothing", detail: None });
        return;
    };

    // 1. Group redaction zones by page index
    let mut page_redactions: std::collections::BTreeMap<usize, Vec<[f32; 4]>> =
        std::collections::BTreeMap::new();
    for zone in redaction_zones {
        let rect_arr = [zone.rect.min.x, zone.rect.min.y, zone.rect.max.x, zone.rect.max.y];
        page_redactions.entry(zone.page_index).or_default().push(rect_arr);
    }

    // 2. Apply physical stream sanitization to each page mutably
    for (page_idx, rects) in page_redactions {
        if let Err(e) = doc.apply_redaction_to_page(page_idx, &rects) {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_redact_failed",
                detail: Some(format!("{page_idx}: {e}")),
            });
            return;
        }
    }

    let version = if upgrade_pdf20 { "2.0" } else { "1.7" };
    // 7.6: what protects the output, which the engine writes as AES-256 because that is
    // the one scheme PDF 2.0 does not deprecate (ADR-0015). An owner password with no user
    // password protects nothing, so it goes only where there is one to restrict.
    let options = fepdf::SaveOptions {
        compress,
        compression_level: 6,
        owner_password: password.as_ref().and(owner_password),
        password,
        ..fepdf::SaveOptions::default()
    };

    let res = if let (Some(certificate), Some(key)) = (cert_path, key_path) {
        // Read as-is and let the engine judge them. Reporting "not a PKCS#8 key" from
        // the layer that knows what a key is beats guessing here, and `unwrap_or_default`
        // used to turn an unreadable file into empty bytes and carry on.
        let read =
            |p: &std::path::Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
        match (read(&certificate), read(&key)) {
            (Ok(certificate), Ok(key)) => {
                let sign_opts = fepdf::SignOptions {
                    // Rule D: a frontend translates. The reason and location used to be
                    // invented here — "Tokyo, Japan", a support address nobody gave —
                    // and were signed into the document as if the user had said them.
                    certificate: Some(certificate),
                    private_key: Some(key),
                    page_index: signature_position.map_or(0, |(idx, _)| idx),
                    ..fepdf::SignOptions::default()
                };
                doc.save_signed(&path, version, &options, &sign_opts)
            }
            (Err(e), _) | (_, Err(e)) => {
                let _ =
                    tx.send(WorkerResponse::Failed { key: "notice_save_failed", detail: Some(e) });
                return;
            }
        }
    } else if linearize {
        doc.save_linearized(&path, version, &options)
    } else {
        doc.save_with_options(&path, version, &options)
    };

    match res {
        Ok(decisions) => {
            let notices = decisions.iter().map(ToString::to_string).collect();
            let _ = tx.send(WorkerResponse::DocumentSaved { path, notices });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_save_failed",
                detail: Some(e.to_string()),
            });
        }
    }
}

#[cfg(test)]
mod binding_direction {
    //! **Two files of 524 declare `/ViewerPreferences /Direction`**, so for almost every
    //! document Table 30's default of L2R is what applies and a vertically set book opens
    //! at the wrong end unless something looks at it.
    //!
    //! **The rule is vertical setting, not the Japanese language.** Japanese is set
    //! horizontally far more often than not — reports, papers, manuals — and binding
    //! follows the setting. Testing `/Lang` bound every Japanese document right to left,
    //! which is how `samples/fy05.pdf`, a horizontally set government report, came to open
    //! from the wrong end and lay its tiles out mirrored.
    //!
    //! **Presence of a vertical font was not enough either.** `fy05.pdf` has 6 of them
    //! among 316 — 1.9%, a table or a cover — against `samples/bokutokitan.pdf`'s 4 of 18,
    //! or 22%.
    //!
    //! **And the evidence is the declaration, not the name.** This used to look for `-V` in
    //! the font's name, which is a naming convention; `FontSummary::is_vertical` carries
    //! what `detect_wmode` reads from the encoding's CMap — the same reading the
    //! interpreter uses to place a glyph (9.7.5.2). It is a guess and is recorded as one
    //! where the reader can see it.
    use super::infer_binding;

    /// `vertical` fonts set vertically, out of `total`. The names are deliberately
    /// uninformative: what decides is the declared writing mode, and a test that fed the
    /// answer in through the name would be testing the old rule.
    fn fonts(vertical: usize, total: usize) -> Vec<fepdf::FontSummary> {
        (0..total)
            .map(|i| fepdf::FontSummary {
                name: format!("Font{i}"),
                font_type: "Type0".to_string(),
                is_embedded: true,
                is_type3: false,
                is_subset: false,
                encoding: if i < vertical { "Identity-V" } else { "Identity-H" }.to_string(),
                has_to_unicode: false,
                is_vertical: i < vertical,
                object_id: 0,
            })
            .collect()
    }

    /// A document set vertically opens from the right.
    #[test]
    fn a_document_set_vertically_binds_right_to_left() {
        assert_eq!(infer_binding(&fonts(4, 18), None).as_deref(), Some("R2L"));
        assert_eq!(infer_binding(&fonts(18, 18), None).as_deref(), Some("R2L"));
    }

    /// **The measured case.** `samples/bokutokitan.pdf` is 4 vertical fonts of 18 and is a
    /// vertically set book; `samples/fy05.pdf` is 6 of 316 and is a horizontally set report
    /// with a vertical table in it. A test for presence answered R2L for both.
    #[test]
    fn a_few_vertical_fonts_in_a_horizontal_document_are_not_a_vertical_document() {
        assert_eq!(infer_binding(&fonts(6, 316), Some("ja-JP")), None, "6 of 316 is not");
        assert_eq!(infer_binding(&fonts(4, 18), None).as_deref(), Some("R2L"), "4 of 18 is");
    }

    /// **The language is not the evidence.** Japanese is set horizontally more often than
    /// not, and binding follows the setting. Testing `/Lang` bound every Japanese document
    /// right to left.
    #[test]
    fn the_language_alone_decides_nothing() {
        assert_eq!(infer_binding(&fonts(0, 12), Some("ja-JP")), None);
        assert_eq!(infer_binding(&fonts(4, 18), Some("en-US")).as_deref(), Some("R2L"));
    }

    /// **The evidence is the declaration, not the name.** The rule used to look for `-V` in
    /// the font's name, so a font *named* for a vertical CMap decided it and a font that
    /// merely declared one did not. `NotoSerif-Vietnamese` was the control for the first
    /// half; this is the control for the second.
    #[test]
    fn a_name_decides_nothing_either_way() {
        let mut named_vertical = fonts(0, 4);
        for f in &mut named_vertical {
            f.name = "KozMinPr6N-Regular-V".to_string();
        }
        assert_eq!(infer_binding(&named_vertical, None), None, "a name is not a declaration");

        let mut declared_vertical = fonts(4, 4);
        for f in &mut declared_vertical {
            f.name = "NotoSerif-Vietnamese".to_string();
        }
        assert_eq!(
            infer_binding(&declared_vertical, None).as_deref(),
            Some("R2L"),
            "and a declaration stands whatever the font is called"
        );
    }

    /// The control. Without it, "vertical CJK binds right to left" and "everything binds
    /// right to left" are the same green test.
    #[test]
    fn anything_else_is_left_alone_for_the_standard_default() {
        assert_eq!(infer_binding(&fonts(0, 1), None), None);
        assert_eq!(infer_binding(&fonts(0, 40), Some("en-US")), None);
        assert_eq!(infer_binding(&[], None), None, "and a document with no fonts at all");
    }
}

#[cfg(test)]
mod history {
    use super::{History, Operation, PageSelection};

    fn remove(index: usize) -> Operation {
        Operation::RemovePages(PageSelection::Single(index))
    }

    /// A document with nothing applied to it is the file it came from.
    #[test]
    fn an_untouched_document_is_not_edited() {
        let history = History::new();
        assert!(!history.edited());
        assert!(history.applied.is_empty() && history.undone.is_empty());
    }

    /// Taking one back moves it across rather than dropping it, so it can come back.
    #[test]
    fn undo_moves_an_operation_across_and_redo_moves_it_home() {
        let mut history = History::new();
        history.applied.push(remove(0));
        history.applied.push(remove(1));

        let taken = history.applied.pop().expect("two were applied");
        history.undone.push(taken);
        assert_eq!(history.applied.len(), 1);
        assert!(history.edited(), "one operation still stands");

        let back = history.undone.pop().expect("one was taken");
        history.applied.push(back);
        assert_eq!(history.applied.len(), 2);
        assert!(history.undone.is_empty());
    }

    /// **Undoing back to the start leaves the file it was opened from**, which is the
    /// property the close guard reads: a document undone to nothing is not edited, so
    /// closing it asks nothing.
    #[test]
    fn undoing_everything_is_not_an_edit() {
        let mut history = History::new();
        history.applied.push(remove(0));
        assert!(history.edited());

        let taken = history.applied.pop().expect("one was applied");
        history.undone.push(taken);
        assert!(!history.edited(), "back at the file it came from");
    }

    /// A new operation after an undo abandons what was undone. Keeping it would make the
    /// history a tree, and a second branch is a second thing the window has to explain.
    #[test]
    fn a_new_operation_abandons_what_was_undone() {
        let mut history = History::new();
        history.applied.push(remove(0));
        let taken = history.applied.pop().expect("one was applied");
        history.undone.push(taken);

        // What `apply_recorded` does on `Ok`.
        history.applied.push(remove(5));
        history.undone.clear();

        assert_eq!(history.applied, vec![remove(5)]);
        assert!(history.undone.is_empty(), "the abandoned branch is gone");
    }
}
