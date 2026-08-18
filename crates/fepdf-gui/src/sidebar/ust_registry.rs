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

    pub fn remove_node(&mut self, id: usize) -> Option<USTNode> {
        if let Some(ref mut root) = self.root {
            if root.id == id {
                return None;
            }
            return Self::remove_node_recursive(root, id);
        }
        None
    }

    fn remove_node_recursive(node: &mut USTNode, id: usize) -> Option<USTNode> {
        for idx in 0..node.children.len() {
            if node.children[idx].id == id {
                return Some(node.children.remove(idx));
            }
        }
        for child in &mut node.children {
            if let Some(removed) = Self::remove_node_recursive(child, id) {
                return Some(removed);
            }
        }
        None
    }

    pub fn move_node(
        &mut self,
        dragged_id: usize,
        target_id: usize,
        relation: DragRelation,
    ) -> bool {
        if dragged_id == target_id {
            return false;
        }

        if let Some(ref root) = self.root
            && let Some(dragged_node) = Self::find_node_by_id_recursive(root, dragged_id)
            && Self::is_descendant(dragged_node, target_id)
        {
            return false;
        }

        if let Some(dragged_node) = self.remove_node(dragged_id)
            && let Some(ref mut root) = self.root
            && Self::insert_node_recursive(root, target_id, dragged_node, relation).is_ok()
        {
            return true;
        }
        false
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

    fn insert_node_recursive(
        current: &mut USTNode,
        target_id: usize,
        node_to_insert: USTNode,
        relation: DragRelation,
    ) -> Result<(), USTNode> {
        if relation == DragRelation::AsChild && current.id == target_id {
            current.children.push(node_to_insert);
            return Ok(());
        }

        for idx in 0..current.children.len() {
            if current.children[idx].id == target_id {
                match relation {
                    DragRelation::Above => {
                        current.children.insert(idx, node_to_insert);
                        return Ok(());
                    }
                    DragRelation::Below => {
                        current.children.insert(idx + 1, node_to_insert);
                        return Ok(());
                    }
                    DragRelation::AsChild => {
                        current.children[idx].children.push(node_to_insert);
                        return Ok(());
                    }
                }
            }
        }

        let mut temp = Some(node_to_insert);
        for child in &mut current.children {
            if let Some(n) = temp.take() {
                match Self::insert_node_recursive(child, target_id, n, relation) {
                    Ok(()) => return Ok(()),
                    Err(n) => {
                        temp = Some(n);
                    }
                }
            }
        }

        if let Some(n) = temp { Err(n) } else { Ok(()) }
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

    #[test]
    fn test_move_node() {
        let mut registry = USTRegistry::new();
        let doc_node = USTNode {
            id: 0,
            tag: "Document".to_string(),
            title: "PDF Document Catalog".to_string(),
            alt_text: None,
            rect: None,
            page_index: None,
            handle_index: None,
            children: vec![USTNode {
                id: 1,
                tag: "Part".to_string(),
                title: "Page 1 Section".to_string(),
                alt_text: None,
                rect: None,
                page_index: None,
                handle_index: None,
                children: vec![
                    USTNode {
                        id: 2,
                        tag: "H1".to_string(),
                        title: "Heading of Page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                    USTNode {
                        id: 3,
                        tag: "P".to_string(),
                        title: "Paragraph content for page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                    USTNode {
                        id: 4,
                        tag: "Figure".to_string(),
                        title: "Illustration on page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                ],
            }],
        };
        registry.root = Some(doc_node);
        registry.next_node_id = 5;

        // Move Paragraph (id 3) Above Heading (id 2)
        assert!(registry.move_node(3, 2, DragRelation::Above));

        let root = registry.root.as_ref().unwrap();
        let page = &root.children[0];
        assert_eq!(page.children[0].id, 3);
        assert_eq!(page.children[1].id, 2);

        // Move Illustration (id 4) As Child of Paragraph (id 3)
        assert!(registry.move_node(4, 3, DragRelation::AsChild));

        let root = registry.root.as_ref().unwrap();
        let page = &root.children[0];
        let para = &page.children[0];
        assert_eq!(para.children[0].id, 4);

        // Invalid moves: dragging parent to child should fail
        assert!(!registry.move_node(3, 4, DragRelation::Above));
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
