use crate::Handle;
use crate::Object;

/// Strategy for organizing the page tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTreeStrategy {
    /// A single flat level of page nodes.
    Flat,
    /// A balanced tree structure with a limit on kids per node.
    /// Rebuild the tree so no node holds more than `max_kids` children.
    Balanced {
        /// Maximum children per interior node.
        max_kids: usize,
    },
}

/// A virtual, read-only structured view of the page tree.
#[derive(Debug, Clone)]
pub enum PageTreeView<'a> {
    /// A flat slice of leaf page handles.
    Flat(&'a [Handle<Object>]),
    /// A balanced hierarchy of page tree views.
    /// A balanced interior node and the subtrees beneath it.
    Balanced {
        /// Maximum children per interior node.
        max_kids: usize,
        /// The subtrees beneath this node.
        nodes: Vec<PageTreeView<'a>>,
    },
}
