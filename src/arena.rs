//! The storage of the nodes of a [`Xot`](crate::Xot).
//!
//! Every node lives in a slot of an [`Arena`], connected to its parent, its
//! first and last child, and its previous and next sibling. A [`NodeId`] is
//! the handle of a node: the position of its slot and the stamp of the node it
//! was made for. All of the crate reaches the storage through this module
//! only, so that the storage can change without touching any other module.
//!
//! The arena keeps its nodes in an indextree 4.7.2 arena. It behaves as
//! indextree does, and the other modules depend on this:
//!
//! - A handle finds its slot by position. Access to the value of a removed
//!   node through its handle panics (`Try to access a freed node`), but
//!   [`Arena::is_removed`] and the parent, child and sibling accessors work.
//!   Once a later node takes the slot, the old handle accesses, changes and
//!   removes that node.
//! - A handle is equal to another when both have the same position and
//!   stamp, so the handle of a removed node is not equal to the handle of a
//!   node that takes its slot later, as long as the slot has been taken
//!   fewer than 32,768 times (indextree's stamp stops changing there).
//! - A removed node's slot is taken by a later [`Arena::new_node`], the slot
//!   removed first before the others.
//! - The operations check their arguments in the order of indextree and give
//!   its [`indextree::NodeError`] values; the crate turns them into
//!   [`Error::NodeError`](crate::Error::NodeError).
//!
//! For now the handle, the edge and the iterator types are indextree's own,
//! and the accessors are `#[inline(always)]`, so that the code the compiler
//! makes for the loops over the tree is the same as when the crate called
//! indextree directly. The other modules name these types through this
//! module only, and access and change the tree only through the methods of
//! [`Arena`].
use std::fmt;

/// The handle of a node in an [`Arena`]: the position of its slot and the
/// stamp of the node it was made for.
pub(crate) use indextree::NodeId;

/// An edge of a traversal: the start or the end of a node.
pub(crate) use indextree::NodeEdge;

/// The node storage: nodes with data of type `T`, linked into trees.
#[derive(Clone)]
pub(crate) struct Arena<T> {
    nodes: indextree::Arena<T>,
}

impl<T: fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.nodes, f)
    }
}

impl<T> Arena<T> {
    /// An empty arena.
    pub(crate) fn new() -> Self {
        Arena {
            nodes: indextree::Arena::new(),
        }
    }

    /// A new node with the data `data` and without parent, children or
    /// siblings, in the slot of the node removed first if there is one, else
    /// in a new slot.
    pub(crate) fn new_node(&mut self, data: T) -> NodeId {
        self.nodes.new_node(data)
    }

    /// The data of the node in the slot of `node_id`.
    ///
    /// Panics if that node is removed.
    #[inline(always)]
    pub(crate) fn get(&self, node_id: NodeId) -> &T {
        self.nodes[node_id].get()
    }

    /// The data of the node in the slot of `node_id`, to change.
    ///
    /// Panics if that node is removed.
    #[inline(always)]
    pub(crate) fn get_mut(&mut self, node_id: NodeId) -> &mut T {
        self.nodes[node_id].get_mut()
    }

    /// Whether the slot of `node_id` holds a removed node.
    ///
    /// This looks at the slot only: once a new node takes the slot of a
    /// removed node, this is false for the handle of the removed node too.
    #[inline(always)]
    pub(crate) fn is_removed(&self, node_id: NodeId) -> bool {
        self.nodes[node_id].is_removed()
    }

    /// The parent of the node in the slot of `node_id`.
    #[inline(always)]
    pub(crate) fn parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].parent()
    }

    /// The first child of the node in the slot of `node_id`.
    #[inline(always)]
    pub(crate) fn first_child(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].first_child()
    }

    /// The last child of the node in the slot of `node_id`.
    #[inline(always)]
    pub(crate) fn last_child(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].last_child()
    }

    /// The next sibling of the node in the slot of `node_id`.
    #[inline(always)]
    pub(crate) fn next_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].next_sibling()
    }

    /// The previous sibling of the node in the slot of `node_id`.
    #[inline(always)]
    pub(crate) fn previous_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes[node_id].previous_sibling()
    }

    /// Append `new_child` to the children of `parent`, detaching it first.
    ///
    /// Fails with `AppendSelf` if the two are the same node, then with
    /// `Removed` if either is removed, then with `AppendAncestor` if
    /// `new_child` is an ancestor of `parent`.
    pub(crate) fn checked_append(
        &mut self,
        parent: NodeId,
        new_child: NodeId,
    ) -> Result<(), indextree::NodeError> {
        parent.checked_append(new_child, &mut self.nodes)
    }

    /// Prepend `new_child` to the children of `parent`.
    ///
    /// Fails with `PrependSelf` if the two are the same node, then with
    /// `Removed` if either is removed, then with `PrependAncestor` if
    /// `new_child` is an ancestor of `parent`. It does not detach
    /// `new_child` first, so it panics if `new_child` is the first child of
    /// `parent` already.
    pub(crate) fn checked_prepend(
        &mut self,
        parent: NodeId,
        new_child: NodeId,
    ) -> Result<(), indextree::NodeError> {
        parent.checked_prepend(new_child, &mut self.nodes)
    }

    /// Insert `new_sibling` after `sibling`, detaching it first.
    ///
    /// Fails with `InsertAfterSelf` if the two are the same node, then with
    /// `Removed` if either is removed. Ancestors are not checked.
    pub(crate) fn checked_insert_after(
        &mut self,
        sibling: NodeId,
        new_sibling: NodeId,
    ) -> Result<(), indextree::NodeError> {
        sibling.checked_insert_after(new_sibling, &mut self.nodes)
    }

    /// Insert `new_sibling` before `sibling`, detaching it first.
    ///
    /// Fails with `InsertBeforeSelf` if the two are the same node, then with
    /// `Removed` if either is removed. Ancestors are not checked.
    pub(crate) fn checked_insert_before(
        &mut self,
        sibling: NodeId,
        new_sibling: NodeId,
    ) -> Result<(), indextree::NodeError> {
        sibling.checked_insert_before(new_sibling, &mut self.nodes)
    }

    /// Unlink a node from its parent and its siblings. It keeps its
    /// descendants.
    pub(crate) fn detach(&mut self, node_id: NodeId) {
        node_id.detach(&mut self.nodes)
    }

    /// Remove a node: its children take its place among its siblings, and it
    /// is left without parent, children or siblings.
    pub(crate) fn remove(&mut self, node_id: NodeId) {
        node_id.remove(&mut self.nodes)
    }

    /// Remove a node and its descendants: the node is detached, then it and
    /// its descendants are removed in document order. The removed
    /// descendants keep their parent, children and siblings, and the node
    /// its first and last child.
    pub(crate) fn remove_subtree(&mut self, node_id: NodeId) {
        node_id.remove_subtree(&mut self.nodes)
    }

    /// The children of a node, first to last.
    ///
    /// This is not a double-ended iterator: indextree 4.7.2's `next_back`
    /// moves the front of the iterator instead of the back. Walk back from
    /// [`Arena::last_child`] through [`Arena::previous_sibling`] instead.
    pub(crate) fn children(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        node_id.children(&self.nodes)
    }

    /// The node and its descendants, in document order.
    pub(crate) fn descendants(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        node_id.descendants(&self.nodes)
    }

    /// The edges of the node and its descendants, in document order: the
    /// start and the end of each node, with those of its descendants between
    /// them.
    pub(crate) fn traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        node_id.traverse(&self.nodes)
    }

    /// The edges of the node and its descendants, in reverse document order.
    pub(crate) fn reverse_traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        node_id.reverse_traverse(&self.nodes)
    }

    /// The node and its ancestors, from the node up.
    pub(crate) fn ancestors(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        node_id.ancestors(&self.nodes)
    }

    /// The node and the siblings that follow it.
    pub(crate) fn following_siblings(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        node_id.following_siblings(&self.nodes)
    }

    /// The node and the siblings that precede it, from the node back.
    pub(crate) fn preceding_siblings(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        node_id.preceding_siblings(&self.nodes)
    }
}
