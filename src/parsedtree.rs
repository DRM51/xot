//! Trees that a [`Xot`] parsed into consecutive new slots, in document order.
//!
//! See [`ParsedTree`] and [`Xot::parsed_tree`].
use std::num::NonZeroU32;

use crate::arena::NodeId;
use crate::xotdata::{Node, Xot};

/// A tree that a [`Xot`] parsed and keeps in document order, as
/// [`Xot::parsed_tree`] gives it.
///
/// A parse makes the nodes of a document one after another: the document
/// node, then each element, followed by its namespace nodes, its attribute
/// nodes and its content. When it puts them in new slots, right after the
/// trees the `Xot` keeps already, the `Xot` keeps the tree, and its nodes sit
/// in storage in document order (the order of [`Xot::all_descendants`] of the
/// document node, namespace and attribute nodes included). The place of a
/// node in that order is then simple arithmetic on its handle:
/// [`ParsedTree::index`] gives it, and [`ParsedTree::node`] gives the node at
/// a place.
///
/// `index` and `node` describe the tree as it was parsed. They keep doing so
/// after the tree changes, but then [`Xot::parsed_tree`] no longer gives the
/// tree, which tells that its order may be another one now.
///
/// Two values are equal when they describe the same tree. They are ordered as
/// the trees were parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParsedTree {
    /// The position plus one of the slot of the document node.
    first: NonZeroU32,
    /// The position plus one of the slot of the last node of the tree.
    last: u32,
}

impl ParsedTree {
    /// The tree of the slots from `first` to `last`, positions plus one, with
    /// `first <= last`.
    #[inline]
    pub(crate) fn new(first: NonZeroU32, last: u32) -> Self {
        debug_assert!(first.get() <= last);
        ParsedTree { first, last }
    }

    /// The position plus one of the slot of the root.
    #[inline(always)]
    pub(crate) fn first(self) -> u32 {
        self.first.get()
    }

    /// The position plus one of the slot of the last node.
    #[inline(always)]
    pub(crate) fn last(self) -> u32 {
        self.last
    }

    /// Whether the slot at `index1` belongs to the tree.
    #[inline(always)]
    pub(crate) fn contains(self, index1: u32) -> bool {
        self.first.get() <= index1 && index1 <= self.last
    }

    /// The root of the tree: the document node that the parse gave.
    #[inline]
    pub fn root(self) -> Node {
        Node::new(NodeId::first_in(self.first))
    }

    /// How many nodes the parse made for the tree, the root included.
    #[inline]
    pub fn node_count(self) -> usize {
        (self.last - self.first.get()) as usize + 1
    }

    /// The place of `node` in the document order of the tree as it was
    /// parsed, 0 for the root, if `node` is one of the nodes the parse made;
    /// `None` otherwise.
    #[inline]
    pub fn index(self, node: Node) -> Option<usize> {
        let id = node.get();
        let index1 = id.index1();
        if id.generation() == 0 && self.contains(index1) {
            Some((index1 - self.first.get()) as usize)
        } else {
            None
        }
    }

    /// The node at `index` in the document order of the tree as it was
    /// parsed, if the tree has that many nodes.
    #[inline]
    pub fn node(self, index: usize) -> Option<Node> {
        let index = u32::try_from(index).ok()?;
        let index1 = self.first.checked_add(index)?;
        if index1.get() <= self.last {
            Some(Node::new(NodeId::first_in(index1)))
        } else {
            None
        }
    }
}

impl Xot {
    /// The parsed tree that `node` belongs to, if this `Xot` keeps it.
    ///
    /// A `Xot` keeps a tree when a parse ([`Xot::parse`],
    /// [`Xot::parse_fragment`] and the others that give a document node)
    /// succeeds and puts it in new slots right after the trees it keeps
    /// already: no slot of a removed node is free when the parse starts, and
    /// if the `Xot` keeps trees, no node was made after the last of them.
    /// Then the nodes of the tree are in document order, and
    /// [`ParsedTree::index`] gives the place of each in it.
    ///
    /// The `Xot` stops keeping the tree when its structure changes: a node of
    /// it is moved, detached or removed, or a node is added to it. It also
    /// stops keeping the trees parsed after it then, unless it is the
    /// first tree that it keeps. A change of a value (a text, a name, the
    /// value of an attribute or a namespace node that is there already)
    /// keeps the tree, and so does a text that joins a text of the tree. A
    /// tree that was dropped once is never kept again.
    ///
    /// This is `Some` only for the nodes of a kept tree, namespace and
    /// attribute nodes included, and never for a node removed since.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let doc = xot.parse(r#"<a x="1"><b/>text</a>"#).unwrap();
    /// let tree = xot.parsed_tree(doc).unwrap();
    /// assert_eq!(tree.root(), doc);
    ///
    /// // the place of each node is its place in document order
    /// for (i, node) in xot.all_descendants(doc).enumerate() {
    ///     assert_eq!(xot.parsed_tree(node), Some(tree));
    ///     assert_eq!(tree.index(node), Some(i));
    ///     assert_eq!(tree.node(i), Some(node));
    /// }
    /// assert_eq!(tree.node_count(), 5);
    ///
    /// // a node made through the API is in no parsed tree
    /// let name = xot.add_name("c");
    /// let c = xot.new_element(name);
    /// assert_eq!(xot.parsed_tree(c), None);
    ///
    /// // a change of the structure ends the keeping of the tree
    /// let a = xot.document_element(doc).unwrap();
    /// xot.append(a, c).unwrap();
    /// assert_eq!(xot.parsed_tree(doc), None);
    /// assert_eq!(xot.dropped_parsed_trees(), &[tree]);
    /// ```
    #[inline]
    pub fn parsed_tree(&self, node: Node) -> Option<ParsedTree> {
        self.arena.parsed_tree(node.get())
    }

    /// The trees that this `Xot` kept and keeps no more, in the order in
    /// which it stopped keeping them (see [`Xot::parsed_tree`]).
    ///
    /// The list only grows, by one entry for each tree the `Xot` stops
    /// keeping, and a tree is never in it twice.
    #[inline]
    pub fn dropped_parsed_trees(&self) -> &[ParsedTree] {
        self.arena.dropped_parsed_trees()
    }
}
