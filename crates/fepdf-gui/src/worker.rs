//! Off-thread document worker and request/response dispatch loop.

use crate::recovery::{Entry, EntryRef, Journal};
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
        /// What protects the output, if anything (7.6).
        protection: Protection,
        compress: bool,
        /// Take the descriptive metadata out: the Info dictionary and the XMP packet.
        strip: bool,
        linearize: bool,
        redaction_zones: Vec<crate::redaction::RedactionZone>,
        /// What the zones are filled with: an RGB colour, or `None` for no fill.
        redaction_fill: Option<[f32; 3]>,
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
    /// A read of the open document, answered with what was read.
    Read(Read),
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
    /// Rebuild the document a window that did not exit normally left in `dir`, and go on
    /// journaling there (ADR-0114). `password` opens a sealed journal and the document
    /// together, since the one sealed the other.
    Recover {
        dir: std::path::PathBuf,
        password: Option<String>,
    },
    /// The window is closing: end the session, removing its journal, and say so on `done`
    /// so the window waits for that and not longer.
    Shutdown {
        done: Sender<()>,
    },
    /// Put back the last operation `Undo` took.
    Redo,
    /// Put the pages of another document where these pages are, as one act.
    ReplacePages {
        /// The pages that go, counted from zero, in order.
        pages: Vec<usize>,
        /// The document whose pages come in, as bytes (an operation has to serialise).
        source: Vec<u8>,
        /// What to say when it worked, in the reader's language.
        done: String,
    },
    /// Write each of these pages as a PNG into a folder the reader chose.
    ExportImages {
        /// The pages, counted from zero, in order.
        pages: Vec<usize>,
        /// The folder.
        folder: std::path::PathBuf,
        /// What each file is called before its page number.
        stem: String,
    },
    /// Find text in every page of the document.
    Find {
        /// What to find.
        query: crate::finding::Query,
        /// Which search this is, handed back so the window can drop an answer to one it
        /// has since replaced.
        search: u64,
    },
    /// What redacting `regions` on `page` would remove, with nothing written.
    PreviewRedaction {
        /// Which page.
        page: usize,
        /// The zones on it, in PDF user space: left, bottom, right, top.
        regions: Vec<(f64, f64, f64, f64)>,
    },
    /// Rasterise a rectangle of a page, for the clipboard.
    Snapshot {
        /// Which page.
        page: usize,
        /// What to take, in the page's own space: left, bottom, right, top.
        keep: (f64, f64, f64, f64),
        /// What to take it at, as a multiple of a point.
        scale: f64,
    },
}

/// What protects a saved document (7.6), as the export wizard asked for it.
///
/// **A password or certificates, never both**: a document takes one security handler, and
/// 7.6.4 and 7.6.5 are different ones. The wizard offers them as one choice, and the
/// engine refuses both at once as well.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Protection {
    /// `/U` (7.6.4.4). `None` writes no password.
    pub password: Option<String>,
    /// `/O`, which is meaningless without a user password and is ignored then.
    pub owner_password: Option<String>,
    /// Certificates to encrypt to (7.6.5), as the files the reader chose.
    pub recipients: Vec<std::path::PathBuf>,
    /// What may be done with the document once open, as the keywords
    /// `fepdf::permission_keywords` lists, comma-separated. `None` grants everything.
    pub permissions: Option<String>,
}

impl Protection {
    /// The options a save is written with, given the certificates' bytes.
    ///
    /// **An owner password with no user password protects nothing**, so it goes only where
    /// there is one to restrict. The output is AES-256, because that is the one scheme
    /// PDF 2.0 does not deprecate (ADR-0015).
    #[must_use]
    pub fn save_options(
        self,
        recipients: Vec<Vec<u8>>,
        compress: bool,
        strip: bool,
    ) -> fepdf::SaveOptions {
        fepdf::SaveOptions {
            compress,
            compression_level: 6,
            strip,
            owner_password: self.password.as_ref().and(self.owner_password),
            password: self.password,
            recipients,
            permissions: self.permissions,
            ..fepdf::SaveOptions::default()
        }
    }
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
/// What `Open` was given: the bytes, the name, and the password.
type Origin = (Bytes, Option<String>, Option<String>);

struct History {
    /// What `Open` was given, kept so the document can be rebuilt from it. `Bytes` is
    /// refcounted and the arena already points into this buffer.
    origin: Option<Origin>,
    /// Applied, in order, one entry an act.
    ///
    /// **An act, not an operation**, because an undo takes back what the reader did and
    /// not what the engine was asked: replacing a page is `InsertFrom` and `RemovePages`,
    /// and an undo that restored the old page and left the new one beside it would be
    /// taking back half a click.
    applied: Vec<Vec<Operation>>,
    /// Taken back, most recent last. Emptied by any new act, because a branch in the
    /// history is a second thing to explain.
    undone: Vec<Vec<Operation>>,
    /// Where this history is written as it happens, so a crash loses none of it
    /// (ADR-0114).
    journal: Journal,
}

impl History {
    const fn new() -> Self {
        Self { origin: None, applied: Vec::new(), undone: Vec::new(), journal: Journal::Off }
    }

    /// A history journaled to `dir`, for a document opened from `origin`.
    fn journaled(dir: Option<&std::path::Path>, origin: Origin) -> Self {
        let journal = Journal::ready(dir.map(std::path::Path::to_path_buf));
        Self { origin: Some(origin), journal, ..Self::new() }
    }

    /// A recovered session's history: its origin, and its entries replayed onto an
    /// empty history as they were onto the one that crashed.
    fn recovered(recovered: &crate::recovery::Recovered, password: Option<String>) -> Self {
        let mut history = Self::new();
        history.origin = Some((recovered.origin.clone(), recovered.name.clone(), password));
        for entry in &recovered.entries {
            match entry {
                Entry::Act(act) => {
                    history.applied.push(act.clone());
                    history.undone.clear();
                }
                // A step with nothing to move moved nothing in the window either, so
                // replaying it as nothing is the same history.
                Entry::Undo => {
                    history.step(true);
                }
                Entry::Redo => {
                    history.step(false);
                }
            }
        }
        history
    }

    /// Writes `entry` to the journal; tells the reader, once, if it cannot.
    fn journal(&mut self, entry: &EntryRef<'_>, tx: &Sender<WorkerResponse>) {
        let Some((origin, name, _)) = self.origin.as_ref() else { return };
        if let Some(why) = self.journal.record(entry, origin, name.as_deref()) {
            let _ = tx
                .send(WorkerResponse::Failed { key: "notice_autosave_failed", detail: Some(why) });
        }
    }

    /// Ends the journal, removing it: what it held is no longer the reader's to lose.
    fn close_journal(&mut self, tx: &Sender<WorkerResponse>) {
        let journal = std::mem::replace(&mut self.journal, Journal::Off);
        if let Some(why) = journal.close() {
            let _ = tx
                .send(WorkerResponse::Failed { key: "notice_autosave_failed", detail: Some(why) });
        }
    }

    /// Whether the document differs from the file it was opened from.
    fn edited(&self) -> bool {
        !self.applied.is_empty()
    }

    /// Takes the last act back, or puts the last one taken back again, and says whether
    /// there was one to move.
    ///
    /// **Moved across rather than dropped**, so an undone act can come back.
    fn step(&mut self, undo: bool) -> bool {
        let (from, to) = if undo {
            (&mut self.applied, &mut self.undone)
        } else {
            (&mut self.undone, &mut self.applied)
        };
        from.pop().map(|act| to.push(act)).is_some()
    }

    /// Every operation still standing, in the order it was applied.
    fn operations(&self) -> Vec<Operation> {
        self.applied.concat()
    }
}

/// Everything the UI needs after a document finishes loading.
///
/// Kept behind a `Box` in [`WorkerResponse`] so that the far more frequent
/// `PageRendered` messages are not padded out to this variant's size.
pub struct LoadedDocument {
    pub name: Option<String>,
    pub num_pages: usize,
    /// Each page's box and turn, which the view lays out and maps points through.
    pub page_frames: Vec<crate::interaction::PageFrame>,
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
    /// What a page's redaction zones would remove, in PDF user space.
    RedactionPreview {
        /// Which page.
        page: usize,
        /// Every glyph, image area, path area and annotation that goes.
        going: Vec<[f32; 4]>,
    },
    /// Where a `Find` found its query, over every page.
    Found {
        /// The `search` the request carried.
        search: u64,
        /// Every match, in page order.
        found: Vec<crate::finding::Found>,
    },
    /// A rectangle of a page, rasterised, for the clipboard.
    ///
    /// **Rendered here and not in the window.** The document lives on this side, and a
    /// rasterisation on the drawing thread is a window that stops while it happens.
    SnapshotTaken {
        /// The pixels, RGBA.
        pixels: Vec<u8>,
        /// How many across.
        width: u32,
        /// How many down.
        height: u32,
    },
    /// How much of the Matterhorn protocol the audit looked at.
    ///
    /// **Sent with the findings and not instead of them.** An empty list of findings from
    /// a hundred and twelve of 137 failure conditions is not a document that conforms, and a reader shown
    /// one without the other is shown an assurance nobody gave.
    AuditScope {
        /// How many failure conditions were looked at.
        checked: usize,
        /// The ones the protocol leaves to a person, with its own wording.
        ///
        /// **A count would not do.** "48 more were not looked at" and "here are the 48
        /// questions the protocol expects you to answer" are the same number and
        /// different work, and only the second is something a reader can act on.
        left_to_a_person: Vec<fepdf::LeftToAPerson>,
        /// How many the protocol has.
        in_protocol: usize,
    },
    /// The document's form, as it stands now.
    ///
    /// **Sent after every change and not only on opening**, because filling a field
    /// changes the form: the drawer listing it has to list what the document has, not
    /// what the file had.
    FormChanged {
        /// The fields, in the order the document declares them.
        form: Box<fepdf::FormFields>,
    },
    PageRendered {
        index: usize,
        _scale: f64,
        scene: Arc<Scene>,
        text: Option<String>,
        spans: Option<Vec<crate::interaction::TextSpan>>,
        /// The page's runs, which are what a text edit names.
        ///
        /// Carried beside `spans` rather than instead of them: a span is what extraction
        /// read and a run is one show-text operator, and selection and editing want
        /// different ones.
        runs: Option<Vec<crate::interaction::RunBox>>,
    },
    AuditFindings {
        findings: Vec<crate::sidebar::AuditRow>,
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
        page_frames: Vec<crate::interaction::PageFrame>,
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
    /// The answer to `Scales`.
    Scales {
        /// Which page.
        page: usize,
        /// Its rectilinear scales, in viewport order.
        scales: Vec<fepdf::measure::Scale>,
        /// Its `/UserUnit`: how many 1/72 inch a unit of its user space is.
        user_unit: f64,
    },
    /// The answer to `Print`: what the spooler said, or why it was not asked.
    Printed {
        /// What to tell the reader.
        notice: crate::app::Notice,
    },
    /// The answer to `Read::Compare`: what was found, or why nothing was.
    Compared {
        /// How the two documents differ.
        comparison: Option<Box<fepdf::compare::Comparison>>,
        /// Why they could not be compared.
        why: Option<String>,
    },
    /// The answer to `Reading`.
    Reading {
        /// The passages in the structure's order, and the lexicons it names.
        reading: Box<fepdf::reading::Reading>,
    },
    /// The answer to `Survey`.
    Surveyed {
        /// What runs without the reader doing anything, and what the document can do.
        actions: Box<fepdf::ActionReport>,
        /// The share of what the file presents whose contents the engine reads.
        coverage: Option<fepdf::Coverage>,
        /// Every signature in the file as it was opened, and whether each verifies.
        signatures: Option<Box<fepdf::SignatureReport>>,
    },
    DocumentSaved {
        path: std::path::PathBuf,
        /// What the write cost, in the document's own terms. Empty for most files;
        /// non-empty when the source declared restrictions the output cannot carry,
        /// which the user is about to hand to someone else (7.6.4.2).
        notices: Vec<String>,
    },
    /// A journal to recover is sealed, and the password given does not open it.
    RecoveryLocked {
        /// Whether a password was offered, as `NeedsPassword` says.
        retried: bool,
    },
    /// Something did not happen. `key` frames it and `detail` is the engine's own
    /// sentence, which names an ISO clause and has no translation.
    Failed {
        key: &'static str,
        detail: Option<String>,
    },
}

/// `recovery` is where this window's session journals what it does (ADR-0114), or
/// `None` where the platform names no place for it.
pub fn run_worker(
    rx: Receiver<WorkerRequest>,
    tx: Sender<WorkerResponse>,
    ctx: egui::Context,
    recovery: Option<std::path::PathBuf>,
) {
    // RR-15 Limit: GUI - main routing message loop dispatcher for background worker thread
    let mut current_doc: Option<PdfDocument> = None;
    // The bytes the open read. `Bytes` is refcounted and the arena already points into
    // this buffer, so holding it costs a pointer rather than the file.
    let mut current_bytes: Option<Bytes> = None;
    let mut history = History::new();
    let system_fonts = VelloBackend::load_system_fonts();
    let mut pages = PageCache::default();

    for request in rx {
        match request {
            WorkerRequest::Open { data, name, password } => {
                pages.clear();
                current_bytes = Some(data.clone());
                let origin = (data, name, password);
                current_doc = open_requested(&mut history, recovery.as_deref(), origin, &tx);
                send_form(current_doc.as_ref(), &tx);
                ctx.request_repaint();
            }
            WorkerRequest::RenderPage { index, scale } => {
                handle_render(
                    current_doc.as_ref(),
                    index,
                    scale,
                    &tx,
                    Arc::clone(&system_fonts),
                    &mut pages,
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
                pages.clear();
                // Retagging re-runs the whole PDF/UA audit, which is the long half.
                let _ = tx.send(WorkerResponse::Busy { key: "busy_auditing" });
                handle_update_node(&mut current_doc, &mut history, handle_id, tag, alt_text, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            save @ WorkerRequest::Save { .. } => {
                pages.clear();
                let _ = tx.send(WorkerResponse::Busy { key: "busy_saving" });
                save_requested(&mut current_doc, &mut history, save, &tx);
                ctx.request_repaint();
            }
            WorkerRequest::ReplacePages { pages, source, done } => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_applying" });
                handle_replace(&mut current_doc, &mut history, (pages, source), done, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::ExportImages { pages, folder, stem } => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_exporting" });
                handle_export_images(current_doc.as_ref(), &pages, &folder, &stem, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::PreviewRedaction { page, regions } => {
                handle_redaction_preview(current_doc.as_ref(), page, regions, &tx);
            }
            WorkerRequest::Find { query, search } => {
                let busy = WorkerResponse::Busy { key: "busy_finding" };
                handle_find(current_doc.as_ref(), (&query, search), &mut pages, busy, &tx);
                ctx.request_repaint();
            }
            WorkerRequest::Snapshot { page, keep, scale } => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_rendering" });
                handle_snapshot(current_doc.as_ref(), page, keep, scale, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Apply { operation, done } => {
                pages.clear();
                // `Retag` rebuilds the structure tree from heuristics; the others are
                // quick, and one arm cannot tell which it was handed.
                let _ = tx.send(WorkerResponse::Busy { key: "busy_applying" });
                handle_apply(&mut current_doc, &mut history, vec![*operation], done, &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::ExtractPages { indices, remove, name } => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_exporting" });
                handle_extract(&mut current_doc, &mut history, (&indices, remove, &name), &tx);
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Read(read) => {
                let _ = tx.send(WorkerResponse::Busy { key: read.busy() });
                let _ = tx.send(answer(current_doc.as_ref(), read));
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Survey => {
                let _ = tx.send(WorkerResponse::Busy { key: "busy_surveying" });
                handle_survey(current_doc.as_ref(), current_bytes.as_ref(), &tx);
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
                        pages.clear();
                        let _ =
                            tx.send(WorkerResponse::LayersChanged { layers: doc.layers().rows });
                    }
                }
                ctx.request_repaint();
            }
            WorkerRequest::ReorderPagesBatch { source_indices, target_insert_pos } => {
                pages.clear();
                record(
                    &mut current_doc,
                    &mut history,
                    Operation::ReorderBatch { sources: source_indices, target: target_insert_pos },
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::RemovePages { mut indices } => {
                pages.clear();
                // One operation, not a descending loop. Sorting the indices so that
                // removing one did not move the next was the frontend doing the engine's
                // arithmetic; `RemovePages` takes the set and owns the order.
                indices.sort_unstable();
                indices.dedup();
                record(
                    &mut current_doc,
                    &mut history,
                    Operation::RemovePages(PageSelection::Indices(indices)),
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::DuplicatePage { index } => {
                pages.clear();
                record(
                    &mut current_doc,
                    &mut history,
                    Operation::DuplicatePages(PageSelection::Single(index)),
                    None,
                    &tx,
                );
                ctx.request_repaint();
            }
            WorkerRequest::RotatePages { indices, delta } => {
                pages.clear();
                record(
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
            WorkerRequest::Recover { dir, password } => {
                pages.clear();
                let _ = tx.send(WorkerResponse::Busy { key: "busy_recovering" });
                if let Some(doc) = handle_recover(&mut history, (dir, password), &tx) {
                    current_bytes = history.origin.as_ref().map(|(data, ..)| data.clone());
                    current_doc = Some(doc);
                    send_form(current_doc.as_ref(), &tx);
                }
                let _ = tx.send(WorkerResponse::Idle);
                ctx.request_repaint();
            }
            WorkerRequest::Shutdown { done } => {
                history.close_journal(&tx);
                let _ = done.send(());
                break;
            }
            step @ (WorkerRequest::Undo | WorkerRequest::Redo) => {
                pages.clear();
                let undo = matches!(step, WorkerRequest::Undo);
                let key = if undo { "history_undoing" } else { "history_redoing" };
                let _ = tx.send(WorkerResponse::Busy { key });
                if history.step(undo) {
                    history.journal(if undo { &EntryRef::Undo } else { &EntryRef::Redo }, &tx);
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
    act: Vec<Operation>,
    done: Option<String>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(current) = doc.as_mut() else { return };
    let mut result = Ok(());
    for (nth, operation) in act.iter().enumerate() {
        result = current.apply(operation.clone());
        if result.is_err() {
            // **Half an act is not left standing.** What the act had already changed is
            // not in the history, so rebuilding from the history takes it back out.
            if nth > 0 {
                *doc = rebuild(history, tx);
            }
            break;
        }
    }
    let Some(doc) = doc.as_mut() else { return };
    match result {
        Ok(()) => {
            history.journal(&EntryRef::Act(&act), tx);
            history.applied.push(act);
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

/// Applies one operation as an act of its own.
fn record(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    operation: Operation,
    done: Option<String>,
    tx: &Sender<WorkerResponse>,
) {
    apply_recorded(doc, history, vec![operation], done, tx);
}

/// Replaces `pages` with every page of `source`, recorded as one act.
///
/// **Inserted first, then removed**, and the source is opened here first to count its
/// pages, so both operations are known before either is applied: the pages that go are
/// the same pages moved along by that many. A source that does not open fails here,
/// before anything has changed.
fn handle_replace(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    (pages, source): (Vec<usize>, Vec<u8>),
    done: String,
    tx: &Sender<WorkerResponse>,
) {
    let Some(first) = pages.first().copied() else { return };
    let options = fepdf::IngestionOptions::default();
    let count = PdfDocument::open_with_options(Bytes::from(source.clone()), &options)
        .and_then(|incoming| incoming.page_count());
    let added = match count {
        Ok(added) => added,
        Err(e) => {
            let detail = Some(format!("{e:?}"));
            let _ = tx.send(WorkerResponse::Failed { key: "notice_operation_failed", detail });
            return;
        }
    };
    let moved = pages.iter().map(|page| page + added).collect();
    let act = vec![
        Operation::InsertFrom { source, at: first },
        Operation::RemovePages(fepdf::PageSelection::Indices(moved)),
    ];
    handle_apply(doc, history, act, done, tx);
}

/// Writes each page as `<stem>-<page>.png` in `folder`, and says where.
///
/// **A read, not an operation**: the document does not change, so nothing is recorded,
/// the way `extract_text` and the snapshot are reads. The pages are numbered as the reader
/// counts them, padded so a folder lists them in order.
fn handle_export_images(
    doc: Option<&PdfDocument>,
    pages: &[usize],
    folder: &std::path::Path,
    stem: &str,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc else { return };
    let width = doc.page_count().unwrap_or(0).to_string().len();
    for &page in pages {
        let path = folder.join(format!("{stem}-{:0width$}.png", page + 1));
        if let Err(why) = doc.render_page_to_file(page, &path) {
            let detail = Some(format!("{}: {why}", path.display()));
            let _ = tx.send(WorkerResponse::Failed { key: "notice_export_failed", detail });
            return;
        }
    }
    let _ =
        tx.send(WorkerResponse::DocumentSaved { path: folder.to_path_buf(), notices: Vec::new() });
}

/// Opens what `Open` was given, with a new history journaled to `recovery`, and ends
/// the journal of the document it replaces.
///
/// **Sealed when the document is encrypted**, with the password it opened with: the
/// journal holds what the reader typed, and the file was protected so that would not be
/// on disk in the clear (ADR-0114).
fn open_requested(
    history: &mut History,
    recovery: Option<&std::path::Path>,
    (data, name, password): Origin,
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    history.close_journal(tx);
    *history = History::journaled(recovery, (data.clone(), name.clone(), password.clone()));
    let doc = handle_open(data, name, password.clone(), &[], tx);
    if doc.as_ref().is_some_and(PdfDocument::is_encrypted) {
        history.journal.seal_with(password.unwrap_or_default());
    }
    doc
}

/// Rebuilds the document a crashed window journaled in `dir`, makes its history this
/// one, and goes on journaling there.
///
/// **The history is replaced only once the journal has read.** A wrong password leaves
/// whatever is open as it was, and asks again.
fn handle_recover(
    history: &mut History,
    (dir, password): (std::path::PathBuf, Option<String>),
    tx: &Sender<WorkerResponse>,
) -> Option<PdfDocument> {
    let recovered = match crate::recovery::recover(&dir, password.as_deref()) {
        Ok(recovered) => recovered,
        Err(crate::recovery::RecoveryError::WrongPassword) => {
            let _ = tx.send(WorkerResponse::RecoveryLocked { retried: password.is_some() });
            return None;
        }
        Err(e) => {
            let detail = Some(e.to_string());
            let _ = tx.send(WorkerResponse::Failed { key: "notice_recovery_failed", detail });
            return None;
        }
    };
    history.close_journal(tx);
    *history = History::recovered(&recovered, password);
    let lost_tail = recovered.lost_tail;
    history.journal = match crate::recovery::Session::resume(dir, recovered) {
        Ok(session) => Journal::Writing(session),
        Err(e) => {
            let detail = Some(e.to_string());
            let _ = tx.send(WorkerResponse::Failed { key: "notice_autosave_failed", detail });
            Journal::Failed
        }
    };
    if lost_tail {
        let _ = tx.send(WorkerResponse::Failed { key: "notice_recovery_lost_tail", detail: None });
    }
    rebuild(history, tx)
}

/// Opens the original bytes again and replays what is still in the history onto them.
fn rebuild(history: &History, tx: &Sender<WorkerResponse>) -> Option<PdfDocument> {
    let (data, name, password) = history.origin.as_ref()?;
    let _ = tx.send(WorkerResponse::HistoryChanged {
        can_undo: !history.applied.is_empty(),
        can_redo: !history.undone.is_empty(),
        edited: history.edited(),
    });
    handle_open(data.clone(), name.clone(), password.clone(), &history.operations(), tx)
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
            let page_frames =
                (0..num_pages).map(|i| crate::interaction::PageFrame::of(&doc, i)).collect();

            let mut next_id = 0;
            let mut ust_root = resolve_struct_tree_root(&doc, &mut next_id);

            if ust_root.is_none() {
                ust_root = Some(crate::sidebar::USTNode {
                    id: 0,
                    tag: "Document".to_string(),
                    // **No title, because this thread has no language to write one in.**
                    // It said `PDF Document Catalog (Untagged)` — the only row of the
                    // structure tree that was a sentence this product wrote rather than
                    // something read out of the document, and it was English in every
                    // language. A node with no title of its own is named by the panel.
                    title: String::new(),
                    alt_text: None,
                    rect: None,
                    page_index: None,
                    handle_index: None,
                    mcids: Vec::new(),
                    mark_pages: Vec::new(),
                    lang: None,
                    role: None,
                    actual_text: None,
                    expansion: None,
                    phoneme: None,
                    phonetic_alphabet: "ipa".to_owned(),
                    order: Vec::new(),
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
                page_frames,
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

/// Rasterises a rectangle of a page and sends it back for the clipboard.
///
/// A region that cannot be drawn is reported rather than logged: a snapshot that failed
/// has taken whatever was on the clipboard with it either way, so the reader has to be
/// told which happened.
fn handle_snapshot(
    doc: Option<&PdfDocument>,
    page: usize,
    keep: (f64, f64, f64, f64),
    scale: f64,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc else { return };
    match doc.render_region(page, keep, sheet_scale(doc, page, scale)) {
        Ok((pixels, width, height)) => {
            let _ = tx.send(WorkerResponse::SnapshotTaken { pixels, width, height });
        }
        Err(why) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "snapshot_failed",
                detail: Some(why.to_string()),
            });
        }
    }
}

/// `scale`, a multiple of 72 DPI on the sheet, as the multiple of `page`'s user space
/// `render_region` takes.
///
/// **`render_region` leaves `/UserUnit` to its caller** (Table 31), and the snapshot is
/// asked for in DPI, which is a claim about the sheet. On a page of `/UserUnit 10` the
/// snapshot the drawer calls 96 DPI came out at 9.6 until 2026-09-29.
fn sheet_scale(doc: &PdfDocument, page: usize, scale: f64) -> f64 {
    scale * doc.get_page_user_unit(page).unwrap_or(1.0)
}

/// Hands the drawer the form the document has now.
///
/// Read from the open document rather than from its bytes: the one being filled in has
/// been changed since it was opened, and serialising it again to ask what is in it would
/// answer about a file nobody has.
fn send_form(doc: Option<&PdfDocument>, tx: &Sender<WorkerResponse>) {
    let form = doc.map(|doc| fepdf::form_of(doc.inner())).unwrap_or_default();
    let _ = tx.send(WorkerResponse::FormChanged { form: Box::new(form) });
}

/// What this worker remembers about the pages it has read, and throws away together.
///
/// **They go stale together, because each describes the page as it was.** They were three
/// maps cleared by hand at eleven sites, which is one thing counted three times: a twelfth
/// site that cleared two of them would leave the window drawing boxes round text that had
/// moved, and nothing would say so.
#[derive(Default)]
struct PageCache {
    /// The page's text, as extraction read it.
    text: std::collections::BTreeMap<usize, String>,
    /// What extraction read and where it thinks it was, for selection.
    spans: std::collections::BTreeMap<usize, Vec<crate::interaction::TextSpan>>,
    /// The page's runs, which are what a text edit names.
    runs: std::collections::BTreeMap<usize, Vec<crate::interaction::RunBox>>,
    /// The page's text as a search reads it.
    found: std::collections::BTreeMap<usize, crate::finding::PageText>,
}

impl PageCache {
    /// Whether a search of `doc` has a page to read first.
    fn unread(&self, doc: &PdfDocument) -> bool {
        (0..doc.page_count().unwrap_or(0)).any(|page| !self.found.contains_key(&page))
    }

    /// Forgets every page. One call, because forgetting half of it is the trap.
    fn clear(&mut self) {
        self.text.clear();
        self.spans.clear();
        self.runs.clear();
        self.found.clear();
    }
}

/// What the document is and what it does, for the panel that asks.
///
/// **The signatures are the file's, as it was opened.** `/ByteRange` names offsets into
/// those bytes, and a `Document` has already normalised them away (ADR-0013); an edit made
/// here since does not change what was signed, and is not what the answer is about.
/// Writes `doc` as it stands to a file of its own and hands that to the spooler.
///
/// **Written as an export writes it**, so what is printed is what would be saved — the
/// edits in it — and not the file it was opened from.
fn print(doc: Option<&PdfDocument>, form: &crate::printing::PrintForm) -> crate::app::Notice {
    use crate::app::Notice;
    let (Some(doc), Some(platform)) = (doc, crate::speech::Platform::this()) else {
        return Notice::failed("print_no_platform");
    };
    let file = std::env::temp_dir().join(format!("fepdf_print_{}.pdf", std::process::id()));
    if let Err(why) = doc.save_as_version(&file, "2.0") {
        return Notice::failed("print_failed").about(why.to_string());
    }
    match crate::printing::print(platform, &file, form) {
        Ok(job) => Notice::done("print_sent").about(job),
        Err(crate::printing::PrintFailure::Refused(key)) => Notice::failed(key),
        Err(crate::printing::PrintFailure::Spooler(said)) => {
            Notice::failed("print_failed").about(said)
        }
    }
}

/// A read of the open document that changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// The document as it stands, written out and handed to the spooler (W-17).
    Print(Box<crate::printing::PrintForm>),
    /// How the open document differs from the one at `path` (ROADMAP W-18).
    Compare {
        /// The other document.
        path: std::path::PathBuf,
    },
    /// The document as a synthesiser reads it (`PdfDocument::reading`).
    Reading,
    /// The scales a page declares for measuring (12.9).
    Scales {
        /// Which page.
        page: usize,
    },
}

impl Read {
    /// What the window says while it happens.
    const fn busy(&self) -> &'static str {
        match self {
            Self::Print(_) => "busy_printing",
            Self::Compare { .. } | Self::Reading | Self::Scales { .. } => "busy_reading",
        }
    }
}

/// What `read` finds in `doc`; with no document, what an empty one would answer.
fn answer(doc: Option<&PdfDocument>, read: Read) -> WorkerResponse {
    match read {
        Read::Print(form) => WorkerResponse::Printed { notice: print(doc, &form) },
        Read::Reading => {
            let reading = doc.map(PdfDocument::reading).unwrap_or_default();
            WorkerResponse::Reading { reading: Box::new(reading) }
        }
        Read::Compare { path } => {
            let other = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| PdfDocument::open(bytes.into()).map_err(|e| e.to_string()));
            let compared = match (doc, other) {
                (Some(doc), Ok(other)) => {
                    fepdf::compare::compare(doc, &other, 72.0).map_err(|e| e.to_string())
                }
                (None, _) => Err(String::new()),
                (_, Err(why)) => Err(why),
            };
            match compared {
                Ok(comparison) => {
                    WorkerResponse::Compared { comparison: Some(Box::new(comparison)), why: None }
                }
                Err(why) => WorkerResponse::Compared { comparison: None, why: Some(why) },
            }
        }
        Read::Scales { page } => {
            let scales = doc.map(|doc| doc.scales_on(page)).unwrap_or_default();
            // A page that states no `/UserUnit`, or states one that cannot be read, is in
            // units of 1/72 inch (Table 31's default).
            let user_unit = doc.and_then(|doc| doc.get_page_user_unit(page).ok()).unwrap_or(1.0);
            WorkerResponse::Scales { page, scales, user_unit }
        }
    }
}

fn handle_survey(doc: Option<&PdfDocument>, bytes: Option<&Bytes>, tx: &Sender<WorkerResponse>) {
    let Some(doc) = doc else { return };
    let actions = fepdf::ActionReport::of(doc.inner()).unwrap_or_default();
    // Recorded as absent rather than as zero: a coverage this could not compute and a
    // document that presents nothing are different answers. The same for signatures.
    let coverage = bytes.and_then(|b| fepdf::Coverage::of(b).ok());
    let signatures = bytes.and_then(|b| fepdf::SignatureReport::survey(b).ok()).map(Box::new);
    let _ = tx.send(WorkerResponse::Surveyed { actions: Box::new(actions), coverage, signatures });
}

/// Finds `query` on every page, reading each page's text once for as long as it is
/// unchanged, and answering as search `search`.
///
/// **`busy` is said only when a page has to be read.** Reading every page of
/// `intel_sdm.pdf`'s 5,057 takes 4 s and a search of what has been read takes
/// milliseconds, so the first search says it is working and the ones typed after it do
/// not flash.
/// Answers what redacting `regions` on `page` would remove, or why it would be refused.
///
/// **Read, not done.** `what_redaction_removes` runs the test the redaction runs and writes
/// nothing, so what is shown is what applying will remove — and a page it would refuse,
/// for text it cannot place, says so now rather than at the save.
fn handle_redaction_preview(
    doc: Option<&PdfDocument>,
    page: usize,
    regions: Vec<(f64, f64, f64, f64)>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc else { return };
    if regions.is_empty() {
        let _ = tx.send(WorkerResponse::RedactionPreview { page, going: Vec::new() });
        return;
    }
    let redaction = fepdf::Redaction { page, regions, fill: None };
    match doc.what_redaction_removes(&redaction) {
        Ok(removal) => {
            #[allow(clippy::cast_possible_truncation)]
            let going = [removal.glyphs, removal.images, removal.paths, removal.annotations]
                .concat()
                .into_iter()
                .map(|(x0, y0, x1, y1)| [x0 as f32, y0 as f32, x1 as f32, y1 as f32])
                .collect();
            let _ = tx.send(WorkerResponse::RedactionPreview { page, going });
        }
        Err(e) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_redaction_refused",
                detail: Some(format!("{page}: {e}")),
            });
        }
    }
}

fn handle_find(
    doc: Option<&PdfDocument>,
    (query, search): (&crate::finding::Query, u64),
    pages: &mut PageCache,
    busy: WorkerResponse,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc else { return };
    let Ok(pattern) = query.pattern() else {
        // The window checks the pattern before it asks, and says what is wrong with it.
        return;
    };
    let reading = pages.unread(doc);
    if reading {
        let _ = tx.send(busy);
    }
    let mut found = Vec::new();
    for page in 0..doc.page_count().unwrap_or(0) {
        let text =
            pages.found.entry(page).or_insert_with(|| crate::finding::PageText::read(doc, page));
        found.extend(text.find(page, &pattern));
    }
    let _ = tx.send(WorkerResponse::Found { search, found });
    if reading {
        let _ = tx.send(WorkerResponse::Idle);
    }
}

/// The page's runs, as the window sees them, cached for as long as the page is unchanged.
fn get_or_read_runs(
    doc: &PdfDocument,
    index: usize,
    cache: &mut std::collections::BTreeMap<usize, Vec<crate::interaction::RunBox>>,
) -> Option<Vec<crate::interaction::RunBox>> {
    if let Some(cached) = cache.get(&index) {
        return Some(cached.clone());
    }
    let runs: Vec<crate::interaction::RunBox> = fepdf::text::runs_of_page(doc.inner(), index)
        .ok()?
        .into_iter()
        .map(|run| crate::interaction::RunBox {
            index: run.index,
            text: run.text,
            pieces: run.pieces,
            font: run.font,
            origin: egui::pos2(run.origin.0 as f32, run.origin.1 as f32),
            advance: egui::vec2(run.advance.0 as f32, run.advance.1 as f32),
            rise: egui::vec2(run.rise.0 as f32, run.rise.1 as f32),
        })
        .collect();
    cache.insert(index, runs.clone());
    Some(runs)
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
    pages: &mut PageCache,
) {
    let Some(doc) = doc_opt else { return };
    // **A page the document no longer has is not a page that failed.** The window asks for
    // the pages it shows, and an edit that makes fewer — combining fourteen onto four —
    // lands while its requests for the old ones are still queued; each was reported as
    // "could not draw page 13".
    if doc.page_count().is_ok_and(|count| index >= count) {
        return;
    }
    let r = doc.get_page_box(index).unwrap_or_else(|_| fepdf::Rect::new(0.0, 0.0, 595.0, 842.0));
    let rot = doc.get_page_rotation(index).unwrap_or(0);
    // The engine's table, not a copy of it: the copy drew a turned page mirrored.
    let (initial_transform, _, _) = fepdf::page_display_transform(r, rot, scale);
    let mut backend = VelloBackend::new(system_fonts);

    let text = get_or_extract_text(doc, index, &mut pages.text);
    let spans = get_or_extract_spans(doc, index, &mut pages.spans);
    let runs = get_or_read_runs(doc, index, &mut pages.runs);

    match doc.render_page(index, &mut backend, initial_transform) {
        Ok(()) => {
            let scene = Arc::new(backend.scene().clone());
            let _ = tx.send(WorkerResponse::PageRendered {
                index,
                _scale: scale,
                scene,
                text,
                spans,
                runs,
            });
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
    send_audit(doc, tx);
}

/// Audits the document and tells the window what was looked at as well as what was found.
///
/// **An audit that failed used to arrive as a clean report.** Both callers wrote
/// `.unwrap_or_default()`, so a structure tree that could not be read came back as an
/// empty list of findings — which is the answer a conforming document gives. It is said
/// now, and the scope comes with it either way.
fn send_audit(doc: &PdfDocument, tx: &Sender<WorkerResponse>) {
    match doc.audit_ua2_report() {
        Ok(report) => {
            let _ = tx.send(WorkerResponse::AuditScope {
                checked: report.scope.checked.len(),
                left_to_a_person: report.scope.left_to_a_person,
                in_protocol: report.scope.in_protocol,
            });
            let findings = report
                .findings
                .into_iter()
                .map(|f| crate::sidebar::AuditRow {
                    condition: f.checkpoint,
                    outcome: f.outcome,
                    message: f.message,
                    handle_id: f.handle_id,
                })
                .collect();
            let _ = tx.send(WorkerResponse::AuditFindings { findings });
        }
        Err(why) => {
            let _ = tx.send(WorkerResponse::Failed {
                key: "notice_audit_failed",
                detail: Some(why.to_string()),
            });
        }
    }
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
    act: Vec<Operation>,
    done: String,
    tx: &Sender<WorkerResponse>,
) {
    let before = page_frames_of(doc.as_ref());
    let moved = act.iter().any(Operation::moves_content);
    apply_recorded(doc, history, act, Some(done), tx);
    let after = page_frames_of(doc.as_ref());
    let resized = after != before;
    send_form(doc.as_ref(), tx);
    if resized {
        let _ = tx.send(WorkerResponse::PagesChanged { page_frames: after });
    }
    // **The structure tree is measured against the pages, so what moved them moved it.**
    // Its rectangles come from where each element's marked content actually drew, and the
    // reading-order overlay went on outlining where the content used to be. Re-read
    // rather than adjusted: the boxes are read from the document, and a second way of
    // arriving at them is a second answer.
    //
    // Comparing the sizes is not enough on its own — a `ResizePages` naming no sheet
    // leaves every page the size it was and moves everything on it — so the operation is
    // asked as well, by `Operation::moves_content`, which is where the vocabulary is.
    if resized || moved {
        let root = doc.as_ref().and_then(|doc| resolve_struct_tree_root(doc, &mut 0));
        let _ = tx.send(WorkerResponse::StructTreeChanged { root: root.map(Box::new) });
    }
}

/// Every page's size, which is what the window lays out from.
///
/// **Compared rather than counted.** This asked whether the page *count* had changed,
/// which `InsertFrom` and `RemovePages` alter and `ResizePages` does not: a document
/// resized to A3 went on being drawn at its old size because the window keeps its own
/// sizes and nothing told it. Comparing the sizes covers both, and covers the next
/// operation that moves a box without adding a page.
fn page_frames_of(doc: Option<&PdfDocument>) -> Vec<crate::interaction::PageFrame> {
    let Some(doc) = doc else { return Vec::new() };
    let Ok(count) = doc.page_count() else { return Vec::new() };
    (0..count).map(|index| crate::interaction::PageFrame::of(doc, index)).collect()
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
        record(doc, history, Operation::RemovePages(taken), None, tx);
        let _ = tx.send(WorkerResponse::PagesChanged { page_frames: page_frames_of(doc.as_ref()) });
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
    record(doc_opt, history, Operation::MoveStructElem(move_), None, tx);
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
    record(
        doc_opt,
        history,
        Operation::UpdateStructElem(fepdf::StructElemUpdate {
            handle_index: handle_id,
            new_tag: Some(tag),
            new_alt: alt_text,
            ..fepdf::StructElemUpdate::default()
        }),
        None,
        tx,
    );
    if history.applied.len() == before {
        return;
    }
    let Some(doc) = doc_opt else { return };

    // The tree changed, so what it was audited against did too.
    send_audit(doc, tx);
}

/// Saves as a `Save` request asks: its zones redacted first, as a recorded act, and the
/// save made only where that went through.
fn save_requested(
    current_doc: &mut Option<PdfDocument>,
    history: &mut History,
    request: WorkerRequest,
    tx: &Sender<WorkerResponse>,
) {
    let WorkerRequest::Save {
        path,
        protection,
        compress,
        strip,
        linearize,
        redaction_zones,
        redaction_fill,
        cert_path,
        key_path,
        signature_position,
    } = request
    else {
        return;
    };
    if redact_before_saving(current_doc, history, (redaction_zones, redaction_fill), tx) {
        handle_save(
            current_doc.as_ref(),
            path,
            protection,
            (compress, strip),
            linearize,
            cert_path,
            key_path,
            signature_position,
            tx,
        );
    }
    let _ = tx.send(WorkerResponse::Idle);
}

/// Applies the zones marked for redaction, one `Operation::Redact` a page filled with
/// `fill`, as one act in the history; whether the save may go ahead.
///
/// **An operation, recorded, rather than a write on the side.** The export scrubbed the
/// open document through a function of its own, which no history held, so an undo after
/// a save replayed a document with the redaction gone (Rule D, ROADMAP Y-10).
fn redact_before_saving(
    doc: &mut Option<PdfDocument>,
    history: &mut History,
    (zones, fill): (Vec<crate::redaction::RedactionZone>, Option<[f32; 3]>),
    tx: &Sender<WorkerResponse>,
) -> bool {
    if zones.is_empty() {
        return true;
    }
    let mut by_page: std::collections::BTreeMap<usize, Vec<(f64, f64, f64, f64)>> =
        std::collections::BTreeMap::new();
    for zone in zones {
        let r = zone.rect;
        by_page.entry(zone.page_index).or_default().push((
            f64::from(r.min.x),
            f64::from(r.min.y),
            f64::from(r.max.x),
            f64::from(r.max.y),
        ));
    }
    // The reader chose the colour, so it is named rather than left to the engine.
    let fill: Vec<f64> =
        fill.map(|rgb| rgb.iter().map(|c| f64::from(*c)).collect()).unwrap_or_default();
    let act: Vec<Operation> = by_page
        .into_iter()
        .map(|(page, regions)| {
            Operation::Redact(fepdf::Redaction { page, regions, fill: Some(fill.clone()) })
        })
        .collect();
    let recorded = history.applied.len();
    apply_recorded(doc, history, act, None, tx);
    history.applied.len() > recorded
}

fn handle_save(
    // RR-15 Limit: Dispatcher - Thread pool worker saving request routing dispatcher handling signatures, redactions and compression saving options
    doc_opt: Option<&PdfDocument>,
    path: std::path::PathBuf,
    protection: Protection,
    (compress, strip): (bool, bool),
    linearize: bool,
    cert_path: Option<std::path::PathBuf>,
    key_path: Option<std::path::PathBuf>,
    signature_position: Option<(usize, [f32; 4])>,
    tx: &Sender<WorkerResponse>,
) {
    let Some(doc) = doc_opt else {
        let _ = tx.send(WorkerResponse::Failed { key: "notice_save_nothing", detail: None });
        return;
    };

    // PDF 2.0, which is the one version this engine writes (see the facade's
    // `written_version`). An option to write 1.7 put that in the header and nothing else.
    let version = "2.0";
    // Certificates are read here, beside the save that needs them: one the reader cannot
    // read is said now, as the save failing, rather than written as no recipient at all.
    let mut recipients = Vec::with_capacity(protection.recipients.len());
    for path in &protection.recipients {
        match std::fs::read(path) {
            Ok(certificate) => recipients.push(certificate),
            Err(e) => {
                let detail = Some(format!("{}: {e}", path.display()));
                let _ = tx.send(WorkerResponse::Failed { key: "notice_save_failed", detail });
                return;
            }
        }
    }
    let options = protection.save_options(recipients, compress, strip);

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
                object_id: None,
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

    fn remove(index: usize) -> Vec<Operation> {
        vec![Operation::RemovePages(PageSelection::Single(index))]
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

        assert!(history.step(true), "there was one to take back");
        assert_eq!(history.applied.len(), 1);
        assert!(history.edited(), "one operation still stands");

        assert!(history.step(false), "there was one to put back");
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

    /// **A recovered history is the history that crashed**, undo and all: replaying the
    /// journal's entries onto an empty history leaves what stood and what was taken back
    /// where the window had them.
    #[test]
    fn a_journal_replays_into_the_history_it_recorded() {
        use crate::recovery::{Entry, EntryRef, Session};
        let dir =
            std::env::temp_dir().join(format!("fepdf-history-{}", crate::recovery::session_name()));
        let mut session =
            Session::begin(dir.clone(), b"origin", None, None).expect("a session begins");
        // Three acts, two taken back, one put back, then a new act abandoning the rest:
        // every move the window can make.
        for entry in [
            EntryRef::Act(&remove(0)),
            EntryRef::Act(&remove(1)),
            EntryRef::Act(&remove(2)),
            EntryRef::Undo,
            EntryRef::Undo,
            EntryRef::Redo,
            EntryRef::Undo,
            EntryRef::Undo,
            EntryRef::Redo,
        ] {
            session.append(&entry).expect("appended");
        }
        drop(session);

        let recovered = crate::recovery::recover(&dir, None).expect("recovered");
        assert_eq!(recovered.entries.len(), 9);
        assert!(matches!(recovered.entries.first(), Some(Entry::Act(_))));
        let history = History::recovered(&recovered, None);
        std::fs::remove_dir_all(&dir).expect("cleaned up");

        assert_eq!(history.applied, vec![remove(0)], "what stood");
        assert_eq!(history.undone, vec![remove(2), remove(1)], "what was taken back, in order");
        assert_eq!(
            history.origin.map(|(data, ..)| data),
            Some(bytes::Bytes::from_static(b"origin"))
        );
    }
}

#[cfg(test)]
mod finding {
    //! **A search covers the document, not the pages the window has drawn.** The studio
    //! used to search the spans the window had cached, so a word on a page nobody had
    //! scrolled to was reported as not there.
    use super::{PageCache, WorkerResponse, handle_find};
    use crate::finding::Query;
    use fepdf::{IngestionOptions, PdfDocument};

    /// Two pages, the second drawing `word`, and neither of them rendered.
    fn two_pages(word: &str) -> PdfDocument {
        let content = format!("BT /F1 12 Tf 1 0 0 1 30 700 Tm ({word}) Tj ET");
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R \
             /Resources << /Font << /F1 6 0 R >> >> >>"
                .to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        PdfDocument::open_with_options(
            fepdf_fixtures::assemble(&bodies).into(),
            &IngestionOptions::default(),
        )
        .expect("the fixture opens")
    }

    #[test]
    fn a_word_on_a_page_never_drawn_is_found() {
        let doc = two_pages("ACME");
        let mut pages = PageCache::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let query = Query { text: "acme".to_string(), regex: false, case_sensitive: false };

        handle_find(Some(&doc), (&query, 7), &mut pages, WorkerResponse::Idle, &tx);
        let found = rx.try_iter().find_map(|response| {
            if let WorkerResponse::Found { search, found } = response {
                Some((search, found))
            } else {
                None
            }
        });
        let Some((search, found)) = found else { panic!("the worker did not answer") };
        assert_eq!(search, 7, "the answer does not say which search it answers");
        assert_eq!(found.iter().map(|f| f.page).collect::<Vec<_>>(), vec![1]);
    }
}

#[cfg(test)]
mod saving {
    //! **What the export wizard asks for is what the file holds.** The wizard's choices
    //! reach `handle_save` as a `Protection` and a `strip`, and nothing else looks at the
    //! file that comes out — so this reads it back.
    use super::{Protection, WorkerResponse, handle_save};
    use fepdf::{Credentials, EncryptionReport, IngestionOptions, PdfDocument};

    fn titled() -> PdfDocument {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
            "<< /Title (A title that should go) /Author (Someone) >>".to_string(),
        ];
        let mut bytes = fepdf_fixtures::assemble(&bodies);
        // `assemble` writes no `/Info`; the trailer is the file's last dictionary, so
        // the entry goes in front of its `/Root`.
        let at = bytes.windows(5).rposition(|w| w == b"/Root").expect("a trailer");
        bytes.splice(at..at, b"/Info 4 0 R ".iter().copied());
        PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
            .expect("the fixture opens")
    }

    fn save(
        doc: &PdfDocument,
        protection: Protection,
        strip: bool,
    ) -> (Vec<WorkerResponse>, std::path::PathBuf) {
        // One file per call: the tests run in parallel in one process, and a shared name let
        // one test find the file another wrote, and fail on "a file was written anyway".
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("fepdf_gui_saving_{}_{call}.pdf", std::process::id()));
        let (tx, rx) = std::sync::mpsc::channel();
        handle_save(
            Some(doc),
            path.clone(),
            protection,
            (true, strip),
            false,
            None,
            None,
            None,
            &tx,
        );
        (rx.try_iter().collect(), path)
    }

    /// A password, one permission taken away, and the metadata stripped — each read back.
    #[test]
    fn the_saved_file_holds_what_the_wizard_asked_for() {
        let doc = titled();
        let protection = Protection {
            password: Some("open".to_string()),
            owner_password: Some("owner".to_string()),
            recipients: Vec::new(),
            permissions: Some(
                "print,modify,annotate,forms,accessibility,assemble,print-high".to_string(),
            ),
        };
        let (said, path) = save(&doc, protection, true);
        assert!(
            said.iter().any(|r| matches!(r, WorkerResponse::DocumentSaved { .. })),
            "the save did not report itself done"
        );
        let bytes = std::fs::read(&path).expect("the output is there");
        // A temporary file this test wrote; failing to delete it changes no answer.
        let _ = std::fs::remove_file(&path);

        let report = EncryptionReport::survey(&bytes, Credentials::password("open"))
            .expect("the output reads");
        assert!(report.encrypted, "the output is not encrypted");
        let copy = report.permissions.iter().find(|p| p.bit == 5).expect("bit 5 is reported");
        assert!(!copy.granted, "copying was taken away and the file grants it");
        let print = report.permissions.iter().find(|p| p.bit == 3).expect("bit 3 is reported");
        assert!(print.granted, "printing was left and the file denies it");

        let options =
            IngestionOptions { password: Some("open".to_string()), ..IngestionOptions::default() };
        let reopened = PdfDocument::open_with_options(bytes.into(), &options).expect("it opens");
        let metadata = reopened.metadata();
        assert_eq!(metadata.title, None, "the title was stripped and is still there");
        assert_eq!(metadata.author, None, "the author was stripped and is still there");
    }

    /// **A certificate that cannot be read fails the save, naming it**, rather than being
    /// written as no recipient — which would be a document encrypted to nobody, or not at
    /// all.
    #[test]
    fn an_unreadable_certificate_fails_the_save() {
        let doc = titled();
        let missing = std::path::PathBuf::from("/nonexistent/recipient.der");
        let protection = Protection { recipients: vec![missing], ..Protection::default() };
        let (said, path) = save(&doc, protection, false);
        let failure = said.iter().find_map(|r| {
            if let WorkerResponse::Failed { detail, .. } = r { detail.clone() } else { None }
        });
        assert!(
            failure.as_deref().is_some_and(|d| d.contains("recipient.der")),
            "the failure does not name the certificate: {failure:?}"
        );
        assert!(!path.exists(), "a file was written anyway");
    }
}

#[cfg(test)]
mod survey {
    //! The survey answers about signatures as well, from the bytes the document was
    //! opened from. `verify-signature` was the CLI's alone.
    use super::{WorkerResponse, handle_survey};
    use fepdf::{IngestionOptions, PdfDocument};

    /// **An unsigned file is answered as unsigned, not left unanswered.** `None` is what
    /// the panel shows when the survey could not read the file; a file with no signature
    /// is a report with none in it, and the two read differently to someone checking.
    #[test]
    fn an_unsigned_file_is_answered_as_unsigned() {
        let bytes = bytes::Bytes::from(fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        ]));
        let doc = PdfDocument::open_with_options(bytes.clone(), &IngestionOptions::default())
            .expect("the fixture opens");
        let (tx, rx) = std::sync::mpsc::channel();
        handle_survey(Some(&doc), Some(&bytes), &tx);

        let signatures = rx.try_iter().find_map(|response| {
            if let WorkerResponse::Surveyed { signatures, .. } = response {
                Some(signatures)
            } else {
                None
            }
        });
        let Some(Some(report)) = signatures else {
            panic!("the survey did not answer about signatures");
        };
        assert!(report.signatures.is_empty(), "an unsigned file reported a signature");
    }
}

#[cfg(test)]
mod replacing {
    //! **Replacing a page is one act**, `InsertFrom` and `RemovePages` recorded together,
    //! so the reader's undo takes back what they did rather than half of it.
    use super::{History, WorkerResponse, handle_export_images, handle_replace, rebuild};
    use fepdf::{IngestionOptions, PdfDocument};

    /// A document whose pages read `words`, one word a page.
    fn pages(words: &[&str]) -> Vec<u8> {
        let count = words.len();
        let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
        let mut bodies = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        for (i, word) in words.iter().enumerate() {
            let content = format!("BT /F1 24 Tf 1 0 0 1 20 100 Tm ({word}) Tj ET");
            bodies.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents {} 0 R \
                 /Resources << /Font << /F1 3 0 R >> >> >>",
                5 + 2 * i
            ));
            bodies.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
        }
        fepdf_fixtures::assemble(&bodies)
    }

    fn reads(doc: &PdfDocument) -> Vec<String> {
        (0..doc.page_count().expect("it counts"))
            .map(|page| doc.extract_text(page).expect("it reads").trim().to_string())
            .collect()
    }

    /// A document opened the way the worker opens one, with its history rooted in it.
    fn opened(words: &[&str]) -> (Option<PdfDocument>, History) {
        let bytes = bytes::Bytes::from(pages(words));
        let mut history = History::new();
        history.origin = Some((bytes.clone(), None, None));
        let doc = PdfDocument::open_with_options(bytes, &IngestionOptions::default())
            .expect("the fixture opens");
        (Some(doc), history)
    }

    #[test]
    fn a_page_is_replaced_by_every_page_of_the_source_and_undone_in_one_step() {
        let (mut doc, mut history) = opened(&["A", "B", "C"]);
        let (tx, _rx) = std::sync::mpsc::channel();
        handle_replace(&mut doc, &mut history, (vec![1], pages(&["X", "Y"])), String::new(), &tx);

        assert_eq!(reads(doc.as_ref().expect("a document")), ["A", "X", "Y", "C"]);
        assert_eq!(history.applied.len(), 1, "the replacement is not one act");

        assert!(history.step(true), "there was nothing to undo");
        let undone = rebuild(&history, &tx).expect("it rebuilds");
        assert_eq!(reads(&undone), ["A", "B", "C"], "one undo did not take it all back");
    }

    /// **Every page can be replaced**, which is why the insertion goes first: removing
    /// first would leave a document with no pages, which is refused.
    #[test]
    fn every_page_can_be_replaced() {
        let (mut doc, mut history) = opened(&["A", "B"]);
        let (tx, _rx) = std::sync::mpsc::channel();
        handle_replace(&mut doc, &mut history, (vec![0, 1], pages(&["X"])), String::new(), &tx);
        assert_eq!(reads(doc.as_ref().expect("a document")), ["X"]);
    }

    /// **Half a replacement is not left standing.** Page 9 is not there, so the removal
    /// is refused after the source is already in; the document goes back to what it was.
    #[test]
    fn a_refused_removal_leaves_the_document_as_it_was() {
        let (mut doc, mut history) = opened(&["A", "B"]);
        let (tx, rx) = std::sync::mpsc::channel();
        handle_replace(&mut doc, &mut history, (vec![1, 9], pages(&["X"])), String::new(), &tx);

        assert_eq!(reads(doc.as_ref().expect("a document")), ["A", "B"], "half of it stayed");
        assert!(history.applied.is_empty(), "a refused act was recorded");
        assert!(
            rx.try_iter().any(|r| matches!(r, WorkerResponse::Failed { .. })),
            "the refusal was not reported"
        );
    }

    /// **A source that does not open changes nothing and records nothing.**
    #[test]
    fn a_source_that_does_not_open_changes_nothing() {
        let (mut doc, mut history) = opened(&["A", "B"]);
        let (tx, rx) = std::sync::mpsc::channel();
        let broken = b"%PDF-1.7\nnot a document".to_vec();
        handle_replace(&mut doc, &mut history, (vec![1], broken), String::new(), &tx);

        assert_eq!(reads(doc.as_ref().expect("a document")), ["A", "B"], "the document changed");
        assert!(history.applied.is_empty(), "a refused act was recorded");
        assert!(
            rx.try_iter().any(|r| matches!(r, WorkerResponse::Failed { .. })),
            "the refusal was not reported"
        );
    }

    /// One PNG a page, named after the document and numbered as a reader counts.
    #[test]
    fn each_page_is_written_as_a_numbered_image() {
        let (doc, _) = opened(&["A", "B", "C"]);
        let folder = std::env::temp_dir().join(format!("fepdf_gui_images_{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        let (tx, rx) = std::sync::mpsc::channel();
        handle_export_images(doc.as_ref(), &[0, 2], &folder, "doc", &tx);

        let mut written: Vec<String> = std::fs::read_dir(&folder)
            .expect("the folder reads")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        written.sort();
        // A temporary folder this test made; failing to delete it changes no answer.
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(written, ["doc-1.png", "doc-3.png"]);
        assert!(
            rx.try_iter().any(|r| matches!(r, WorkerResponse::DocumentSaved { .. })),
            "the export did not say where it went"
        );
    }
}

/// What a snapshot's resolution is in the page's own units.
#[cfg(test)]
mod snapshot_scale {
    use super::sheet_scale;
    use fepdf::PdfDocument;

    fn page_with(entries: &str) -> PdfDocument {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] {entries} >>"),
        ];
        PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
    }

    /// **96 DPI is 96 DPI of the sheet**: on a page whose unit is ten points, the page's
    /// space is drawn ten times as large for it.
    #[test]
    fn a_user_unit_scales_the_snapshot() {
        assert!((sheet_scale(&page_with("/UserUnit 10"), 0, 4.0 / 3.0) - 40.0 / 3.0).abs() < 1e-9);
        assert!((sheet_scale(&page_with(""), 0, 4.0 / 3.0) - 4.0 / 3.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod redaction_preview {
    //! **What a zone will remove is shown before it is done** (ROADMAP Y-10): the window
    //! asks, and the worker answers from the test the redaction runs, writing nothing.
    use super::{WorkerResponse, handle_redaction_preview};
    use fepdf::PdfDocument;

    /// A page drawing `content` with Helvetica as `/F1`.
    fn page(content: &str) -> PdfDocument {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
               /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
    }

    /// **Every glyph the zone meets is named, and nothing is written.**
    #[test]
    fn the_preview_names_what_goes() {
        let doc = page("BT /F1 24 Tf 72 700 Td (AAA) Tj ET BT /F1 24 Tf 72 600 Td (BBB) Tj ET");
        let (tx, rx) = std::sync::mpsc::channel();
        handle_redaction_preview(Some(&doc), 0, vec![(60.0, 690.0, 300.0, 730.0)], &tx);
        let Ok(WorkerResponse::RedactionPreview { page, going }) = rx.try_recv() else {
            panic!("the worker gave no preview");
        };
        assert_eq!((page, going.len()), (0, 3), "{going:?}");
        assert!(
            doc.extract_text(0).expect("it reads").contains("AAA"),
            "the preview removed the text"
        );
    }

    /// **A page the redaction would refuse says so now**, rather than at the save.
    #[test]
    fn a_page_it_would_refuse_says_so() {
        let doc = page("BT /F9 24 Tf 72 700 Td (AAA) Tj ET");
        let (tx, rx) = std::sync::mpsc::channel();
        handle_redaction_preview(Some(&doc), 0, vec![(60.0, 690.0, 300.0, 730.0)], &tx);
        let Ok(WorkerResponse::Failed { key, .. }) = rx.try_recv() else {
            panic!("the refusal was not said");
        };
        assert_eq!(key, "notice_redaction_refused");
    }
}
