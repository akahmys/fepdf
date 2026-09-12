use crate::worker::WorkerRequest;
use serde::{Deserialize, Serialize};

/// Presentation node for structure tree hierarchy in GUI.
pub use fepdf::StructureTreeNode as USTNode;

#[derive(Serialize, Deserialize)]
pub struct USTRegistry {
    pub root: Option<USTNode>,
    pub selected_node_id: Option<usize>,
    pub next_node_id: usize,
    pub audit_findings: Vec<(String, String, String, Option<u32>)>, // (checkpoint, severity, message, handle_id)
    pub pending_center_node_id: Option<usize>,
}

impl Default for USTRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DragRelation {
    Above,
    Below,
    AsChild,
}

impl USTRegistry {
    pub fn new() -> Self {
        Self {
            root: None,
            selected_node_id: None,
            next_node_id: 1,
            audit_findings: Vec::new(),
            pending_center_node_id: None,
        }
    }

    pub fn clear(&mut self) {
        self.root = None;
        self.selected_node_id = None;
        self.next_node_id = 1;
        self.audit_findings.clear();
        self.pending_center_node_id = None;
    }

    pub fn find_node_id_by_handle_id(&self, handle_id: u32) -> Option<usize> {
        self.root.as_ref().and_then(|r| Self::find_node_id_by_handle_recursive(r, handle_id))
    }

    fn find_node_id_by_handle_recursive(node: &USTNode, handle_id: u32) -> Option<usize> {
        if node.handle_index == Some(handle_id) {
            return Some(node.id);
        }
        for child in &node.children {
            if let Some(id) = Self::find_node_id_by_handle_recursive(child, handle_id) {
                return Some(id);
            }
        }
        None
    }

    /// Resolves a node to the page it sits on and its bounding box in PDF user space.
    ///
    /// Nodes with no resolved `/Pg` fall back to the first page, which is what the
    /// viewport did unconditionally before `USTNode::page_index` existed.
    pub fn find_placement_by_id(&self, id: usize) -> Option<(usize, [f32; 4])> {
        let root = self.root.as_ref()?;
        let (page_index, rect) = Self::find_placement_recursive(root, id)?;
        Some((page_index.unwrap_or(0), rect))
    }

    fn find_placement_recursive(node: &USTNode, id: usize) -> Option<(Option<usize>, [f32; 4])> {
        if node.id == id {
            return node.rect.map(|r| (node.page_index, r));
        }
        for child in &node.children {
            if let Some(found) = Self::find_placement_recursive(child, id) {
                return Some(found);
            }
        }
        None
    }

    /// The request a finished drag becomes, or `None` when there is nothing to ask for.
    ///
    /// **Two of the three refusals are the engine's, and this makes neither of them.** A
    /// cycle and a target that is not in the tree are refused by `MoveStructElem`, which
    /// is where the file's own shape is known. What is decided here is only what the
    /// window can see: a drag onto itself, and an element with no handle — a tag drawn in
    /// this window stands for a selection and is in no file yet, so there is nothing to
    /// move.
    pub fn move_request(
        &self,
        dragged_id: usize,
        target_id: usize,
        relation: DragRelation,
    ) -> Option<WorkerRequest> {
        if dragged_id == target_id {
            return None;
        }
        let root = self.root.as_ref()?;
        let handle_index = Self::find_node_by_id_recursive(root, dragged_id)?.handle_index?;
        let target_index = Self::find_node_by_id_recursive(root, target_id)?.handle_index?;
        let placement = match relation {
            DragRelation::Above => fepdf::Placement::Before,
            DragRelation::Below => fepdf::Placement::After,
            DragRelation::AsChild => fepdf::Placement::Inside,
        };
        Some(WorkerRequest::MoveNode(fepdf::StructElemMove {
            handle_index,
            target_index,
            placement,
        }))
    }

    pub fn find_node_by_id_recursive(current: &USTNode, id: usize) -> Option<&USTNode> {
        if current.id == id {
            return Some(current);
        }
        for child in &current.children {
            if let Some(found) = Self::find_node_by_id_recursive(child, id) {
                return Some(found);
            }
        }
        None
    }

    pub fn is_descendant(parent: &USTNode, target_id: usize) -> bool {
        if parent.id == target_id {
            return true;
        }
        for child in &parent.children {
            if Self::is_descendant(child, target_id) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone)]
pub struct FigureInfo {
    pub id: usize,
    pub alt_text: Option<String>,
    pub handle_id: Option<u32>,
}

pub fn collect_figures(node: &USTNode, figures: &mut Vec<FigureInfo>) {
    if node.tag == "Figure" {
        figures.push(FigureInfo {
            id: node.id,
            alt_text: node.alt_text.clone(),
            handle_id: node.handle_index,
        });
    }
    for child in &node.children {
        collect_figures(child, figures);
    }
}

pub fn update_alt_text(node: &mut USTNode, id: usize, new_alt: Option<String>) -> bool {
    if node.id == id {
        node.alt_text = new_alt;
        return true;
    }
    for child in &mut node.children {
        if update_alt_text(child, id, new_alt.clone()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree with handles on it, since a move is addressed by handle and not by row.
    fn tagged_tree() -> USTRegistry {
        let leaf = |id: usize, handle: u32| USTNode {
            id,
            tag: "P".to_string(),
            title: "P".to_string(),
            alt_text: None,
            rect: None,
            page_index: Some(0),
            handle_index: Some(handle),
            mcids: Vec::new(),
            lang: None,
            role: None,
            children: Vec::new(),
        };
        let mut root = leaf(0, 10);
        root.tag = "Document".to_string();
        root.children = vec![leaf(1, 11), leaf(2, 12)];
        let mut registry = USTRegistry::new();
        registry.root = Some(root);
        registry
    }

    #[test]
    fn a_drag_becomes_a_request_addressed_by_handle() {
        // The window numbers its rows as it walks them; the file knows nothing of those
        // numbers. A move that named them would move whatever happened to be counted
        // second on that pass.
        let registry = tagged_tree();
        let request = registry.move_request(2, 1, DragRelation::Above).expect("a request");
        let WorkerRequest::MoveNode(moved) = request else {
            panic!("a drag became something other than a move");
        };
        assert_eq!((moved.handle_index, moved.target_index), (12, 11));
        assert_eq!(moved.placement, fepdf::Placement::Before);
    }

    #[test]
    fn the_three_relations_map_onto_the_three_placements() {
        let registry = tagged_tree();
        let placement = |relation| {
            let Some(WorkerRequest::MoveNode(moved)) = registry.move_request(2, 1, relation) else {
                panic!("a drag became something other than a move");
            };
            moved.placement
        };
        assert_eq!(placement(DragRelation::Above), fepdf::Placement::Before);
        assert_eq!(placement(DragRelation::Below), fepdf::Placement::After);
        assert_eq!(placement(DragRelation::AsChild), fepdf::Placement::Inside);
    }

    #[test]
    fn a_drag_onto_itself_asks_for_nothing() {
        assert!(tagged_tree().move_request(1, 1, DragRelation::Above).is_none());
    }

    #[test]
    fn an_element_that_is_in_no_file_yet_cannot_be_moved_in_one() {
        // A tag drawn in this window stands for a selection and carries no handle. The
        // cycle and the missing target are the engine's refusals, not this one's.
        let mut registry = tagged_tree();
        if let Some(root) = registry.root.as_mut() {
            root.children[0].handle_index = None;
        }
        assert!(registry.move_request(1, 2, DragRelation::Above).is_none());
    }

    fn node(id: usize, page_index: Option<usize>, rect: Option<[f32; 4]>) -> USTNode {
        USTNode {
            id,
            tag: "P".to_string(),
            title: format!("node {id}"),
            alt_text: None,
            rect,
            page_index,
            handle_index: None,
            mcids: Vec::new(),
            lang: None,
            role: None,
            children: Vec::new(),
        }
    }

    fn registry_with(children: Vec<USTNode>) -> USTRegistry {
        let mut registry = USTRegistry::new();
        let mut root = node(0, None, None);
        root.children = children;
        registry.root = Some(root);
        registry
    }

    #[test]
    fn find_placement_reports_the_node_own_page() {
        // Regression: the viewport used to hardcode page 0, so selecting a tag on a
        // later page highlighted and scrolled to the first page instead.
        let registry = registry_with(vec![
            node(1, Some(0), Some([10.0, 20.0, 30.0, 40.0])),
            node(2, Some(4), Some([50.0, 60.0, 70.0, 80.0])),
        ]);

        assert_eq!(registry.find_placement_by_id(1), Some((0, [10.0, 20.0, 30.0, 40.0])));
        assert_eq!(registry.find_placement_by_id(2), Some((4, [50.0, 60.0, 70.0, 80.0])));
    }

    #[test]
    fn find_placement_falls_back_to_first_page_when_pg_unresolved() {
        // Tags parsed from a PDF whose /Pg could not be resolved keep the old
        // behaviour rather than disappearing from the viewport.
        let registry = registry_with(vec![node(1, None, Some([1.0, 2.0, 3.0, 4.0]))]);
        assert_eq!(registry.find_placement_by_id(1), Some((0, [1.0, 2.0, 3.0, 4.0])));
    }

    #[test]
    fn find_placement_searches_nested_nodes() {
        let mut branch = node(1, Some(1), None);
        branch.children = vec![node(2, Some(7), Some([5.0, 5.0, 6.0, 6.0]))];
        let registry = registry_with(vec![branch]);
        assert_eq!(registry.find_placement_by_id(2), Some((7, [5.0, 5.0, 6.0, 6.0])));
    }

    #[test]
    fn find_placement_returns_none_without_a_rect_or_a_match() {
        let registry = registry_with(vec![node(1, Some(3), None)]);
        // A node carrying no bounding box has nothing to highlight.
        assert_eq!(registry.find_placement_by_id(1), None);
        assert_eq!(registry.find_placement_by_id(99), None);
    }

    #[test]
    fn ust_node_page_index_defaults_when_absent_from_a_draft() {
        // UST drafts written before page_index existed must still deserialize.
        let legacy = r#"{
            "id": 3,
            "tag": "H1",
            "title": "legacy",
            "alt_text": null,
            "rect": null,
            "handle_id": null,
            "children": []
        }"#;
        let parsed: USTNode = serde_json::from_str(legacy).expect("legacy draft should load");
        assert_eq!(parsed.page_index, None);
        assert_eq!(parsed.id, 3);
    }
}
