// The relations, the checks and the iterators of this module are adapted
// from indextree 4.7.2 (https://github.com/saschagrunert/indextree), which
// is under the MIT license:
//
// MIT License
//
// Copyright (c) 2018 Sascha Grunert
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The storage of the nodes of a [`Xot`](crate::Xot).
//!
//! Every node lives in a slot of an [`Arena`]. A slot holds the data of its
//! node, the positions of the parent, the previous and the next sibling and
//! the first and the last child of the node (each a `u32`, 0 for none), and a
//! generation: 0 for the first node in the slot, one more for each later node
//! that takes it. For the data of a [`Xot`](crate::Xot) a slot takes 64
//! bytes. A [`NodeId`] is the handle of a node: the position of its slot plus
//! one, and the generation of the node it was made for, 8 bytes in all.
//!
//! The slots are kept in chunks of 2^16. The first chunk grows like a `Vec`,
//! so that a small arena stays small; every later chunk is made with room for
//! all its slots when its first slot is needed, and only ever pushed into. So
//! the slots of a later chunk never move, and growth holds an old and a new
//! copy of at most the first chunk (4 MiB) at once.
//!
//! A removed node leaves its slot on a free list, which a later
//! [`Arena::new_node`] takes from, the slot freed first before the others.
//! The generation of a slot goes up by one each time a node takes it again; a
//! slot that reaches the generation `u32::MAX` is not used again.
//!
//! A slot keeps the positions of its parent, siblings and children, but not
//! their generations; the generation in a handle read from it comes from the
//! slot it refers to. A node only refers to nodes that are not removed, and
//! these keep their generation, so this is the generation of the node at the
//! time the reference was made. A removed node keeps the generations its
//! five fields referred to when it was removed, as these slots can be taken
//! by later nodes; a handle read from a removed node gets its generation from
//! there.
//!
//! The arena behaves as indextree 4.7.2 did, from which its operations are
//! adapted, and the other modules depend on this:
//!
//! - A handle finds its slot by position. Access to the value of a removed
//!   node through its handle panics (`Try to access a freed node`), but
//!   [`Arena::is_removed`] and the parent, child and sibling accessors work.
//!   Once a later node takes the slot, the old handle accesses, changes and
//!   removes that node.
//! - A handle is equal to another when both have the same position and
//!   generation. The handle of a removed node is never equal to the handle of
//!   a later node in its slot. (indextree's stamp stops changing after the
//!   32,767th reuse of a slot, so that from then on the handle of a removed
//!   node is equal to the handle of each later node in its slot.)
//! - A removed node's slot is taken by a later [`Arena::new_node`], the slot
//!   removed first before the others; [`Arena::remove_subtree`] removes a
//!   node and its descendants in document order.
//! - The operations check their arguments in the order of indextree, give
//!   its [`indextree::NodeError`] values, which the crate turns into
//!   [`Error::NodeError`](crate::Error::NodeError), and panic where it does,
//!   with the same message, but for one panic: [`Arena::remove`] panics when
//!   the children of the node do not agree with its parent and siblings,
//!   which takes a cycle made by `insert_after` or `insert_before`, and words
//!   this panic in its own way.
//!
//! The one exception is a change through the handle of a removed node whose
//! slot a later node took since: indextree kept that handle in the fields it
//! changed, so that it was read back from there, while here the handle of the
//! node that is in the slot is read back.
//!
//! The operations take indextree's steps one by one. Where the nodes
//! involved are not removed and agree with each other, these steps change
//! only a few fields, and the operations change those directly; see "Fast
//! paths" below.
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU32, NonZeroUsize};

/// The number of slots in a chunk is 2^`CHUNK_BITS`.
const CHUNK_BITS: u32 = 16;
/// The number of slots in a chunk: 4 MiB of 64-byte slots.
const CHUNK_LEN: usize = 1 << CHUNK_BITS;
/// The position of a slot in its chunk is its position `& CHUNK_MASK`.
const CHUNK_MASK: u32 = (1 << CHUNK_BITS) - 1;

// The fields of a slot that refer to other nodes, as indices into
// `Slot::link` and `FreeSlot::link_generations`.
const PARENT: usize = 0;
const PREVIOUS: usize = 1;
const NEXT: usize = 2;
const FIRST: usize = 3;
const LAST: usize = 4;

/// The handle of a node in an [`Arena`]: the position of its slot plus one,
/// and the generation of the node it was made for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct NodeId {
    index1: NonZeroU32,
    generation: u32,
}

impl Hash for NodeId {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64((u64::from(self.generation) << 32) | u64::from(self.index1.get()));
    }
}

impl NodeId {
    /// The handle with the position of `node_id` and the generation 0.
    ///
    /// Panics if the position does not fit in a `u32`.
    pub(crate) fn from_indextree(node_id: indextree::NodeId) -> Self {
        let index1: NonZeroUsize = node_id.into();
        let index1 = u32::try_from(index1.get())
            .ok()
            .and_then(NonZeroU32::new)
            .expect("indextree node id beyond u32");
        NodeId {
            index1,
            generation: 0,
        }
    }
}

/// An edge of a traversal: the start or the end of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum NodeEdge {
    /// The start of a node, before its descendants.
    Start(NodeId),
    /// The end of a node, after its descendants.
    End(NodeId),
}

/// The data that [`Arena::children_data`] gives for a child, as
/// [`Arena::get`] gives it: it panics for a removed child.
#[inline(always)]
pub(crate) fn live<T>(data: Option<&T>) -> &T {
    match data {
        Some(data) => data,
        None => unreachable!("Try to access a freed node"),
    }
}

/// A slot: a node, or a free slot that a later node can take.
#[derive(Clone, Debug)]
struct Slot<T> {
    /// The data of the node, or the free list entry of a removed node.
    data: SlotData<T>,
    /// The positions plus one of the parent, the previous and the next
    /// sibling, and the first and the last child (at `PARENT`, `PREVIOUS`,
    /// `NEXT`, `FIRST` and `LAST`), 0 where there is none.
    link: [u32; 5],
    /// The generation of the node in the slot: 0 for the first node, one
    /// more for each later node that takes the slot.
    generation: u32,
}

#[derive(Clone, Debug)]
enum SlotData<T> {
    /// A node and its data.
    Live(T),
    /// A removed node.
    Free(FreeSlot),
}

#[derive(Clone, Copy, Debug)]
struct FreeSlot {
    /// The position plus one of the next slot of the free list, 0 for none.
    next_free: u32,
    /// The generations of the nodes that the fields in `Slot::link` refer
    /// to, as they were when the node was removed, 0 where there is none.
    link_generations: [u32; 5],
}

impl<T> Slot<T> {
    #[inline(always)]
    fn is_free(&self) -> bool {
        matches!(self.data, SlotData::Free(_))
    }

    /// The node that entry `k` of the fields of this slot refers to: from a
    /// live slot with the generation of the slot it refers to, from a free
    /// slot with the generation kept for it.
    #[inline(always)]
    fn link_to(&self, k: usize) -> Link {
        Link {
            index1: self.link[k],
            generation: match &self.data {
                SlotData::Live(_) => None,
                SlotData::Free(free) => Some(free.link_generations[k]),
            },
        }
    }
}

/// A reference to a node inside an operation or an iterator: the position
/// of its slot plus one (0 for none), and its generation where it is known.
/// `None` stands for the generation of the node in that slot, which is read
/// only when it is needed. No slot changes its generation during an
/// operation, as only [`Arena::new_node`] changes it.
#[derive(Clone, Copy, Debug)]
struct Link {
    index1: u32,
    generation: Option<u32>,
}

impl Link {
    const NONE: Link = Link {
        index1: 0,
        generation: None,
    };

    #[inline(always)]
    fn known(node_id: NodeId) -> Link {
        Link {
            index1: node_id.index1.get(),
            generation: Some(node_id.generation),
        }
    }

    #[inline(always)]
    fn is_some(self) -> bool {
        self.index1 != 0
    }
}

/// An error in the structure that an operation was given, which the
/// operations turn into a panic (indextree's `ConsistencyError`).
#[derive(Debug, Clone, Copy)]
enum ConsistencyError {
    /// A node was given as its own parent.
    ParentChildLoop,
    /// A node was given as its own sibling.
    SiblingsLoop,
}

/// The node storage: nodes with data of type `T`, linked into trees.
#[derive(Debug)]
pub(crate) struct Arena<T> {
    /// The slots: the one at position `p` is
    /// `chunks[p >> CHUNK_BITS][p & CHUNK_MASK]`.
    chunks: Vec<Vec<Slot<T>>>,
    /// The number of slots.
    len: u32,
    /// The position plus one of the first slot of the free list, 0 when it
    /// is empty.
    first_free: u32,
    /// The position plus one of the last slot of the free list, 0 when it is
    /// empty.
    last_free: u32,
}

impl<T: Clone> Clone for Arena<T> {
    /// A copy in which every chunk has the room of the original, so that a
    /// chunk of the copy does not move as it fills up either.
    fn clone(&self) -> Self {
        let chunks = self
            .chunks
            .iter()
            .map(|chunk| {
                let mut copy = Vec::with_capacity(chunk.capacity());
                copy.extend_from_slice(chunk);
                copy
            })
            .collect();
        Arena {
            chunks,
            len: self.len,
            first_free: self.first_free,
            last_free: self.last_free,
        }
    }
}

impl<T> Arena<T> {
    /// An empty arena.
    pub(crate) fn new() -> Self {
        Arena {
            chunks: Vec::new(),
            len: 0,
            first_free: 0,
            last_free: 0,
        }
    }

    /// The slot at the position `index1 - 1`; `index1` is not 0.
    #[inline(always)]
    fn slot(&self, index1: u32) -> &Slot<T> {
        let position = index1 - 1;
        &self.chunks[(position >> CHUNK_BITS) as usize][(position & CHUNK_MASK) as usize]
    }

    /// The slot at the position `index1 - 1`, to change; `index1` is not 0.
    #[inline(always)]
    fn slot_mut(&mut self, index1: u32) -> &mut Slot<T> {
        let position = index1 - 1;
        &mut self.chunks[(position >> CHUNK_BITS) as usize][(position & CHUNK_MASK) as usize]
    }

    /// The generation of the node that `link` refers to; it refers to one.
    #[inline(always)]
    fn generation_of(&self, link: Link) -> u32 {
        match link.generation {
            Some(generation) => generation,
            None => self.slot(link.index1).generation,
        }
    }

    /// Whether two references stand for the same handle, or both for none, as
    /// indextree compares an `Option<NodeId>`.
    #[inline(always)]
    fn same(&self, a: Link, b: Link) -> bool {
        a.index1 == b.index1 && (a.index1 == 0 || self.generation_of(a) == self.generation_of(b))
    }

    /// The node that entry `k` of the fields of the slot of `node` refers to.
    #[inline(always)]
    fn link_of(&self, node: Link, k: usize) -> Link {
        self.slot(node.index1).link_to(k)
    }

    /// Make entry `k` of the fields of the slot of `node` refer to `target`. A
    /// free slot keeps the generation of `target` too.
    #[inline(always)]
    fn set_link(&mut self, node: Link, k: usize, target: Link) {
        let slot = self.slot_mut(node.index1);
        slot.link[k] = target.index1;
        if slot.is_free() {
            let generation = if target.is_some() {
                self.generation_of(target)
            } else {
                0
            };
            if let SlotData::Free(free) = &mut self.slot_mut(node.index1).data {
                free.link_generations[k] = generation;
            }
        }
    }

    /// Entry `k` of the fields of the slot of `node`, which is set to none.
    #[inline(always)]
    fn take_link(&mut self, node: Link, k: usize) -> Link {
        let slot = self.slot_mut(node.index1);
        let link = slot.link_to(k);
        slot.link[k] = 0;
        if let SlotData::Free(free) = &mut slot.data {
            free.link_generations[k] = 0;
        }
        link
    }

    #[inline(always)]
    fn is_detached(&self, node: Link) -> bool {
        let slot = self.slot(node.index1);
        slot.link[PARENT] == 0 && slot.link[PREVIOUS] == 0 && slot.link[NEXT] == 0
    }

    /// A new node with the data `data` and without parent, children or
    /// siblings, in the slot of the node removed first if there is one, else
    /// in a new slot.
    ///
    /// Panics with `Too many nodes in the arena` if the arena has `u32::MAX`
    /// slots already and none is free.
    pub(crate) fn new_node(&mut self, data: T) -> NodeId {
        if let Some(index1) = self.pop_front_free_node() {
            let slot = self.slot_mut(index1.get());
            debug_assert!(slot.is_free());
            // never overflows: a slot with the generation u32::MAX is not on
            // the free list
            slot.generation += 1;
            slot.link = [0; 5];
            slot.data = SlotData::Live(data);
            NodeId {
                index1,
                generation: slot.generation,
            }
        } else {
            let position = self.len;
            let index1 =
                NonZeroU32::new(position.wrapping_add(1)).expect("Too many nodes in the arena");
            let chunk = (position >> CHUNK_BITS) as usize;
            if chunk == self.chunks.len() {
                // the first chunk grows like a Vec; every later one is made
                // with room for all its slots, so that it never moves
                self.chunks.push(if chunk == 0 {
                    Vec::new()
                } else {
                    Vec::with_capacity(CHUNK_LEN)
                });
            }
            self.chunks[chunk].push(Slot {
                data: SlotData::Live(data),
                link: [0; 5],
                generation: 0,
            });
            self.len = index1.get();
            NodeId {
                index1,
                generation: 0,
            }
        }
    }

    /// Take the first slot of the free list off it.
    fn pop_front_free_node(&mut self) -> Option<NonZeroU32> {
        let first = NonZeroU32::new(self.first_free)?;
        let next_free = match &self.slot(first.get()).data {
            SlotData::Free(free) => free.next_free,
            SlotData::Live(_) => unreachable!("A data node consider as a freed node"),
        };
        self.first_free = next_free;
        if next_free == 0 {
            self.last_free = 0;
        }
        Some(first)
    }

    /// Mark the node in the slot at `index1 - 1` as removed, keeping the
    /// generations its fields refer to, and put the slot at the end of the
    /// free list, unless its generation is `u32::MAX`: then no later node
    /// takes the slot. The fields of the slot, which stay as they are.
    fn free_node(&mut self, index1: u32) -> [u32; 5] {
        let position = index1 - 1;
        let (c, o) = (
            (position >> CHUNK_BITS) as usize,
            (position & CHUNK_MASK) as usize,
        );
        let link = self.chunks[c][o].link;
        let mut link_generations = [0; 5];
        if link != [0; 5] {
            let chunk = &self.chunks[c];
            for (generation, &target) in link_generations.iter_mut().zip(&link) {
                if target != 0 {
                    // most nodes a node refers to are in its own chunk
                    let target = target - 1;
                    *generation = if target >> CHUNK_BITS == position >> CHUNK_BITS {
                        chunk[(target & CHUNK_MASK) as usize].generation
                    } else {
                        self.slot(target + 1).generation
                    };
                }
            }
        }
        let slot = &mut self.chunks[c][o];
        debug_assert!(!slot.is_free(), "a removed node is removed again");
        slot.data = SlotData::Free(FreeSlot {
            next_free: 0,
            link_generations,
        });
        if slot.generation == u32::MAX {
            return link;
        }
        if self.last_free != 0 {
            let last_free = self.last_free;
            if let SlotData::Free(free) = &mut self.slot_mut(last_free).data {
                free.next_free = index1;
            }
            self.last_free = index1;
        } else {
            debug_assert_eq!(self.first_free, 0);
            self.first_free = index1;
            self.last_free = index1;
        }
        link
    }

    /// The data of the node in the slot of `node_id`.
    ///
    /// Panics if that node is removed.
    #[inline]
    pub(crate) fn get(&self, node_id: NodeId) -> &T {
        match &self.slot(node_id.index1.get()).data {
            SlotData::Live(data) => data,
            SlotData::Free(_) => unreachable!("Try to access a freed node"),
        }
    }

    /// The data of the node in the slot of `node_id`, to change.
    ///
    /// Panics if that node is removed.
    #[inline]
    pub(crate) fn get_mut(&mut self, node_id: NodeId) -> &mut T {
        match &mut self.slot_mut(node_id.index1.get()).data {
            SlotData::Live(data) => data,
            SlotData::Free(_) => unreachable!("Try to access a freed node"),
        }
    }

    /// Whether the slot of `node_id` holds a removed node.
    ///
    /// This looks at the slot only: once a new node takes the slot of a
    /// removed node, this is false for the handle of the removed node too.
    #[inline]
    pub(crate) fn is_removed(&self, node_id: NodeId) -> bool {
        self.slot(node_id.index1.get()).is_free()
    }

    /// The node that entry `k` of the fields of the slot of `node_id` refers
    /// to. The
    /// generation comes from the slot of that node, read as [`Arena::get`]
    /// reads it, so that the compiler can share the two reads.
    #[inline(always)]
    fn read_link(&self, node_id: NodeId, k: usize) -> Option<NodeId> {
        let slot = self.slot(node_id.index1.get());
        let index1 = NonZeroU32::new(slot.link[k])?;
        let generation = match &slot.data {
            SlotData::Live(_) => self.slot(index1.get()).generation,
            SlotData::Free(free) => free.link_generations[k],
        };
        Some(NodeId { index1, generation })
    }

    /// The parent of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, PARENT)
    }

    /// The first child of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn first_child(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, FIRST)
    }

    /// The last child of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn last_child(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, LAST)
    }

    /// The next sibling of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn next_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, NEXT)
    }

    /// The previous sibling of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn previous_sibling(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, PREVIOUS)
    }

    // The relations of indextree 4.7.2 (relations.rs and siblings_range.rs),
    // step by step.

    /// Connect `previous` and `next` as siblings under `parent` (each may be
    /// none), and make them the first or the last child of `parent` where
    /// they are at an end.
    fn connect_neighbors(&mut self, parent: Link, previous: Link, next: Link) {
        if cfg!(debug_assertions) {
            if parent.is_some() {
                let parent_slot = self.slot(parent.index1);
                debug_assert_eq!(parent_slot.link[FIRST] != 0, parent_slot.link[LAST] != 0);
                debug_assert!(!parent_slot.is_free());
            }
            debug_assert!(!(previous.is_some() && self.slot(previous.index1).is_free()));
            debug_assert!(!(next.is_some() && self.slot(next.index1).is_free()));
        }

        let (mut parent_first_child, mut parent_last_child) = if parent.is_some() {
            let parent_slot = self.slot(parent.index1);
            (parent_slot.link_to(FIRST), parent_slot.link_to(LAST))
        } else {
            (Link::NONE, Link::NONE)
        };
        if previous.is_some() {
            // `previous` ==> `next`
            self.set_link(previous, NEXT, next);
            if !parent_first_child.is_some() {
                parent_first_child = previous;
            }
        } else {
            // `next` is the first child of the parent.
            parent_first_child = next;
        }
        if next.is_some() {
            // `previous` <== `next`
            self.set_link(next, PREVIOUS, previous);
            if !parent_last_child.is_some() {
                parent_last_child = next;
            }
        } else {
            // `previous` is the last child of the parent.
            parent_last_child = previous;
        }

        if parent.is_some() {
            debug_assert_eq!(parent_first_child.is_some(), parent_last_child.is_some());
            self.set_link(parent, FIRST, parent_first_child);
            self.set_link(parent, LAST, parent_last_child);
        }

        self.debug_assert_triangle_nodes(parent, previous, next);
    }

    /// With debug assertions, check that `previous` and `next` are siblings under
    /// `parent` (indextree's `debug_assert_triangle_nodes!`).
    #[inline(always)]
    fn debug_assert_triangle_nodes(&self, parent: Link, previous: Link, next: Link) {
        if cfg!(debug_assertions) {
            if previous.is_some() {
                assert!(
                    self.same(self.link_of(previous, PARENT), parent),
                    "`prev->parent` must equal to `parent`"
                );
                assert!(
                    self.same(self.link_of(previous, NEXT), next),
                    "`prev->next` must equal to `next`"
                );
            }
            if next.is_some() {
                assert!(
                    self.same(self.link_of(next, PARENT), parent),
                    "`next->parent` must equal to `parent`"
                );
                assert!(
                    self.same(self.link_of(next, PREVIOUS), previous),
                    "`next->prev` must equal to `prev`"
                );
            }
        }
    }

    /// Unlink the siblings from `first` to `last` from the siblings around
    /// them and from their parent; they keep their parent
    /// (`SiblingsRange::detach_from_siblings`).
    fn detach_from_siblings(&mut self, first: Link, last: Link) {
        let parent = self.link_of(first, PARENT);
        let prev_of_range = self.take_link(first, PREVIOUS);
        let next_of_range = self.take_link(last, NEXT);
        self.connect_neighbors(parent, prev_of_range, next_of_range);

        if cfg!(debug_assertions) {
            debug_assert!(!self.link_of(first, PREVIOUS).is_some());
            debug_assert!(!self.link_of(last, NEXT).is_some());
            self.debug_assert_triangle_nodes(parent, prev_of_range, next_of_range);
            if parent.is_some() {
                let first_child = self.link_of(parent, FIRST);
                let last_child = self.link_of(parent, LAST);
                debug_assert_eq!(first_child.is_some(), last_child.is_some());
                self.debug_assert_triangle_nodes(parent, Link::NONE, first_child);
                self.debug_assert_triangle_nodes(parent, last_child, Link::NONE);
            }
        }
    }

    /// Set the parent of `first` and of its next siblings to `new_parent`
    /// (`DetachedSiblingsRange::rewrite_parents`).
    fn rewrite_parents(&mut self, first: Link, new_parent: Link) -> Result<(), ConsistencyError> {
        let mut child = first;
        while child.is_some() {
            if self.same(child, new_parent) {
                // Attempt to set the node itself as its parent.
                return Err(ConsistencyError::ParentChildLoop);
            }
            self.set_link(child, PARENT, new_parent);
            child = self.link_of(child, NEXT);
        }
        Ok(())
    }

    /// Put the detached siblings from `first` to `last` under `parent`,
    /// between `previous_sibling` and `next_sibling`
    /// (`DetachedSiblingsRange::transplant`).
    fn transplant(
        &mut self,
        first: Link,
        last: Link,
        parent: Link,
        previous_sibling: Link,
        next_sibling: Link,
    ) -> Result<(), ConsistencyError> {
        if cfg!(debug_assertions) {
            if previous_sibling.is_some() {
                debug_assert!(self.same(self.link_of(previous_sibling, PARENT), parent));
            }
            if next_sibling.is_some() {
                debug_assert!(self.same(self.link_of(next_sibling, PARENT), parent));
            }
            self.debug_assert_triangle_nodes(parent, previous_sibling, next_sibling);
            if parent.is_some() {
                let parent_slot = self.slot(parent.index1);
                debug_assert_eq!(parent_slot.link[FIRST] != 0, parent_slot.link[LAST] != 0);
            }
        }

        self.rewrite_parents(first, parent)?;
        self.connect_neighbors(parent, previous_sibling, first);
        self.connect_neighbors(parent, last, next_sibling);

        if cfg!(debug_assertions) {
            self.debug_assert_triangle_nodes(parent, previous_sibling, first);
            self.debug_assert_triangle_nodes(parent, last, next_sibling);
            if parent.is_some() {
                let first_child = self.link_of(parent, FIRST);
                let last_child = self.link_of(parent, LAST);
                debug_assert!(
                    first_child.is_some() && last_child.is_some(),
                    "parent should have children (at least `self.first`)"
                );
                self.debug_assert_triangle_nodes(parent, Link::NONE, first_child);
                self.debug_assert_triangle_nodes(parent, last_child, Link::NONE);
            }
        }
        Ok(())
    }

    /// Detach `new` and put it under `parent`, between `previous_sibling`
    /// and `next_sibling` (`relations::insert_with_neighbors`).
    fn insert_with_neighbors(
        &mut self,
        new: Link,
        parent: Link,
        previous_sibling: Link,
        next_sibling: Link,
    ) -> Result<(), ConsistencyError> {
        self.debug_assert_triangle_nodes(parent, previous_sibling, next_sibling);
        if self.same(previous_sibling, new) || self.same(next_sibling, new) {
            // One of the given neighbors is going to be detached.
            return Err(ConsistencyError::SiblingsLoop);
        }
        if self.same(parent, new) {
            // The given parent is the node itself.
            return Err(ConsistencyError::ParentChildLoop);
        }

        self.detach_from_siblings(new, new);
        self.transplant(new, new, parent, previous_sibling, next_sibling)
            .expect("Should never fail: neighbors including parent are not `self`");

        self.debug_assert_triangle_nodes(parent, previous_sibling, new);
        self.debug_assert_triangle_nodes(parent, new, next_sibling);
        Ok(())
    }

    /// Whether `target` is `node` or one of its ancestors, each handle
    /// compared as indextree compares them (`node.ancestors().any()`).
    fn ancestors_contain(&self, node: NodeId, target: NodeId) -> bool {
        if node == target {
            return true;
        }
        let mut source = self.slot(node.index1.get());
        loop {
            let index1 = source.link[PARENT];
            if index1 == 0 {
                return false;
            }
            let slot = self.slot(index1);
            if index1 == target.index1.get() {
                let generation = match &source.data {
                    SlotData::Live(_) => slot.generation,
                    SlotData::Free(free) => free.link_generations[PARENT],
                };
                if generation == target.generation {
                    return true;
                }
            }
            source = slot;
        }
    }

    // Fast paths. The steps above change only a few fields in the simple
    // case: no node is given twice, the node to insert is not removed and is
    // detached, the parent is not removed and has both a first and a last
    // child, or neither and then the node gets no siblings, and the siblings
    // are children of the parent and not removed. The operations below check
    // for this case and then change these fields directly; in any other case
    // they take the steps above. The siblings come from the fields of the
    // parent or of a sibling, so they are children of the parent, and the
    // nodes that a node which is not removed refers to are not removed
    // either; only debug assertions check these two.

    /// Whether `insert_with_neighbors` puts the live, detached node at `new`
    /// under `parent`, between `previous` and `next` (positions plus one, 0
    /// for none), in the simple case.
    #[inline(always)]
    fn is_simple_insert(&self, new: u32, parent: u32, previous: u32, next: u32) -> bool {
        if new == parent || new == previous || new == next || (previous != 0 && previous == next) {
            return false;
        }
        if parent != 0 {
            let parent_slot = self.slot(parent);
            let (first, last) = (parent_slot.link[FIRST], parent_slot.link[LAST]);
            let agree = if first != 0 && last != 0 {
                true
            } else {
                first == 0 && last == 0 && previous == 0 && next == 0
            };
            if parent_slot.is_free() || !agree {
                return false;
            }
        }
        // the siblings of a node that is not removed are not removed either;
        // `connect_neighbors` checks this with debug assertions
        debug_assert!(!(previous != 0 && self.slot(previous).is_free()));
        debug_assert!(!(next != 0 && self.slot(next).is_free()));
        true
    }

    /// What `insert_with_neighbors` does in the simple case.
    #[inline(always)]
    fn insert_simple(&mut self, new: u32, parent: u32, previous: u32, next: u32) {
        let slot = self.slot_mut(new);
        slot.link[PARENT] = parent;
        slot.link[PREVIOUS] = previous;
        slot.link[NEXT] = next;
        if previous != 0 {
            self.slot_mut(previous).link[NEXT] = new;
        } else if parent != 0 {
            self.slot_mut(parent).link[FIRST] = new;
        }
        if next != 0 {
            self.slot_mut(next).link[PREVIOUS] = new;
        } else if parent != 0 {
            self.slot_mut(parent).link[LAST] = new;
        }
    }

    /// Whether `detach` takes the live node at `node` from `parent`,
    /// `previous` and `next` in the simple case.
    #[inline(always)]
    fn is_simple_detach(&self, node: u32, parent: u32, previous: u32, next: u32) -> bool {
        if node == parent || node == previous || node == next || (previous != 0 && previous == next)
        {
            return false;
        }
        if parent != 0 {
            let parent_slot = self.slot(parent);
            if parent_slot.is_free() || parent_slot.link[FIRST] == 0 || parent_slot.link[LAST] == 0
            {
                return false;
            }
        }
        debug_assert!(!(previous != 0 && self.slot(previous).is_free()));
        debug_assert!(!(next != 0 && self.slot(next).is_free()));
        true
    }

    /// What `detach` does in the simple case.
    #[inline(always)]
    fn detach_simple(&mut self, node: u32, parent: u32, previous: u32, next: u32) {
        if previous != 0 {
            self.slot_mut(previous).link[NEXT] = next;
        } else if parent != 0 {
            self.slot_mut(parent).link[FIRST] = next;
        }
        if next != 0 {
            self.slot_mut(next).link[PREVIOUS] = previous;
        } else if parent != 0 {
            self.slot_mut(parent).link[LAST] = previous;
        }
        let slot = self.slot_mut(node);
        slot.link[PARENT] = 0;
        slot.link[PREVIOUS] = 0;
        slot.link[NEXT] = 0;
    }

    /// Unlink a node from its parent and its siblings. It keeps its
    /// descendants.
    pub(crate) fn detach(&mut self, node_id: NodeId) {
        let n = node_id.index1.get();
        let slot = self.slot(n);
        let (free, parent, previous, next) = (
            slot.is_free(),
            slot.link[PARENT],
            slot.link[PREVIOUS],
            slot.link[NEXT],
        );
        // a node without parent and siblings is detached already, and the
        // steps below would change nothing
        if parent == 0 && previous == 0 && next == 0 {
            return;
        }
        if !free && self.is_simple_detach(n, parent, previous, next) {
            self.detach_simple(n, parent, previous, next);
            return;
        }
        let node = Link::known(node_id);
        self.detach_from_siblings(node, node);
        self.rewrite_parents(node, Link::NONE)
            .expect("Should never happen: `None` as parent is always valid");
        debug_assert!(self.is_detached(node), "The node should be detached");
    }

    /// Some fields of the slot at `index1`: whether it is free, its parent,
    /// previous and next sibling, and its first child.
    #[inline(always)]
    fn fields(&self, index1: u32) -> (bool, u32, u32, u32, u32) {
        let slot = self.slot(index1);
        (
            slot.is_free(),
            slot.link[PARENT],
            slot.link[PREVIOUS],
            slot.link[NEXT],
            slot.link[FIRST],
        )
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
        if new_child == parent {
            return Err(indextree::NodeError::AppendSelf);
        }
        let (p, c) = (parent.index1.get(), new_child.index1.get());
        let parent_slot = self.slot(p);
        let (parent_free, last) = (parent_slot.is_free(), parent_slot.link[LAST]);
        let (child_free, child_parent, child_previous, child_next, child_first) = self.fields(c);
        if parent_free || child_free {
            return Err(indextree::NodeError::Removed);
        }
        // a node without children is no ancestor of `parent`
        if child_first != 0 && self.ancestors_contain(parent, new_child) {
            return Err(indextree::NodeError::AppendAncestor);
        }
        if child_parent == 0
            && child_previous == 0
            && child_next == 0
            && self.is_simple_insert(c, p, last, 0)
        {
            self.insert_simple(c, p, last, 0);
            return Ok(());
        }
        self.detach(new_child);
        let parent = Link::known(parent);
        let last_child = self.link_of(parent, LAST);
        self.insert_with_neighbors(Link::known(new_child), parent, last_child, Link::NONE)
            .expect("Should never fail: `new_child` is not `self` and they are not removed");
        Ok(())
    }

    /// Append `new_child`, a node that [`Arena::new_node`] just made, to the
    /// children of `parent`, a node that is not removed, without the checks
    /// of [`Arena::checked_append`]: for such a node it does what that does.
    ///
    /// The two are different nodes, neither is removed, and `new_child` has
    /// no parent, siblings or children, so it is not an ancestor of `parent`
    /// and is detached already; `parent` has both a first and a last child or
    /// neither. Debug assertions check this, and only they do.
    #[inline]
    pub(crate) fn append_fresh(&mut self, parent: NodeId, new_child: NodeId) {
        let (p, c) = (parent.index1.get(), new_child.index1.get());
        if cfg!(debug_assertions) {
            assert_ne!(p, c, "a node is appended to itself");
            let parent_slot = self.slot(p);
            assert!(
                !parent_slot.is_free() && parent_slot.generation == parent.generation,
                "the parent is removed"
            );
            assert_eq!(
                parent_slot.link[FIRST] != 0,
                parent_slot.link[LAST] != 0,
                "the parent has a first child or a last child, not both"
            );
            let child_slot = self.slot(c);
            assert!(
                !child_slot.is_free() && child_slot.generation == new_child.generation,
                "the new child is removed"
            );
            assert_eq!(child_slot.link, [0; 5], "the new child is not a new node");
        }
        let last = self.slot(p).link[LAST];
        self.insert_simple(c, p, last, 0);
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
        if new_child == parent {
            return Err(indextree::NodeError::PrependSelf);
        }
        let (p, c) = (parent.index1.get(), new_child.index1.get());
        let parent_slot = self.slot(p);
        let (parent_free, first) = (parent_slot.is_free(), parent_slot.link[FIRST]);
        let (child_free, child_parent, child_previous, child_next, child_first) = self.fields(c);
        if parent_free || child_free {
            return Err(indextree::NodeError::Removed);
        }
        // a node without children is no ancestor of `parent`
        if child_first != 0 && self.ancestors_contain(parent, new_child) {
            return Err(indextree::NodeError::PrependAncestor);
        }
        if child_parent == 0
            && child_previous == 0
            && child_next == 0
            && self.is_simple_insert(c, p, 0, first)
        {
            self.insert_simple(c, p, 0, first);
            return Ok(());
        }
        let parent = Link::known(parent);
        let first_child = self.link_of(parent, FIRST);
        self.insert_with_neighbors(Link::known(new_child), parent, Link::NONE, first_child)
            .expect("Should never fail: `new_child` is not `self` and they are not removed");
        Ok(())
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
        if new_sibling == sibling {
            return Err(indextree::NodeError::InsertAfterSelf);
        }
        let (s, n) = (sibling.index1.get(), new_sibling.index1.get());
        let (sibling_free, parent, _, next, _) = self.fields(s);
        let (new_free, new_parent, new_previous, new_next, _) = self.fields(n);
        if sibling_free || new_free {
            return Err(indextree::NodeError::Removed);
        }
        if new_parent == 0
            && new_previous == 0
            && new_next == 0
            && self.is_simple_insert(n, parent, s, next)
        {
            self.insert_simple(n, parent, s, next);
            return Ok(());
        }
        self.detach(new_sibling);
        let sibling = Link::known(sibling);
        let (next_sibling, parent) = {
            let slot = self.slot(sibling.index1);
            (slot.link_to(NEXT), slot.link_to(PARENT))
        };
        self.insert_with_neighbors(Link::known(new_sibling), parent, sibling, next_sibling)
            .expect("Should never fail: `new_sibling` is not `self` and they are not removed");
        Ok(())
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
        if new_sibling == sibling {
            return Err(indextree::NodeError::InsertBeforeSelf);
        }
        let (s, n) = (sibling.index1.get(), new_sibling.index1.get());
        let (sibling_free, parent, previous, _, _) = self.fields(s);
        let (new_free, new_parent, new_previous, new_next, _) = self.fields(n);
        if sibling_free || new_free {
            return Err(indextree::NodeError::Removed);
        }
        if new_parent == 0
            && new_previous == 0
            && new_next == 0
            && self.is_simple_insert(n, parent, previous, s)
        {
            self.insert_simple(n, parent, previous, s);
            return Ok(());
        }
        self.detach(new_sibling);
        let sibling = Link::known(sibling);
        let (previous_sibling, parent) = {
            let slot = self.slot(sibling.index1);
            (slot.link_to(PREVIOUS), slot.link_to(PARENT))
        };
        self.insert_with_neighbors(Link::known(new_sibling), parent, previous_sibling, sibling)
            .expect("Should never fail: `new_sibling` is not `self` and they are not removed");
        Ok(())
    }

    /// Remove a node: its children take its place among its siblings, and it
    /// is left without parent, children or siblings.
    pub(crate) fn remove(&mut self, node_id: NodeId) {
        let node = Link::known(node_id);
        let (parent, previous_sibling, next_sibling, first_child, last_child) = {
            let slot = self.slot(node.index1);
            (
                slot.link_to(PARENT),
                slot.link_to(PREVIOUS),
                slot.link_to(NEXT),
                slot.link_to(FIRST),
                slot.link_to(LAST),
            )
        };
        self.debug_assert_triangle_nodes(parent, previous_sibling, node);
        self.debug_assert_triangle_nodes(parent, node, next_sibling);
        self.debug_assert_triangle_nodes(node, Link::NONE, first_child);
        self.debug_assert_triangle_nodes(node, last_child, Link::NONE);

        assert_eq!(first_child.is_some(), last_child.is_some());
        self.detach(node_id);
        if first_child.is_some() && last_child.is_some() {
            self.detach_from_siblings(first_child, last_child);
            self.transplant(
                first_child,
                last_child,
                parent,
                previous_sibling,
                next_sibling,
            )
            .expect("Should never fail: neighbors and children must agree");
        }
        self.free_node(node.index1);
        debug_assert!(self.is_detached(node));
    }

    /// Remove a node and its descendants: the node is detached, then it and
    /// its descendants are removed in document order. The removed
    /// descendants keep their parent, children and siblings, and the node
    /// its first and last child.
    pub(crate) fn remove_subtree(&mut self, node_id: NodeId) {
        self.detach(node_id);
        // a preorder traversal, by position: the first child, else the next
        // sibling, else the next sibling of the nearest ancestor that has one
        let mut cursor = node_id.index1.get();
        loop {
            let link = self.free_node(cursor);
            cursor = if link[FIRST] != 0 {
                link[FIRST]
            } else if link[NEXT] != 0 {
                link[NEXT]
            } else {
                let mut ancestor = link[PARENT];
                let mut next = 0;
                while ancestor != 0 {
                    let ancestor_slot = self.slot(ancestor);
                    if ancestor_slot.link[NEXT] != 0 {
                        next = ancestor_slot.link[NEXT];
                        break;
                    }
                    ancestor = ancestor_slot.link[PARENT];
                }
                next
            };
            if cursor == 0 {
                break;
            }
        }
    }

    /// The children of a node, first to last.
    pub(crate) fn children(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let slot = self.slot(node_id.index1.get());
        Siblings::<'_, T, NEXT>::new(&self.chunks, slot.link_to(FIRST), slot.link_to(LAST))
    }

    /// The children of a node with their data, first to last, `None` for a
    /// removed child; [`live`] gives the data as [`Arena::get`] does. Each
    /// step reads one slot.
    pub(crate) fn children_data(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = (NodeId, Option<&T>)> + '_ {
        let slot = self.slot(node_id.index1.get());
        ChildrenData {
            siblings: Siblings::<'_, T, NEXT>::new(
                &self.chunks,
                slot.link_to(FIRST),
                slot.link_to(LAST),
            ),
        }
    }

    /// The node and its descendants, in document order.
    pub(crate) fn descendants(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        Descendants {
            chunks: &self.chunks,
            root: node_id,
            next: Link::known(node_id),
        }
    }

    /// The edges of the node and its descendants, in document order: the
    /// start and the end of each node, with those of its descendants between
    /// them.
    pub(crate) fn traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        Traverse {
            chunks: &self.chunks,
            root: node_id,
            next: Some(Edge::Start(Link::known(node_id))),
        }
    }

    /// The edges of the node and its descendants, in reverse document order.
    pub(crate) fn reverse_traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        ReverseTraverse {
            chunks: &self.chunks,
            root: node_id,
            next: Some(Edge::End(Link::known(node_id))),
        }
    }

    /// The node and its ancestors, from the node up.
    pub(crate) fn ancestors(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        Ancestors {
            cursor: Cursor::new(&self.chunks),
            next: Link::known(node_id),
        }
    }

    /// The node and the siblings that follow it.
    pub(crate) fn following_siblings(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let parent = self.slot(node_id.index1.get()).link[PARENT];
        let tail = if parent != 0 {
            self.slot(parent).link_to(LAST)
        } else {
            Link::NONE
        };
        Siblings::<'_, T, NEXT>::new(&self.chunks, Link::known(node_id), tail)
    }

    /// The node and the siblings that precede it, from the node back.
    pub(crate) fn preceding_siblings(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let parent = self.slot(node_id.index1.get()).link[PARENT];
        let tail = if parent != 0 {
            self.slot(parent).link_to(FIRST)
        } else {
            Link::NONE
        };
        Siblings::<'_, T, PREVIOUS>::new(&self.chunks, Link::known(node_id), tail)
    }
}

/// The slot at the position `index1 - 1` among `chunks`, read as
/// [`Arena::get`] reads it, so that the compiler can share the two reads of
/// a node by an iterator and by its caller; `index1` is not 0.
#[inline(always)]
fn slot_in<T>(chunks: &[Vec<Slot<T>>], index1: u32) -> &Slot<T> {
    let position = index1 - 1;
    &chunks[(position >> CHUNK_BITS) as usize][(position & CHUNK_MASK) as usize]
}

/// The reader of slots of the iterators that walk along siblings or up. It holds the chunks themselves
/// rather than the arena, so that the compiler sees that the steps change
/// none of them, and it keeps the chunk of the slot it read last: the nodes
/// an iterator goes through are mostly in one chunk, and a slot there is
/// read without the chunk table.
#[derive(Clone)]
struct Cursor<'a, T> {
    chunks: &'a [Vec<Slot<T>>],
    chunk: &'a [Slot<T>],
    /// The index of `chunk` in `chunks`, `u32::MAX` before the first read.
    chunk_index: u32,
}

impl<'a, T> Cursor<'a, T> {
    #[inline(always)]
    fn new(chunks: &'a [Vec<Slot<T>>]) -> Self {
        Cursor {
            chunks,
            chunk: &[],
            chunk_index: u32::MAX,
        }
    }

    /// The slot at the position `index1 - 1`; `index1` is not 0.
    #[inline(always)]
    fn slot(&mut self, index1: u32) -> &'a Slot<T> {
        let position = index1 - 1;
        let chunk_index = position >> CHUNK_BITS;
        if chunk_index != self.chunk_index {
            self.chunk = &self.chunks[chunk_index as usize];
            self.chunk_index = chunk_index;
        }
        &self.chunk[(position & CHUNK_MASK) as usize]
    }
}

/// The nodes from `head` on through entry `STEP` of the fields (the next or
/// the previous sibling) up to `tail`, or to the last one if the steps do not
/// come to `tail`
/// (indextree's `Children`, `FollowingSiblings` and `PrecedingSiblings`
/// going forward). Each step reads one slot.
#[derive(Clone)]
struct Siblings<'a, T, const STEP: usize> {
    cursor: Cursor<'a, T>,
    head: Link,
    tail: Link,
}

impl<'a, T, const STEP: usize> Siblings<'a, T, STEP> {
    #[inline(always)]
    fn new(chunks: &'a [Vec<Slot<T>>], head: Link, tail: Link) -> Self {
        Siblings {
            cursor: Cursor::new(chunks),
            head,
            tail,
        }
    }

    /// The next node and its slot.
    #[inline(always)]
    fn step(&mut self) -> Option<(NodeId, &'a Slot<T>)> {
        let index1 = NonZeroU32::new(self.head.index1)?;
        let slot = self.cursor.slot(index1.get());
        let head = NodeId {
            index1,
            generation: self.head.generation.unwrap_or(slot.generation),
        };
        // the tail is the same handle when it has the same position and
        // generation; at the same position its slot is this one
        if self.tail.index1 == index1.get()
            && self.tail.generation.unwrap_or(slot.generation) == head.generation
        {
            self.head = Link::NONE;
            self.tail = Link::NONE;
        } else {
            self.head = slot.link_to(STEP);
        }
        Some((head, slot))
    }
}

impl<T, const STEP: usize> Iterator for Siblings<'_, T, STEP> {
    type Item = NodeId;

    #[inline(always)]
    fn next(&mut self) -> Option<NodeId> {
        self.step().map(|(node_id, _)| node_id)
    }
}

impl<T, const STEP: usize> std::iter::FusedIterator for Siblings<'_, T, STEP> {}

/// The children of a node with their data, `None` for a removed child: the
/// data comes from the slot that the step reads anyway.
#[derive(Clone)]
struct ChildrenData<'a, T> {
    siblings: Siblings<'a, T, NEXT>,
}

impl<'a, T> Iterator for ChildrenData<'a, T> {
    type Item = (NodeId, Option<&'a T>);

    #[inline(always)]
    fn next(&mut self) -> Option<(NodeId, Option<&'a T>)> {
        let (node_id, slot) = self.siblings.step()?;
        Some((
            node_id,
            match &slot.data {
                SlotData::Live(data) => Some(data),
                SlotData::Free(_) => None,
            },
        ))
    }
}

impl<T> std::iter::FusedIterator for ChildrenData<'_, T> {}

/// The node and its ancestors (indextree's `Ancestors`). Each step reads one
/// slot.
#[derive(Clone)]
struct Ancestors<'a, T> {
    cursor: Cursor<'a, T>,
    next: Link,
}

impl<T> Iterator for Ancestors<'_, T> {
    type Item = NodeId;

    #[inline(always)]
    fn next(&mut self) -> Option<NodeId> {
        let index1 = NonZeroU32::new(self.next.index1)?;
        let slot = self.cursor.slot(index1.get());
        let node_id = NodeId {
            index1,
            generation: self.next.generation.unwrap_or(slot.generation),
        };
        self.next = slot.link_to(PARENT);
        Some(node_id)
    }
}

impl<T> std::iter::FusedIterator for Ancestors<'_, T> {}

/// The node and its descendants (indextree's `Descendants`: the nodes of
/// the start edges of [`Traverse`]). Each node takes one step, which reads
/// its slot, and the slots of the ancestors it climbs to where it has no
/// first child: the end edges in between.
#[derive(Clone)]
struct Descendants<'a, T> {
    chunks: &'a [Vec<Slot<T>>],
    root: NodeId,
    next: Link,
}

impl<T> Iterator for Descendants<'_, T> {
    type Item = NodeId;

    #[inline(always)]
    fn next(&mut self) -> Option<NodeId> {
        let index1 = NonZeroU32::new(self.next.index1)?;
        let slot = slot_in(self.chunks, index1.get());
        let node_id = NodeId {
            index1,
            generation: self.next.generation.unwrap_or(slot.generation),
        };
        self.next = if slot.link[FIRST] != 0 {
            slot.link_to(FIRST)
        } else {
            // the end of this node, then of each ancestor up to one with a
            // next sibling, as `Traverse` goes, stopping at the root
            let mut current = node_id;
            let mut current_slot = slot;
            loop {
                if current == self.root {
                    break Link::NONE;
                }
                if current_slot.link[NEXT] != 0 {
                    break current_slot.link_to(NEXT);
                }
                let parent = current_slot.link_to(PARENT);
                let Some(parent_index1) = NonZeroU32::new(parent.index1) else {
                    break Link::NONE;
                };
                current_slot = slot_in(self.chunks, parent_index1.get());
                current = NodeId {
                    index1: parent_index1,
                    generation: parent.generation.unwrap_or(current_slot.generation),
                };
            }
        };
        Some(node_id)
    }
}

impl<T> std::iter::FusedIterator for Descendants<'_, T> {}

/// The next edge of a traversal, with the node as a reference.
#[derive(Clone, Copy)]
enum Edge {
    Start(Link),
    End(Link),
}

/// The edges of a node and its descendants (indextree's `Traverse`). Each
/// step reads one slot.
#[derive(Clone)]
struct Traverse<'a, T> {
    chunks: &'a [Vec<Slot<T>>],
    root: NodeId,
    next: Option<Edge>,
}

impl<T> Iterator for Traverse<'_, T> {
    type Item = NodeEdge;

    #[inline(always)]
    fn next(&mut self) -> Option<NodeEdge> {
        match self.next.take()? {
            Edge::Start(link) => {
                let index1 = NonZeroU32::new(link.index1)?;
                let slot = slot_in(self.chunks, index1.get());
                let node_id = NodeId {
                    index1,
                    generation: link.generation.unwrap_or(slot.generation),
                };
                self.next = Some(if slot.link[FIRST] != 0 {
                    Edge::Start(slot.link_to(FIRST))
                } else {
                    Edge::End(Link::known(node_id))
                });
                Some(NodeEdge::Start(node_id))
            }
            Edge::End(link) => {
                let index1 = NonZeroU32::new(link.index1)?;
                let slot = slot_in(self.chunks, index1.get());
                let node_id = NodeId {
                    index1,
                    generation: link.generation.unwrap_or(slot.generation),
                };
                self.next = if node_id == self.root {
                    None
                } else if slot.link[NEXT] != 0 {
                    Some(Edge::Start(slot.link_to(NEXT)))
                } else if slot.link[PARENT] != 0 {
                    Some(Edge::End(slot.link_to(PARENT)))
                } else {
                    // only if the tree was changed during the traversal;
                    // indextree stops then
                    None
                };
                Some(NodeEdge::End(node_id))
            }
        }
    }
}

impl<T> std::iter::FusedIterator for Traverse<'_, T> {}

/// The edges of a node and its descendants in reverse document order
/// (indextree's `ReverseTraverse`). Each step reads one slot.
#[derive(Clone)]
struct ReverseTraverse<'a, T> {
    chunks: &'a [Vec<Slot<T>>],
    root: NodeId,
    next: Option<Edge>,
}

impl<T> Iterator for ReverseTraverse<'_, T> {
    type Item = NodeEdge;

    #[inline(always)]
    fn next(&mut self) -> Option<NodeEdge> {
        match self.next.take()? {
            Edge::End(link) => {
                let index1 = NonZeroU32::new(link.index1)?;
                let slot = slot_in(self.chunks, index1.get());
                let node_id = NodeId {
                    index1,
                    generation: link.generation.unwrap_or(slot.generation),
                };
                self.next = Some(if slot.link[LAST] != 0 {
                    Edge::End(slot.link_to(LAST))
                } else {
                    Edge::Start(Link::known(node_id))
                });
                Some(NodeEdge::End(node_id))
            }
            Edge::Start(link) => {
                let index1 = NonZeroU32::new(link.index1)?;
                let slot = slot_in(self.chunks, index1.get());
                let node_id = NodeId {
                    index1,
                    generation: link.generation.unwrap_or(slot.generation),
                };
                self.next = if node_id == self.root {
                    None
                } else if slot.link[PREVIOUS] != 0 {
                    Some(Edge::End(slot.link_to(PREVIOUS)))
                } else if slot.link[PARENT] != 0 {
                    Some(Edge::Start(slot.link_to(PARENT)))
                } else {
                    None
                };
                Some(NodeEdge::Start(node_id))
            }
        }
    }
}

impl<T> std::iter::FusedIterator for ReverseTraverse<'_, T> {}

#[cfg(test)]
mod tests;
