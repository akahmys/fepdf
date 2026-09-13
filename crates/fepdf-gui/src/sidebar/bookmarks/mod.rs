//! The bookmark panel (ISO 32000-2, 12.3.3).
//!
//! **The reader edits a draft, and one button writes it.** `Operation::UpdateOutlines`
//! replaces the whole tree, so committing on every keystroke would put one operation per
//! character into the history — an undo that took back a letter at a time. The draft is
//! the panel's own copy; `適用` sends it as a single operation, and what the engine then
//! reads back replaces the draft, so the panel shows the file rather than its wishes.
//!
//! What the panel cannot offer is what `OutlineNode` cannot carry: Table 153's `/C`,
//! `/F`, `/SE` and `/A`, and whether an item is open. A document read here and written
//! back loses them, which is the operation's shape rather than the panel's.

pub mod edit;
mod view;

pub use view::{Asked, show};

use edit::Path;
use fepdf::{OutlineReport, OutlineTree};

/// The bookmark tree the reader is editing, and where it came from.
pub struct BookmarkPanel {
    /// As the engine last read it. Kept so the panel can say whether the draft differs.
    filed: OutlineTree,
    /// What the engine said about the read: how many bookmarks, and how many named no
    /// page of this document.
    report: OutlineReport,
    /// What the reader has made of it, not yet written.
    draft: OutlineTree,
    /// The bookmark whose title and page the fields below the list are editing.
    chosen: Option<Path>,
    /// Bookmarks whose children are hidden. Held by path, so it survives an edit no
    /// worse than the paths themselves do.
    folded: std::collections::BTreeSet<Path>,
}

impl Default for BookmarkPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl BookmarkPanel {
    /// A panel with no document behind it yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            filed: OutlineTree::default(),
            report: OutlineReport::default(),
            draft: OutlineTree::default(),
            chosen: None,
            folded: std::collections::BTreeSet::new(),
        }
    }

    /// Takes the tree the engine read, discarding any draft over it.
    ///
    /// **Discarding, because the engine has the last word.** This arrives after an
    /// operation the reader asked for, and a draft kept across it would be edits to a
    /// tree that no longer exists — page 12's bookmark after page 12 was removed.
    pub fn filed(&mut self, tree: OutlineTree, report: OutlineReport) {
        self.chosen = None;
        self.folded.clear();
        self.draft = tree.clone();
        self.filed = tree;
        self.report = report;
    }

    /// Whether the draft says something the file does not.
    #[must_use]
    pub fn edited(&self) -> bool {
        self.draft != self.filed
    }

    /// Chooses the bookmark at `path`, as clicking its row does.
    pub fn choose(&mut self, path: Path) {
        self.chosen = edit::at(&self.draft.items, &path).is_some().then_some(path);
    }

    /// Retitles the chosen bookmark, as typing in the title field does.
    pub fn retitle(&mut self, title: &str) {
        let Some(path) = self.chosen.clone() else { return };
        if let Some(node) = edit::at_mut(&mut self.draft.items, &path) {
            title.clone_into(&mut node.title);
        }
    }

    /// The draft, to be handed to `Operation::UpdateOutlines`.
    #[must_use]
    pub fn draft(&self) -> &OutlineTree {
        &self.draft
    }
}
