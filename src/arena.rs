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

/// The chunk of the slot with the position plus one `index1`, and the place
/// of the slot in the chunk; `index1` is not 0.
#[inline(always)]
fn place(index1: u32) -> (u32, usize) {
    let position = index1 - 1;
    (position >> CHUNK_BITS, (position & CHUNK_MASK) as usize)
}

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
#[cfg_attr(test, derive(PartialEq))]
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
#[cfg_attr(test, derive(PartialEq))]
enum SlotData<T> {
    /// A node and its data.
    Live(T),
    /// A removed node.
    Free(FreeSlot),
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(PartialEq))]
struct FreeSlot {
    /// The position plus one of the next slot of the free list, 0 for none.
    next_free: u32,
    /// The generations of the nodes that the fields in `Slot::link` refer
    /// to, as they were when the node was removed, for the fields whose bit
    /// is set in `kept`; 0 for the others.
    link_generations: [u32; 5],
    /// Bit `k` is set where entry `k` of `link_generations` holds the
    /// generation of the node that entry `k` of `Slot::link` refers to. Where
    /// it is not set, the generation is read from the slot of that node: the
    /// entry referred to a node that was not removed when this one was, which
    /// can only be removed later, so that its slot comes after this one on
    /// the free list and no later node takes it before one takes this slot.
    kept: u8,
}

/// A node that an operation knows, as the position of its slot plus one
/// (0 for none) and its generation, for [`Arena::free_node`].
#[derive(Clone, Copy)]
struct Known {
    index1: u32,
    generation: u32,
}

impl Known {
    const NONE: Known = Known {
        index1: 0,
        generation: 0,
    };
}

/// Some fields of a node that [`Arena::free_node`] removed, as positions
/// plus one (0 for none), and its generation.
#[derive(Clone, Copy)]
struct Freed {
    parent: u32,
    next: u32,
    first: u32,
    generation: u32,
}

impl<T> Slot<T> {
    #[inline(always)]
    fn is_free(&self) -> bool {
        matches!(self.data, SlotData::Free(_))
    }

    /// The data of the node, `None` for a removed node.
    #[inline(always)]
    fn data(&self) -> Option<&T> {
        match &self.data {
            SlotData::Live(data) => Some(data),
            SlotData::Free(_) => None,
        }
    }

    /// The node that entry `k` of the fields of this slot refers to: with the
    /// generation of the slot it refers to, or from a free slot with the
    /// generation kept for it, if one is kept.
    #[inline(always)]
    fn link_to(&self, k: usize) -> Link {
        Link {
            index1: self.link[k],
            generation: self.kept_generation(k),
        }
    }

    /// The generation kept for entry `k` of the fields of a free slot, if
    /// one is kept; otherwise the generation is that of the slot that the
    /// entry refers to.
    #[inline(always)]
    fn kept_generation(&self, k: usize) -> Option<u32> {
        match &self.data {
            SlotData::Free(free) if free.kept & (1 << k) != 0 => Some(free.link_generations[k]),
            _ => None,
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
        let (chunk, place) = place(index1);
        &self.chunks[chunk as usize][place]
    }

    /// The slot at the position `index1 - 1`, to change; `index1` is not 0.
    #[inline(always)]
    fn slot_mut(&mut self, index1: u32) -> &mut Slot<T> {
        let (chunk, place) = place(index1);
        &mut self.chunks[chunk as usize][place]
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
                if target.is_some() {
                    free.kept |= 1 << k;
                } else {
                    free.kept &= !(1 << k);
                }
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
            free.kept &= !(1 << k);
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
        if let Some(index1) = NonZeroU32::new(self.first_free) {
            // the first slot of the free list, which this takes off it
            let slot = self.slot_mut(index1.get());
            let next_free = match &slot.data {
                SlotData::Free(free) => free.next_free,
                SlotData::Live(_) => unreachable!("A data node consider as a freed node"),
            };
            // never overflows: a slot with the generation u32::MAX is not on
            // the free list
            slot.generation += 1;
            slot.link = [0; 5];
            slot.data = SlotData::Live(data);
            let generation = slot.generation;
            self.first_free = next_free;
            if next_free == 0 {
                self.last_free = 0;
            }
            NodeId { index1, generation }
        } else {
            let index1 =
                NonZeroU32::new(self.len.wrapping_add(1)).expect("Too many nodes in the arena");
            let chunk = place(index1.get()).0 as usize;
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

    /// Mark the node in the slot at `index1 - 1` as removed, and put the slot
    /// at the end of the free list, unless its generation is `u32::MAX`: then
    /// no later node takes the slot. The fields of the slot stay as they are;
    /// some of them are returned with the generation of the node.
    ///
    /// The node keeps the generations of the nodes that its fields refer to
    /// where a later node could take their slots before one takes this slot
    /// (see [`FreeSlot::kept`]): for the parent and the previous sibling,
    /// which the caller may have removed before, and for all of them when
    /// the slot is not put on the free list. `parent` and `previous` give the
    /// nodes that the caller knows for these two, so that their slots need
    /// not be looked at; where the parent or the previous sibling is another
    /// node, the generation comes from its slot. The nodes that the other
    /// fields refer to are not removed yet.
    #[inline(always)]
    fn free_node(&mut self, index1: u32, parent: Known, previous: Known) -> Freed {
        let slot = self.slot_mut(index1);
        debug_assert!(!slot.is_free(), "a removed node is removed again");
        let (link, generation) = (slot.link, slot.generation);
        let known = |k: usize, node: Known| link[k] == 0 || link[k] == node.index1;
        if generation == u32::MAX || !known(PARENT, parent) || !known(PREVIOUS, previous) {
            return self.free_node_in_full(index1, parent, previous);
        }
        // what `free_slot` gives when the caller knows both nodes
        let mut free = FreeSlot {
            next_free: 0,
            link_generations: [0; 5],
            kept: 0,
        };
        for (k, known) in [(PARENT, parent), (PREVIOUS, previous)] {
            if link[k] != 0 {
                free.link_generations[k] = known.generation;
                free.kept |= 1 << k;
            }
        }
        slot.data = SlotData::Free(free);
        self.push_free(index1);
        Freed {
            parent: link[PARENT],
            next: link[NEXT],
            first: link[FIRST],
            generation,
        }
    }

    /// Put the free slot at `index1` at the end of the free list.
    #[inline(always)]
    fn push_free(&mut self, index1: u32) {
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
    }

    /// The free list entry of a node with the fields `link`, with the
    /// generations of its parent and its previous sibling, as
    /// [`Arena::free_node`] keeps them.
    #[inline(always)]
    fn free_slot(&self, link: [u32; 5], parent: Known, previous: Known) -> FreeSlot {
        let mut free = FreeSlot {
            next_free: 0,
            link_generations: [0; 5],
            kept: 0,
        };
        for (k, known) in [(PARENT, parent), (PREVIOUS, previous)] {
            let target = link[k];
            if target != 0 {
                free.link_generations[k] = if target == known.index1 {
                    known.generation
                } else {
                    self.slot(target).generation
                };
                free.kept |= 1 << k;
            }
        }
        free
    }

    /// [`Arena::free_node`] where it looks up generations in the slots that
    /// the fields refer to: for a parent or a previous sibling that the
    /// caller does not give, and for a slot with the generation `u32::MAX`,
    /// which is not put on the free list, so that its node keeps the
    /// generations of all the nodes its fields refer to.
    #[cold]
    #[inline(never)]
    fn free_node_in_full(&mut self, index1: u32, parent: Known, previous: Known) -> Freed {
        let slot = self.slot(index1);
        let (link, generation) = (slot.link, slot.generation);
        let mut free = self.free_slot(link, parent, previous);
        let last = generation == u32::MAX;
        if last {
            for k in [NEXT, FIRST, LAST] {
                let target = link[k];
                if target != 0 {
                    free.link_generations[k] = self.slot(target).generation;
                    free.kept |= 1 << k;
                }
            }
        }
        self.slot_mut(index1).data = SlotData::Free(free);
        if !last {
            self.push_free(index1);
        }
        Freed {
            parent: link[PARENT],
            next: link[NEXT],
            first: link[FIRST],
            generation,
        }
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

    /// The data of the nodes in the slots of `a` and of `b`, to change both,
    /// as [`Arena::get_mut`] gives each; `None` if the two have one slot.
    ///
    /// Panics if either node is removed.
    pub(crate) fn get_pair_mut(&mut self, a: NodeId, b: NodeId) -> Option<(&mut T, &mut T)> {
        let (a, b) = (a.index1.get(), b.index1.get());
        if a == b {
            return None;
        }
        // the chunk of each and the position in it
        let ((chunk_a, in_a), (chunk_b, in_b)) = (place(a), place(b));
        let (chunk_a, chunk_b) = (chunk_a as usize, chunk_b as usize);
        // split the slots, or the chunks, between the two
        let (slot_a, slot_b) = if chunk_a == chunk_b {
            let chunk = &mut self.chunks[chunk_a];
            if in_a < in_b {
                let (low, high) = chunk.split_at_mut(in_b);
                (&mut low[in_a], &mut high[0])
            } else {
                let (low, high) = chunk.split_at_mut(in_a);
                (&mut high[0], &mut low[in_b])
            }
        } else if chunk_a < chunk_b {
            let (low, high) = self.chunks.split_at_mut(chunk_b);
            (&mut low[chunk_a][in_a], &mut high[0][in_b])
        } else {
            let (low, high) = self.chunks.split_at_mut(chunk_a);
            (&mut high[0][in_a], &mut low[chunk_b][in_b])
        };
        match (&mut slot_a.data, &mut slot_b.data) {
            (SlotData::Live(data_a), SlotData::Live(data_b)) => Some((data_a, data_b)),
            _ => unreachable!("Try to access a freed node"),
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
    /// to. The generation comes from the slot of that node, read as
    /// [`Arena::get`] reads it, so that the compiler can share the two reads,
    /// unless the slot of `node_id` is free and kept it.
    #[inline(always)]
    fn read_link(&self, node_id: NodeId, k: usize) -> Option<NodeId> {
        let source = node_id.index1.get();
        let (chunk, place) = place(source);
        let chunk = &self.chunks[chunk as usize];
        let slot = &chunk[place];
        let index1 = NonZeroU32::new(slot.link[k])?;
        let generation = match slot.kept_generation(k) {
            Some(generation) => generation,
            None => target_slot(self, chunk, source, index1.get()).generation,
        };
        Some(NodeId { index1, generation })
    }

    /// The data of the node in the slot of `node_id`, and the node that entry
    /// `k` of its fields refers to, with its data: [`Arena::get`] of the
    /// node and of [`Arena::read_link`] in one go, which panics as that does
    /// if either is removed.
    #[inline(always)]
    fn get_with_link(&self, node_id: NodeId, k: usize) -> (&T, Option<(NodeId, &T)>) {
        let source = node_id.index1.get();
        let (chunk, place) = place(source);
        let chunk = &self.chunks[chunk as usize];
        let slot = &chunk[place];
        let data = live(slot.data());
        let Some(index1) = NonZeroU32::new(slot.link[k]) else {
            return (data, None);
        };
        // a slot that is not free keeps no generations
        let target = target_slot(self, chunk, source, index1.get());
        let node_id = NodeId {
            index1,
            generation: target.generation,
        };
        (data, Some((node_id, live(target.data()))))
    }

    /// The data of the node in the slot of `node_id`, and its next sibling
    /// with its data, as [`Arena::get`] and [`Arena::next_sibling`] give
    /// them, which panics if either is removed.
    #[inline]
    pub(crate) fn get_with_next_sibling(&self, node_id: NodeId) -> (&T, Option<(NodeId, &T)>) {
        self.get_with_link(node_id, NEXT)
    }

    /// The data of the node in the slot of `node_id`, and its previous
    /// sibling with its data, as [`Arena::get`] and
    /// [`Arena::previous_sibling`] give them, which panics if either is
    /// removed.
    #[inline]
    pub(crate) fn get_with_previous_sibling(&self, node_id: NodeId) -> (&T, Option<(NodeId, &T)>) {
        self.get_with_link(node_id, PREVIOUS)
    }

    /// The parent of the node in the slot of `node_id`.
    #[inline]
    pub(crate) fn parent(&self, node_id: NodeId) -> Option<NodeId> {
        self.read_link(node_id, PARENT)
    }

    /// The first child of the node in the slot of `node_id`. The crate reads
    /// first children through [`Arena::children_data`] and the traversals;
    /// the tests read them here.
    #[cfg(test)]
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
                let generation = source.kept_generation(PARENT).unwrap_or(slot.generation);
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
        // the node has no parent and no siblings now
        self.free_node(node.index1, Known::NONE, Known::NONE);
        debug_assert!(self.is_detached(node));
    }

    /// Remove a node and its descendants: the node is detached, then it and
    /// its descendants are removed in document order. The removed
    /// descendants keep their parent, children and siblings, and the node
    /// its first and last child.
    pub(crate) fn remove_subtree(&mut self, node_id: NodeId) {
        self.detach(node_id);
        // a preorder traversal, by position: the first child, else the next
        // sibling, else the next sibling of the nearest ancestor that has
        // one. It knows the parent and the previous sibling of the node it
        // comes to, which it removed before, and their generations.
        let mut cursor = node_id.index1.get();
        let (mut parent, mut previous) = (Known::NONE, Known::NONE);
        loop {
            let freed = self.free_node(cursor, parent, previous);
            let this = Known {
                index1: cursor,
                generation: freed.generation,
            };
            cursor = if freed.first != 0 {
                (parent, previous) = (this, Known::NONE);
                freed.first
            } else if freed.next != 0 {
                previous = this;
                freed.next
            } else {
                let mut ancestor = freed.parent;
                let mut next = 0;
                while ancestor != 0 {
                    let ancestor_slot = self.slot(ancestor);
                    if ancestor_slot.link[NEXT] != 0 {
                        next = ancestor_slot.link[NEXT];
                        previous = Known {
                            index1: ancestor,
                            generation: ancestor_slot.generation,
                        };
                        // the ancestor kept the generation of its parent
                        parent = match ancestor_slot.kept_generation(PARENT) {
                            Some(generation) => Known {
                                index1: ancestor_slot.link[PARENT],
                                generation,
                            },
                            None => Known::NONE,
                        };
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

    /// The children of `node_id`, first to last, each with its data, given to
    /// `visit` until it returns `Some`, which is then the result; `None` if
    /// it returns `None` for each child.
    ///
    /// The slot of `node_id` holds a node that is not removed, which a debug
    /// assertion checks. Its children are then the ones that
    /// [`Arena::children_data`] gives, up to its last child, and none of them
    /// is removed: a removed one would panic as [`Arena::get`] does. A step
    /// reads one slot.
    ///
    /// Children made one after another are mostly in slots one after
    /// another. Where the next sibling of a child is in the slot after the
    /// slot of the child, in the same chunk, the step takes that slot as the
    /// next one in the chunk, and the next sibling in the fields of the child
    /// only confirms it: the read of a slot does not wait for the fields of
    /// the slot before.
    #[inline(always)]
    pub(crate) fn find_child<R>(
        &self,
        node_id: NodeId,
        mut visit: impl FnMut(NodeId, &T) -> Option<R>,
    ) -> Option<R> {
        // the children are mostly in the chunk of the parent
        let mut cursor = Cursor::new(&self.chunks);
        let parent = cursor.slot(node_id.index1.get());
        debug_assert!(!parent.is_free(), "the parent is removed");
        let (mut next, last) = (parent.link[FIRST], parent.link[LAST]);
        while let Some(mut index1) = NonZeroU32::new(next) {
            let (chunk, place) = cursor.chunk(index1.get());
            let mut slot = &chunk[place];
            // the slots after it in its chunk, in order
            let mut after = chunk[place + 1..].iter();
            loop {
                let SlotData::Live(data) = &slot.data else {
                    unreachable!("Try to access a freed node")
                };
                let child = NodeId {
                    index1,
                    generation: slot.generation,
                };
                if let Some(result) = visit(child, data) {
                    return Some(result);
                }
                // the last child, where `children_data` stops
                if index1.get() == last {
                    return None;
                }
                next = slot.link[NEXT];
                match (after.next(), index1.checked_add(1)) {
                    // the next sibling is in the next slot of the chunk
                    (Some(next_slot), Some(next_index1)) if next_index1.get() == next => {
                        slot = next_slot;
                        index1 = next_index1;
                    }
                    _ => break,
                }
            }
        }
        None
    }

    /// The node and its descendants, in document order.
    #[inline]
    pub(crate) fn descendants(&self, node_id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.descendants_data(node_id).map(|(node_id, _)| node_id)
    }

    /// The node and its descendants, in document order, each with its data,
    /// `None` for a removed node; [`live`] gives the data as [`Arena::get`]
    /// does. The data comes from the slot that the step reads anyway.
    #[inline]
    pub(crate) fn descendants_data(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = (NodeId, Option<&T>)> + '_ {
        Descendants {
            chunks: &self.chunks,
            root: node_id,
            next: Link::known(node_id),
        }
    }

    /// The edges of the node and its descendants, in document order: the
    /// start and the end of each node, with those of its descendants between
    /// them.
    #[inline]
    pub(crate) fn traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        self.traverse_data(node_id).map(|(edge, _)| edge)
    }

    /// The edges of [`Arena::traverse`], each with the data of its node, as
    /// [`Arena::descendants_data`] gives it.
    #[inline]
    pub(crate) fn traverse_data(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = (NodeEdge, Option<&T>)> + '_ {
        Traverse {
            chunks: &self.chunks,
            root: node_id,
            next: Some(Edge::Start(Link::known(node_id))),
            leaf: None,
        }
    }

    /// The traversal of [`Arena::traverse`], before its first edge, for
    /// [`Arena::traverse_step`].
    pub(crate) fn traverse_start(&self, node_id: NodeId) -> TraverseState {
        TraverseState {
            root: node_id,
            next: Some(Edge::Start(Link::known(node_id))),
        }
    }

    /// The next edge of a traversal and the data of its node, `None` for a
    /// removed node ([`live`] gives the data as [`Arena::get`] does), read
    /// from one slot.
    ///
    /// The state holds no borrow of the arena, so that the caller can change
    /// the arena between two steps. A step reads the fields of the node it
    /// comes to then, so the edges are those of [`Arena::traverse`] as long
    /// as the changes leave alone the nodes the traversal has still to visit,
    /// the fields that lead to them and the generations of the slots it
    /// refers to. [`Arena::new_node`] takes a free slot, and the nodes of a
    /// tree refer to no free slot, so new nodes change none of these.
    #[inline]
    pub(crate) fn traverse_step(
        &self,
        state: &mut TraverseState,
    ) -> Option<(NodeEdge, Option<&T>)> {
        let (edge, slot) = traverse_next(&self.chunks, state.root, &mut state.next)?;
        Some((edge, slot.data()))
    }

    /// The edges of the node and its descendants, in reverse document order.
    #[inline]
    pub(crate) fn reverse_traverse(&self, node_id: NodeId) -> impl Iterator<Item = NodeEdge> + '_ {
        self.reverse_traverse_data(node_id).map(|(edge, _)| edge)
    }

    /// The edges of [`Arena::reverse_traverse`], each with the data of its
    /// node, as [`Arena::descendants_data`] gives it.
    #[inline]
    pub(crate) fn reverse_traverse_data(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = (NodeEdge, Option<&T>)> + '_ {
        ReverseTraverse {
            chunks: &self.chunks,
            root: node_id,
            next: Some(Edge::End(Link::known(node_id))),
        }
    }

    /// The nodes after `node_id` in document order that are not its
    /// descendants, each with its data, as [`Arena::descendants_data`] gives
    /// it: the next sibling of the node, or of its nearest ancestor that has
    /// one, then each node after it in document order, up to the end of the
    /// tree. Each step reads one slot, and the slots of the ancestors it
    /// climbs to where a node has no first child and no next sibling.
    #[inline]
    pub(crate) fn following(
        &self,
        node_id: NodeId,
    ) -> impl Iterator<Item = (NodeId, Option<&T>)> + '_ {
        let slot = self.slot(node_id.index1.get());
        Following {
            chunks: &self.chunks,
            next: following_link(&self.chunks, slot),
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

#[cfg(test)]
impl<T: PartialEq + std::fmt::Debug> Arena<T> {
    /// The first difference between the slots of the two arenas (the data,
    /// the fields and the generation of each) and their free lists, if any.
    pub(crate) fn slot_difference(&self, other: &Self) -> Option<String> {
        let counts = |arena: &Self| (arena.len, arena.first_free, arena.last_free);
        if counts(self) != counts(other) {
            return Some(format!(
                "slots and free list {:?} against {:?}",
                counts(self),
                counts(other)
            ));
        }
        self.chunks
            .iter()
            .flatten()
            .zip(other.chunks.iter().flatten())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(position, (a, b))| format!("slot {position}: {a:?} against {b:?}"))
    }
}

/// The slot at the position `index1 - 1` among `chunks`, read as
/// [`Arena::get`] reads it, so that the compiler can share the two reads of
/// a node by an iterator and by its caller; `index1` is not 0.
#[inline(always)]
fn slot_in<T>(chunks: &[Vec<Slot<T>>], index1: u32) -> &Slot<T> {
    let (chunk, place) = place(index1);
    &chunks[chunk as usize][place]
}

/// The slot with the position plus one `target`, which the slot with the
/// position plus one `source`, in `chunk`, refers to: the nodes a node
/// refers to are mostly in its own chunk, which is then not looked up again.
#[inline(always)]
fn target_slot<'a, T>(
    arena: &'a Arena<T>,
    chunk: &'a [Slot<T>],
    source: u32,
    target: u32,
) -> &'a Slot<T> {
    let (target_chunk, target_place) = place(target);
    if target_chunk == place(source).0 {
        &chunk[target_place]
    } else {
        arena.slot(target)
    }
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
        let (chunk_index, place) = place(index1);
        if chunk_index != self.chunk_index {
            self.chunk = &self.chunks[chunk_index as usize];
            self.chunk_index = chunk_index;
        }
        &self.chunk[place]
    }

    /// The chunk of the slot at the position `index1 - 1`, and the position
    /// of the slot in it; `index1` is not 0.
    #[inline(always)]
    fn chunk(&mut self, index1: u32) -> (&'a [Slot<T>], usize) {
        let (chunk_index, place) = place(index1);
        if chunk_index != self.chunk_index {
            self.chunk = &self.chunks[chunk_index as usize];
            self.chunk_index = chunk_index;
        }
        (self.chunk, place)
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

impl<'a, T> Iterator for Descendants<'a, T> {
    type Item = (NodeId, Option<&'a T>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
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
        Some((node_id, slot.data()))
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
    /// The slot of a node without children whose start was the last edge:
    /// its end, which `next` stands for, is the next edge, and takes the
    /// slot from here.
    leaf: Option<&'a Slot<T>>,
}

impl<'a, T> Iterator for Traverse<'a, T> {
    type Item = (NodeEdge, Option<&'a T>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(slot) = self.leaf.take() {
            // the end that `next` stands for, as `traverse_next` takes it
            if let Some(Edge::End(link)) = self.next.take() {
                if let Some(index1) = NonZeroU32::new(link.index1) {
                    let node_id = NodeId {
                        index1,
                        generation: link.generation.unwrap_or(slot.generation),
                    };
                    self.next = after_end(slot, node_id, self.root);
                    return Some((NodeEdge::End(node_id), slot.data()));
                }
            }
            unreachable!("the end of a node without children follows its start");
        }
        let (edge, slot) = traverse_next(self.chunks, self.root, &mut self.next)?;
        if matches!(edge, NodeEdge::Start(_)) && slot.link[FIRST] == 0 {
            self.leaf = Some(slot);
        }
        Some((edge, slot.data()))
    }
}

/// A traversal that [`Arena::traverse_step`] takes one edge at a time: the
/// edges of [`Traverse`], without a borrow of the arena.
#[derive(Clone, Copy)]
pub(crate) struct TraverseState {
    root: NodeId,
    next: Option<Edge>,
}

/// The edge of a traversal of `root` that `next` stands for, and the slot of
/// its node, which it reads; `next` becomes the edge after it, or `None` at
/// the end: a step of [`Traverse`].
#[inline(always)]
fn traverse_next<'a, T>(
    chunks: &'a [Vec<Slot<T>>],
    root: NodeId,
    next: &mut Option<Edge>,
) -> Option<(NodeEdge, &'a Slot<T>)> {
    match next.take()? {
        Edge::Start(link) => {
            let index1 = NonZeroU32::new(link.index1)?;
            let slot = slot_in(chunks, index1.get());
            let node_id = NodeId {
                index1,
                generation: link.generation.unwrap_or(slot.generation),
            };
            *next = Some(if slot.link[FIRST] != 0 {
                Edge::Start(slot.link_to(FIRST))
            } else {
                Edge::End(Link::known(node_id))
            });
            Some((NodeEdge::Start(node_id), slot))
        }
        Edge::End(link) => {
            let index1 = NonZeroU32::new(link.index1)?;
            let slot = slot_in(chunks, index1.get());
            let node_id = NodeId {
                index1,
                generation: link.generation.unwrap_or(slot.generation),
            };
            *next = after_end(slot, node_id, root);
            Some((NodeEdge::End(node_id), slot))
        }
    }
}

/// The edge after the end of `node_id`, whose slot is `slot`, in a
/// traversal of `root`, or `None` at the end.
#[inline(always)]
fn after_end<T>(slot: &Slot<T>, node_id: NodeId, root: NodeId) -> Option<Edge> {
    if node_id == root {
        None
    } else if slot.link[NEXT] != 0 {
        Some(Edge::Start(slot.link_to(NEXT)))
    } else if slot.link[PARENT] != 0 {
        Some(Edge::End(slot.link_to(PARENT)))
    } else {
        // only if the tree was changed during the traversal; indextree
        // stops then
        None
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

impl<'a, T> Iterator for ReverseTraverse<'a, T> {
    type Item = (NodeEdge, Option<&'a T>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
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
                Some((NodeEdge::End(node_id), slot.data()))
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
                Some((NodeEdge::Start(node_id), slot.data()))
            }
        }
    }
}

impl<T> std::iter::FusedIterator for ReverseTraverse<'_, T> {}

/// The node after the node in `slot` in document order that is not one of
/// its descendants: its next sibling, or that of its nearest ancestor that
/// has one, or none.
#[inline(always)]
fn following_link<T>(chunks: &[Vec<Slot<T>>], slot: &Slot<T>) -> Link {
    if slot.link[NEXT] != 0 {
        return slot.link_to(NEXT);
    }
    let mut ancestor = slot.link[PARENT];
    while ancestor != 0 {
        let ancestor_slot = slot_in(chunks, ancestor);
        if ancestor_slot.link[NEXT] != 0 {
            return ancestor_slot.link_to(NEXT);
        }
        ancestor = ancestor_slot.link[PARENT];
    }
    Link::NONE
}

/// The nodes that [`Arena::following`] gives.
#[derive(Clone)]
struct Following<'a, T> {
    chunks: &'a [Vec<Slot<T>>],
    next: Link,
}

impl<'a, T> Iterator for Following<'a, T> {
    type Item = (NodeId, Option<&'a T>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let index1 = NonZeroU32::new(self.next.index1)?;
        let slot = slot_in(self.chunks, index1.get());
        let node_id = NodeId {
            index1,
            generation: self.next.generation.unwrap_or(slot.generation),
        };
        self.next = if slot.link[FIRST] != 0 {
            slot.link_to(FIRST)
        } else {
            following_link(self.chunks, slot)
        };
        Some((node_id, slot.data()))
    }
}

impl<T> std::iter::FusedIterator for Following<'_, T> {}

#[cfg(test)]
mod tests;
