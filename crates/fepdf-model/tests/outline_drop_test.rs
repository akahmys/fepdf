//! Releasing a deep outline does not abort the process.
//!
//! [ADR-0061](../../../docs/adr/0061-four-walks-bounded-and-two-that-were-not-what-the-sweep-said.md)
//! measured this and left it: `build_outline_level`'s recursion was real but was never
//! what crashed. Ten thousand levels aborts in **`OutlineNode`'s derived `Drop`**, which
//! releases `children` by recursing one stack frame per level, and no bound on any walk
//! moves that. The record named the fix — a manual `Drop` on the type — and called it a
//! change to a public type rather than to a walk, which is why it waited for its own
//! decision.
//!
//! **A stack overflow aborts the test binary rather than reddening one test**, so this
//! file stopping the run is the failure mode, not a red line in the report.

use fepdf_model::document::extensions::OutlineNode;

/// Builds `depth` levels, each holding the one below it.
fn nested(depth: usize) -> OutlineNode {
    let mut node = OutlineNode {
        title: "leaf".to_string(),
        destination_page: 0,
        children: Vec::new(),
        source: None,
    };
    for _ in 0..depth {
        node = OutlineNode {
            title: "level".to_string(),
            destination_page: 0,
            children: vec![node],
            source: None,
        };
    }
    node
}

/// Twenty thousand levels are released without touching the stack per level.
///
/// The measured abort was between 5,000 and 10,000, so 20,000 is past it with room.
#[test]
fn a_deep_outline_is_released_without_recursing() {
    drop(nested(20_000));
}

/// The release is still complete: a shallow tree keeps its shape until it is dropped.
///
/// A `Drop` that dismantles by moving children out could leave a node half-emptied if it
/// ran early; this holds that nothing observable changes before the value dies.
#[test]
fn a_shallow_outline_keeps_its_children_until_it_is_dropped() {
    let tree = nested(3);
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].children.len(), 1);
    assert_eq!(tree.children[0].children[0].children.len(), 1);
    assert!(tree.children[0].children[0].children[0].children.is_empty());
}
