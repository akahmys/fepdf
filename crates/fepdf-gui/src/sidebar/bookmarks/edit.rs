//! Moving a bookmark about in a draft tree, with nothing drawn.
//!
//! A bookmark is addressed by its [`Path`] — the index of each level from the roots
//! down — because `OutlineNode` carries no identity of its own and two bookmarks may
//! share a title and a page. Every edit here is a removal followed by an insertion, so
//! that the one function that can lose a subtree is written once.

use fepdf::OutlineNode;

/// Where a bookmark sits: its index at each level, roots first.
pub type Path = Vec<usize>;

/// The bookmark at `path`, if the tree has one there.
pub fn at<'a>(items: &'a [OutlineNode], path: &[usize]) -> Option<&'a OutlineNode> {
    let (&index, rest) = path.split_first()?;
    let node = items.get(index)?;
    if rest.is_empty() { Some(node) } else { at(&node.children, rest) }
}

/// The bookmark at `path`, to be changed.
pub fn at_mut<'a>(items: &'a mut [OutlineNode], path: &[usize]) -> Option<&'a mut OutlineNode> {
    let (&index, rest) = path.split_first()?;
    let node = items.get_mut(index)?;
    if rest.is_empty() { Some(node) } else { at_mut(&mut node.children, rest) }
}

/// The list `path`'s last index refers into, and that index.
fn level<'a>(
    items: &'a mut Vec<OutlineNode>,
    path: &[usize],
) -> Option<(&'a mut Vec<OutlineNode>, usize)> {
    let (&last, parents) = path.split_last()?;
    let list = if parents.is_empty() { items } else { &mut at_mut(items, parents)?.children };
    Some((list, last))
}

/// Takes the bookmark at `path` out of the tree, with everything under it.
pub fn take(items: &mut Vec<OutlineNode>, path: &[usize]) -> Option<OutlineNode> {
    let (list, index) = level(items, path)?;
    (index < list.len()).then(|| list.remove(index))
}

/// Puts `node` at `path`, pushing whatever was there down.
///
/// Answers `false` — changing nothing — when `path` names no list, which is how a caller
/// that has already taken a node out learns it must put it back somewhere else.
pub fn put(items: &mut Vec<OutlineNode>, path: &[usize], node: OutlineNode) -> bool {
    let Some((list, index)) = level(items, path) else { return false };
    if index > list.len() {
        return false;
    }
    list.insert(index, node);
    true
}

/// Which way a bookmark is being moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Swap with the sibling above.
    Up,
    /// Swap with the sibling below.
    Down,
    /// Become the last child of the sibling above.
    In,
    /// Become the next sibling of the parent.
    Out,
}

/// Moves the bookmark at `path`, and answers where it ended up.
///
/// `None` means the move had nowhere to go — the first of its level cannot rise or
/// indent, the last cannot fall, a root cannot outdent — and the tree is untouched.
pub fn shift(items: &mut Vec<OutlineNode>, path: &[usize], how: Move) -> Option<Path> {
    let landing = landing(items, path, how)?;
    let node = take(items, path)?;
    if put(items, &landing, node) { Some(landing) } else { None }
}

/// Where a move would put the bookmark, worked out before it is detached.
///
/// **Before, because a detached bookmark that cannot be re-attached is gone** along with
/// every bookmark under it, and the reader's only record of it was the draft.
fn landing(items: &[OutlineNode], path: &[usize], how: Move) -> Option<Path> {
    let (&last, parents) = path.split_last()?;
    let mut landing = path.to_vec();
    match how {
        Move::Up => *landing.last_mut()? = last.checked_sub(1)?,
        Move::Down => {
            let width = siblings(items, parents)?;
            *landing.last_mut()? = (last + 1 < width).then_some(last + 1)?;
        }
        Move::In => {
            let mut into = parents.to_vec();
            into.push(last.checked_sub(1)?);
            let depth = at(items, &into)?.children.len();
            into.push(depth);
            landing = into;
        }
        Move::Out => {
            let (&parent, grandparents) = parents.split_last()?;
            landing = grandparents.to_vec();
            landing.push(parent + 1);
        }
    }
    Some(landing)
}

/// How many bookmarks sit at the level `path` names.
fn siblings(items: &[OutlineNode], path: &[usize]) -> Option<usize> {
    if path.is_empty() {
        return Some(items.len());
    }
    Some(at(items, path)?.children.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `a`, `b` with children `b1` and `b2`, then `c`.
    fn tree() -> Vec<OutlineNode> {
        vec![leaf("a"), node("b", vec![leaf("b1"), leaf("b2")]), leaf("c")]
    }

    fn leaf(title: &str) -> OutlineNode {
        node(title, Vec::new())
    }

    fn node(title: &str, children: Vec<OutlineNode>) -> OutlineNode {
        OutlineNode { title: title.into(), destination_page: 0, children }
    }

    /// Titles in the order a reader would read them down the panel.
    fn flat(items: &[OutlineNode]) -> Vec<String> {
        let mut out = Vec::new();
        for item in items {
            out.push(item.title.clone());
            out.extend(flat(&item.children));
        }
        out
    }

    #[test]
    fn a_path_finds_the_bookmark_at_each_level() {
        let items = tree();
        assert_eq!(at(&items, &[1]).map(|n| n.title.as_str()), Some("b"));
        assert_eq!(at(&items, &[1, 1]).map(|n| n.title.as_str()), Some("b2"));
        assert_eq!(at(&items, &[1, 2]), None);
        assert_eq!(at(&items, &[]), None);
    }

    #[test]
    fn taking_a_bookmark_takes_what_is_under_it() {
        let mut items = tree();
        let taken = take(&mut items, &[1]).expect("b is there");
        assert_eq!(taken.children.len(), 2);
        assert_eq!(flat(&items), ["a", "c"]);
    }

    #[test]
    fn moving_down_and_back_up_leaves_the_order_it_started_in() {
        let mut items = tree();
        let landed = shift(&mut items, &[0], Move::Down).expect("a can fall");
        assert_eq!(landed, vec![1]);
        assert_eq!(flat(&items), ["b", "b1", "b2", "a", "c"]);
        shift(&mut items, &landed, Move::Up).expect("a can rise again");
        assert_eq!(flat(&items), flat(&tree()));
    }

    #[test]
    fn indenting_makes_it_the_last_child_of_the_one_above() {
        let mut items = tree();
        let landed = shift(&mut items, &[2], Move::In).expect("c can indent under b");
        assert_eq!(landed, vec![1, 2], "it lands after b's existing two children");
        assert_eq!(flat(&items), ["a", "b", "b1", "b2", "c"]);
        assert_eq!(items.len(), 2, "c is no longer a root");
    }

    #[test]
    fn outdenting_makes_it_the_next_sibling_of_its_parent() {
        let mut items = tree();
        let landed = shift(&mut items, &[1, 0], Move::Out).expect("b1 can outdent");
        assert_eq!(landed, vec![2], "it lands directly after b");
        assert_eq!(flat(&items), ["a", "b", "b2", "b1", "c"]);
    }

    /// Every move that has nowhere to go changes nothing at all.
    ///
    /// **This is the test that matters.** A move is a take followed by a put, and a take
    /// whose put has nowhere to land loses the bookmark and everything beneath it — with
    /// no record of it anywhere but the draft the reader was editing.
    #[test]
    fn a_move_with_nowhere_to_go_leaves_the_tree_exactly_as_it_was() {
        let refusals = [
            (vec![0], Move::Up, "the first root cannot rise"),
            (vec![2], Move::Down, "the last root cannot fall"),
            (vec![0], Move::In, "the first of a level has nothing to indent under"),
            (vec![0], Move::Out, "a root has no parent to step out of"),
            (vec![9], Move::Down, "a path the tree does not have"),
        ];
        for (path, how, why) in refusals {
            let mut items = tree();
            assert!(shift(&mut items, &path, how).is_none(), "{why}");
            assert_eq!(flat(&items), flat(&tree()), "{why}: the tree was changed anyway");
        }
    }
}
