use std::collections::VecDeque;

use crate::xotdata::{Node, Xot};

/// Node in level order
///
/// Used by [`Xot::level_order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LevelOrder {
    /// A node
    Node(Node),
    /// The end of a sequence of nodes.
    End,
}

// traverse the tree in level order, meaning subsequent children are traversed first
// when a sequence of children comes to an end, a LevelOrder::End is yielded
pub(crate) fn level_order_traverse(xot: &Xot, node: Node) -> impl Iterator<Item = LevelOrder> + '_ {
    LevelOrderTraverse {
        xot,
        queue: VecDeque::from([node]),
        // we make last node the current node; that's okay as it's
        // popped right away and thus has the same parent as the current node
        last_node: node,
        pending: None,
        ended: false,
    }
}

struct LevelOrderTraverse<'a> {
    xot: &'a Xot,
    queue: VecDeque<Node>,
    last_node: Node,
    // a node to yield next, after the End that precedes it
    pending: Option<Node>,
    // whether the final End has been yielded
    ended: bool,
}

impl Iterator for LevelOrderTraverse<'_> {
    type Item = LevelOrder;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.pending.take() {
            return Some(LevelOrder::Node(node));
        }
        let Some(node) = self.queue.pop_front() else {
            if self.ended {
                return None;
            }
            self.ended = true;
            return Some(LevelOrder::End);
        };
        let parent_changed = self.xot.parent(self.last_node) != self.xot.parent(node);
        self.last_node = node;
        self.queue.extend(self.xot.children(node));
        if parent_changed {
            self.pending = Some(node);
            Some(LevelOrder::End)
        } else {
            Some(LevelOrder::Node(node))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_order_traverse() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<doc><a><b/><b/></a><a><b><c/><c/><c/></b><b/></a></doc>")
            .unwrap();
        let doc = xot.document_element(root).unwrap();
        let a0 = xot.first_child(doc).unwrap();
        let a1 = xot.next_sibling(a0).unwrap();
        let b0 = xot.first_child(a0).unwrap();
        let b1 = xot.next_sibling(b0).unwrap();
        let b2 = xot.first_child(a1).unwrap();
        let b3 = xot.next_sibling(b2).unwrap();
        let c0 = xot.first_child(b2).unwrap();
        let c1 = xot.next_sibling(c0).unwrap();
        let c2 = xot.next_sibling(c1).unwrap();

        let iter = level_order_traverse(&xot, root);

        let v = iter.collect::<Vec<_>>();
        assert_eq!(
            v,
            vec![
                LevelOrder::Node(root),
                LevelOrder::End,
                LevelOrder::Node(doc),
                LevelOrder::End,
                LevelOrder::Node(a0),
                LevelOrder::Node(a1),
                LevelOrder::End,
                LevelOrder::Node(b0),
                LevelOrder::Node(b1),
                LevelOrder::End,
                LevelOrder::Node(b2),
                LevelOrder::Node(b3),
                LevelOrder::End,
                LevelOrder::Node(c0),
                LevelOrder::Node(c1),
                LevelOrder::Node(c2),
                LevelOrder::End,
            ]
        );
    }

    #[test]
    fn test_level_order_traverse_without_attributes_and_namespaces() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<a xmlns:x="urn:x" k="v"><b/></a>"#).unwrap();
        let a = xot.document_element(root).unwrap();
        let b = xot.first_child(a).unwrap();

        let iter = level_order_traverse(&xot, a);

        let v = iter.collect::<Vec<_>>();
        assert_eq!(
            v,
            vec![
                LevelOrder::Node(a),
                LevelOrder::End,
                LevelOrder::Node(b),
                LevelOrder::End,
            ]
        );
    }

    #[test]
    fn test_level_order_traverse_with_text() {
        let mut xot = Xot::new();
        let root = xot.parse("<doc><a>X</a><a>X<b/>X</a></doc>").unwrap();
        let doc = xot.document_element(root).unwrap();
        let a0 = xot.first_child(doc).unwrap();
        let a1 = xot.next_sibling(a0).unwrap();
        let x0 = xot.first_child(a0).unwrap();
        let x1 = xot.first_child(a1).unwrap();
        let b0 = xot.next_sibling(x1).unwrap();
        let x2 = xot.next_sibling(b0).unwrap();

        let iter = level_order_traverse(&xot, root);

        let v = iter.collect::<Vec<_>>();
        assert_eq!(
            v,
            vec![
                LevelOrder::Node(root),
                LevelOrder::End,
                LevelOrder::Node(doc),
                LevelOrder::End,
                LevelOrder::Node(a0),
                LevelOrder::Node(a1),
                LevelOrder::End,
                LevelOrder::Node(x0),
                LevelOrder::End,
                LevelOrder::Node(x1),
                LevelOrder::Node(b0),
                LevelOrder::Node(x2),
                LevelOrder::End,
            ]
        );
    }
}
