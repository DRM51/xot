//! Tests of the arena: its own rules (reuse, generations, chunks), and a
//! comparison with indextree 4.7.2, from which its operations are adapted.
//!
//! The comparison (`oracle_*`) runs the same random operations on an
//! `indextree::Arena<u32>` and on an `Arena<u32>` and requires, after every
//! step, the same result (value, error or panic message), the same slots
//! (count, removed or not, the five fields as positions, the data), the
//! same handle from each `new_node` (position and stamp, which equals the
//! generation while a slot is taken fewer than 32,768 times), the same
//! handle read through every handle, live, removed or removed with its slot
//! taken again, and the same iterators. The operations that check their arguments (append,
//! prepend and the inserts) get live and removed nodes; `detach`, `remove`
//! and `remove_subtree` check nothing and get live nodes only, as the crate
//! never gives them a removed node (it reads the node's value first, which
//! panics). No operation gets a handle whose slot was taken again since its
//! node was removed: indextree kept such a handle in the fields it changed,
//! which this arena does not do. A sequence ends after a panic and once its
//! nodes form a cycle, which `insert_after` and `insert_before` can make (they
//! do not check ancestors), as later walks up and along such a tree would not
//! end.
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe, RefUnwindSafe, UnwindSafe};

use super::*;
use crate::xmlvalue::Value;

// ---------------------------------------------------------------- helpers

/// Run `f` and catch a panic: `Err` holds its message.
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|cause| {
        cause
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    })
}

/// A seeded xorshift64* generator.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, in_hundred: u64) -> bool {
        self.next() % 100 < in_hundred
    }
}

/// The position plus one and the stamp of an indextree handle. The stamp
/// has no accessor, so it comes from the debug text: `NodeId { index1: 1,
/// stamp: NodeStamp(0) }`.
fn it_key(id: indextree::NodeId) -> (u32, u32) {
    let index1: usize = id.into();
    let text = format!("{id:?}");
    let stamp = text
        .rsplit("NodeStamp(")
        .next()
        .and_then(|after| after.split(')').next())
        .and_then(|stamp| stamp.parse::<i32>().ok())
        .expect("indextree 4.7.2 debug text");
    assert!(stamp >= 0, "a handle has the stamp of a live node");
    (index1 as u32, stamp as u32)
}

fn my_key(id: NodeId) -> (u32, u32) {
    (id.index1.get(), id.generation)
}

fn it_keys(ids: impl Iterator<Item = indextree::NodeId>) -> Vec<(u32, u32)> {
    ids.map(it_key).collect()
}

fn my_keys(ids: impl Iterator<Item = NodeId>) -> Vec<(u32, u32)> {
    ids.map(my_key).collect()
}

fn it_edges(edges: impl Iterator<Item = indextree::NodeEdge>) -> Vec<(bool, (u32, u32))> {
    edges
        .map(|edge| match edge {
            indextree::NodeEdge::Start(id) => (true, it_key(id)),
            indextree::NodeEdge::End(id) => (false, it_key(id)),
        })
        .collect()
}

fn my_edges(edges: impl Iterator<Item = NodeEdge>) -> Vec<(bool, (u32, u32))> {
    edges
        .map(|edge| match edge {
            NodeEdge::Start(id) => (true, my_key(id)),
            NodeEdge::End(id) => (false, my_key(id)),
        })
        .collect()
}

/// The five fields of the slot at `position` (from 0) as positions plus one.
fn my_fields(arena: &Arena<u32>, position: usize) -> [u32; 5] {
    arena.slot(position as u32 + 1).link
}

fn it_fields(node: &indextree::Node<u32>) -> [u32; 5] {
    let index = |id: Option<indextree::NodeId>| {
        id.map_or(0, |id| {
            let index1: usize = id.into();
            index1 as u32
        })
    };
    [
        index(node.parent()),
        index(node.previous_sibling()),
        index(node.next_sibling()),
        index(node.first_child()),
        index(node.last_child()),
    ]
}

/// The five fields read through a handle, each as a handle.
fn it_read(arena: &indextree::Arena<u32>, id: indextree::NodeId) -> [Option<(u32, u32)>; 5] {
    let node = &arena[id];
    [
        node.parent().map(it_key),
        node.previous_sibling().map(it_key),
        node.next_sibling().map(it_key),
        node.first_child().map(it_key),
        node.last_child().map(it_key),
    ]
}

fn my_read(arena: &Arena<u32>, id: NodeId) -> [Option<(u32, u32)>; 5] {
    [
        arena.parent(id).map(my_key),
        arena.previous_sibling(id).map(my_key),
        arena.next_sibling(id).map(my_key),
        arena.first_child(id).map(my_key),
        arena.last_child(id).map(my_key),
    ]
}

// ---------------------------------------------------------------- the rules

/// A tree of `u32` nodes: `parent` gets children with the given values.
fn tree(arena: &mut Arena<u32>, parent: NodeId, values: &[u32]) -> Vec<NodeId> {
    values
        .iter()
        .map(|&value| {
            let child = arena.new_node(value);
            arena.checked_append(parent, child).unwrap();
            child
        })
        .collect()
}

#[test]
fn removed_slots_are_taken_first_in_first_out_with_the_next_generation() {
    let mut arena = Arena::new();
    let a = arena.new_node(1);
    let b = arena.new_node(2);
    let c = arena.new_node(3);
    assert_eq!([my_key(a), my_key(b), my_key(c)], [(1, 0), (2, 0), (3, 0)]);
    arena.remove(c);
    arena.remove(a);
    arena.remove(b);
    let x = arena.new_node(10);
    let y = arena.new_node(20);
    let z = arena.new_node(30);
    let w = arena.new_node(40);
    // in the order of removal, each with the next generation, then new slots
    assert_eq!(
        [my_key(x), my_key(y), my_key(z), my_key(w)],
        [(3, 1), (1, 1), (2, 1), (4, 0)]
    );
    assert_ne!(x, c);
    assert_ne!(y, a);
    // the handle of a removed node reaches the node in its slot
    assert!(!arena.is_removed(c));
    assert_eq!(*arena.get(c), 10);
    arena.remove(y);
    let v = arena.new_node(50);
    assert_eq!(my_key(v), (1, 2));
    assert!(v != a && v != y);
}

#[test]
fn remove_subtree_removes_in_document_order() {
    let mut arena = Arena::new();
    let root = arena.new_node(0);
    let a = tree(&mut arena, root, &[1, 4]);
    let b = tree(&mut arena, a[0], &[2, 3]);
    let c = tree(&mut arena, a[1], &[5]);
    let kept = arena.new_node(99);
    arena.remove_subtree(root);
    let order = [root, a[0], b[0], b[1], a[1], c[0]];
    for node in order {
        assert!(arena.is_removed(node));
    }
    assert!(!arena.is_removed(kept));
    // the root keeps its first and last child, the others all their fields
    assert_eq!(arena.first_child(root), Some(a[0]));
    assert_eq!(arena.last_child(root), Some(a[1]));
    assert_eq!(arena.parent(root), None);
    assert_eq!(arena.parent(b[1]), Some(a[0]));
    assert_eq!(arena.next_sibling(a[0]), Some(a[1]));
    // later nodes take the slots in that order
    for (value, node) in order.iter().enumerate() {
        let new = arena.new_node(100 + value as u32);
        assert_eq!(new.index1, node.index1);
        assert_eq!(new.generation, 1);
    }
    assert_eq!(arena.new_node(200).index1.get(), 8);
}

#[test]
fn remove_puts_the_children_in_the_place_of_the_node() {
    let mut arena = Arena::new();
    let parent = arena.new_node(0);
    let children = tree(&mut arena, parent, &[1, 2, 3]);
    let grandchildren = tree(&mut arena, children[1], &[21, 22, 23]);
    arena.remove(children[1]);
    let expected = [
        children[0],
        grandchildren[0],
        grandchildren[1],
        grandchildren[2],
        children[2],
    ];
    assert_eq!(arena.children(parent).collect::<Vec<_>>(), expected);
    for &child in &grandchildren {
        assert_eq!(arena.parent(child), Some(parent));
    }
    assert_eq!(arena.previous_sibling(grandchildren[0]), Some(children[0]));
    assert_eq!(arena.next_sibling(grandchildren[2]), Some(children[2]));
    // the removed node is left without parent, siblings or children
    assert!(arena.is_removed(children[1]));
    assert_eq!(my_read(&arena, children[1]), [None; 5]);
    // a node without children leaves its siblings joined
    arena.remove(grandchildren[1]);
    assert_eq!(arena.next_sibling(grandchildren[0]), Some(grandchildren[2]));
}

#[test]
fn the_value_of_a_removed_node_panics() {
    let mut arena = Arena::new();
    let a = arena.new_node(1);
    arena.remove(a);
    let message = catch(|| *arena.get(a)).unwrap_err();
    assert_eq!(
        message,
        "internal error: entered unreachable code: Try to access a freed node"
    );
    assert_eq!(catch(|| *arena.get_mut(a)).unwrap_err(), message);
    // reads of its fields and of its state work
    assert!(arena.is_removed(a));
    assert_eq!(arena.parent(a), None);
    // the same panic as indextree 4.7.2
    let mut it = indextree::Arena::new();
    let b = it.new_node(1);
    b.remove(&mut it);
    assert_eq!(catch(|| *it[b].get()).unwrap_err(), message);
}

#[test]
fn removed_nodes_read_what_they_had_after_their_slots_are_taken() {
    // a subtree is removed, then later nodes take some of its slots: reads
    // through the removed nodes give the handle of each node they had, not
    // the handle of the node that took its slot, as indextree's do
    let mut arena = Arena::new();
    let mut it = indextree::Arena::new();
    let root = Pair {
        it: it.new_node(0),
        my: arena.new_node(0),
    };
    let mut pairs = vec![root];
    for parent in 0..4 {
        for value in 0..3 {
            let child = Pair {
                it: it.new_node(value),
                my: arena.new_node(value),
            };
            pairs[parent].it.append(child.it, &mut it);
            arena.checked_append(pairs[parent].my, child.my).unwrap();
            pairs.push(child);
        }
    }
    root.it.remove_subtree(&mut it);
    arena.remove_subtree(root.my);
    for value in 0..5 {
        let new = Pair {
            it: it.new_node(100 + value),
            my: arena.new_node(100 + value),
        };
        assert_eq!(it_key(new.it), my_key(new.my));
        if value > 0 {
            let previous = pairs[pairs.len() - 1];
            previous.it.append(new.it, &mut it);
            arena.checked_append(previous.my, new.my).unwrap();
        }
        pairs.push(new);
    }
    for pair in &pairs {
        let (a, b) = (pair.it, pair.my);
        assert_eq!(it_read(&it, a), my_read(&arena, b));
        assert_eq!(it_keys(a.children(&it)), my_keys(arena.children(b)));
        assert_eq!(it_keys(a.ancestors(&it)), my_keys(arena.ancestors(b)));
        assert_eq!(
            it_keys(a.descendants(&it).take(64)),
            my_keys(arena.descendants(b).take(64))
        );
        assert_eq!(
            it_edges(a.traverse(&it).take(64)),
            my_edges(arena.traverse(b).take(64))
        );
        assert_eq!(
            it_edges(a.reverse_traverse(&it).take(64)),
            my_edges(arena.reverse_traverse(b).take(64))
        );
    }
    // the slots were removed in document order: the root (1), its first
    // child (2) and that child's children (5, 6, 7) are taken again. The
    // second child of the root (3) is still removed, and reads its parent and
    // its previous sibling with the generations they had, although later
    // nodes took their slots
    let second = pairs[2].my;
    assert!(arena.is_removed(second));
    assert_eq!(arena.parent(second).map(my_key), Some((1, 0)));
    assert_eq!(arena.previous_sibling(second).map(my_key), Some((2, 0)));
    assert_eq!(arena.slot(1).generation, 1);
    assert_eq!(arena.slot(2).generation, 1);
    assert!(!arena.is_removed(arena.previous_sibling(second).unwrap()));
}

#[test]
fn a_removed_node_whose_slot_is_not_taken_again_keeps_every_generation() {
    // a slot with the last generation is not put on the free list, so that
    // later nodes can take the slots its fields refer to while it stays
    // removed; reads through it give the nodes it had
    let mut arena = Arena::new();
    let root = arena.new_node(0);
    let children = tree(&mut arena, root, &[1, 2, 3]);
    let grandchildren = tree(&mut arena, children[1], &[21, 22]);
    arena.slot_mut(children[1].index1.get()).generation = u32::MAX;
    let middle = NodeId {
        index1: children[1].index1,
        generation: u32::MAX,
    };
    let had = my_read(&arena, middle);
    assert_eq!(
        had,
        [
            Some(my_key(root)),
            Some(my_key(children[0])),
            Some(my_key(children[2])),
            Some(my_key(grandchildren[0])),
            Some(my_key(grandchildren[1])),
        ]
    );
    arena.remove_subtree(root);
    // the other five slots are taken again, the slot of `middle` is not
    for _ in 0..5 {
        let new = arena.new_node(7);
        assert_ne!(new.index1, middle.index1);
        assert_eq!(new.generation, 1);
    }
    assert!(arena.is_removed(middle));
    assert_eq!(my_read(&arena, middle), had);
}

#[test]
fn a_slot_that_reaches_the_last_generation_is_not_taken_again() {
    let mut arena = Arena::new();
    let a = arena.new_node(1);
    let _b = arena.new_node(2);
    arena.slot_mut(a.index1.get()).generation = u32::MAX - 1;
    let a = NodeId {
        index1: a.index1,
        generation: u32::MAX - 1,
    };
    arena.remove(a);
    // taken once more, with the last generation
    let c = arena.new_node(3);
    assert_eq!(my_key(c), (1, u32::MAX));
    assert_ne!(c, a);
    arena.remove(c);
    // retired: it is not on the free list, so a new slot is made
    assert_eq!((arena.first_free, arena.last_free), (0, 0));
    let d = arena.new_node(4);
    assert_eq!(my_key(d), (3, 0));
    assert!(arena.is_removed(c));
    // a retired slot does not hold up the slots freed after it
    arena.remove(d);
    let e = arena.new_node(5);
    assert_eq!(my_key(e), (3, 1));
}

#[test]
fn chunks_hold_two_to_the_sixteen_slots_and_never_move() {
    let mut arena = Arena::new();
    let mut it = indextree::Arena::new();
    let root = Pair {
        it: it.new_node(0),
        my: arena.new_node(0),
    };
    // the first chunk grows like a Vec
    assert_eq!(arena.chunks.len(), 1);
    assert!(arena.chunks[0].capacity() < CHUNK_LEN);
    let mut parents = vec![root];
    let mut pairs = vec![root];
    for value in 1..70_000u32 {
        let parent = parents[(value as usize * 7) % parents.len()];
        let node = Pair {
            it: it.new_node(value),
            my: arena.new_node(value),
        };
        parent.it.append(node.it, &mut it);
        arena.checked_append(parent.my, node.my).unwrap();
        if value % 3 == 0 {
            parents.push(node);
        }
        pairs.push(node);
    }
    assert_eq!(arena.chunks.len(), 2);
    assert_eq!(arena.chunks[0].len(), CHUNK_LEN);
    assert_eq!(arena.chunks[0].capacity(), CHUNK_LEN);
    assert_eq!(arena.chunks[1].len(), 70_000 - CHUNK_LEN);
    assert_eq!(arena.chunks[1].capacity(), CHUNK_LEN);
    // nodes on both sides of the chunk boundary, as indextree has them
    for pair in pairs.iter().skip(CHUNK_LEN - 100).take(200) {
        assert_eq!(it_read(&it, pair.it), my_read(&arena, pair.my));
    }
    assert_eq!(
        it_edges(root.it.traverse(&it)),
        my_edges(arena.traverse(root.my))
    );
    // and after a subtree that spans the boundary is removed and its slots
    // are taken again
    let pair = pairs[CHUNK_LEN - 3];
    let parent = (
        it[pair.it].parent().unwrap(),
        arena.parent(pair.my).unwrap(),
    );
    parent.0.remove_subtree(&mut it);
    arena.remove_subtree(parent.1);
    for _ in 0..2000 {
        let node = (it.new_node(7), arena.new_node(7));
        assert_eq!(it_key(node.0), my_key(node.1));
    }
    for (position, node) in it.as_slice().iter().enumerate() {
        assert_eq!(it_fields(node), my_fields(&arena, position));
        assert_eq!(node.is_removed(), arena.slot(position as u32 + 1).is_free());
    }
    // a later chunk is made with room for all its slots and does not move
    let start = arena.chunks[1].as_ptr();
    while arena.len as usize != 2 * CHUNK_LEN {
        arena.new_node(0);
    }
    assert_eq!(arena.chunks[1].len(), CHUNK_LEN);
    assert_eq!(arena.chunks[1].as_ptr(), start);
    arena.new_node(0);
    assert_eq!(arena.chunks.len(), 3);
    assert_eq!(arena.chunks[1].as_ptr(), start);
    assert_eq!(arena.chunks[2].capacity(), CHUNK_LEN);
}

#[test]
fn a_clone_keeps_the_room_of_each_chunk() {
    let mut arena = Arena::new();
    let root = arena.new_node(0);
    for value in 1..70_000u32 {
        let node = arena.new_node(value);
        arena.checked_append(root, node).unwrap();
    }
    let removed = arena.last_child(root).unwrap();
    arena.remove(removed);
    let mut copy = arena.clone();
    for (original, copy) in arena.chunks.iter().zip(copy.chunks.iter()) {
        assert_eq!(original.len(), copy.len());
        assert_eq!(original.capacity(), copy.capacity());
    }
    assert_eq!(copy.chunks[1].capacity(), CHUNK_LEN);
    assert_eq!(
        copy.children(root).collect::<Vec<_>>(),
        arena.children(root).collect::<Vec<_>>()
    );
    assert_eq!(
        (copy.len, copy.first_free, copy.last_free),
        (arena.len, arena.first_free, arena.last_free)
    );
    // the copy takes the removed slot first, then fills its partly filled
    // chunk without moving it
    let start = copy.chunks[1].as_ptr();
    let again = copy.new_node(1);
    assert_eq!(my_key(again), (removed.index1.get(), 1));
    for value in 0..1000 {
        let node = copy.new_node(value);
        copy.checked_append(root, node).unwrap();
    }
    assert_eq!(copy.chunks[1].as_ptr(), start);
    // the original does not change
    assert!(arena.is_removed(removed));
    assert_eq!(arena.children(root).count(), 69_998);
    assert_eq!(copy.children(root).count(), 70_998);
}

#[test]
fn sizes() {
    use std::mem::size_of;
    // the node ids and edges, on every target
    assert_eq!(size_of::<NodeId>(), 8);
    assert_eq!(size_of::<Option<NodeId>>(), 8);
    assert_eq!(size_of::<crate::Node>(), 8);
    assert_eq!(size_of::<Option<crate::Node>>(), 8);
    assert_eq!(size_of::<crate::NodeEdge>(), 12);
    assert_eq!(size_of::<NodeEdge>(), 12);
    assert_eq!(size_of::<FreeSlot>(), 28);
    println!(
        "Value {} bytes, SlotData<Value> {}, Slot<Value> {}",
        size_of::<Value>(),
        size_of::<SlotData<Value>>(),
        size_of::<Slot<Value>>()
    );
}

/// The sizes with current compilers on 64-bit targets, which the language
/// does not promise: a free slot takes one of the values that the tag of
/// `Value` does not use, so that a slot is its value and 24 bytes.
#[cfg(target_pointer_width = "64")]
#[test]
fn sizes_on_64_bit_targets() {
    use std::mem::size_of;
    assert_eq!(size_of::<Value>(), 40);
    assert_eq!(size_of::<SlotData<Value>>(), 40);
    assert_eq!(size_of::<Slot<Value>>(), 64);
}

#[test]
fn traits() {
    fn check<T: Send + Unpin + UnwindSafe + RefUnwindSafe>(_: &T) {}
    let mut xot = crate::Xot::new();
    let root = xot.parse("<a>b</a>").unwrap();
    let edge = xot.traverse(root).next().unwrap();
    check(&xot);
    check(&root);
    check(&edge);
    check(&Arena::<Value>::new());
    // they can be shared between threads, and moved to another thread
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| xot.to_string(root).unwrap());
        assert_eq!(reader.join().unwrap(), "<a>b</a>");
        scope.spawn(|| (root, edge)).join().unwrap();
    });
}

#[test]
fn a_handle_in_a_hash_set() {
    let id = |index1: u32, generation: u32| NodeId {
        index1: NonZeroU32::new(index1).unwrap(),
        generation,
    };
    let mut set = std::collections::HashSet::new();
    for index1 in 1..100 {
        for generation in 0..3 {
            set.insert(id(index1, generation));
            set.insert(id(index1, generation));
            set.insert(crate::Node::new(id(index1, generation)).get());
        }
    }
    assert_eq!(set.len(), 99 * 3);
    assert!(set.contains(&id(5, 2)));
    assert!(!set.contains(&id(2, 5)));
}

#[test]
fn an_indextree_id_converts_to_the_node_at_its_position() {
    let mut it = indextree::Arena::new();
    let ids: Vec<_> = (0..3u32).map(|value| it.new_node(value)).collect();
    let mut xot = crate::Xot::new();
    let root = xot.parse("<a/>").unwrap();
    let element = xot.document_element(root).unwrap();
    assert_eq!(crate::Node::from(ids[0]), root);
    assert_eq!(crate::Node::from(ids[1]), element);
    assert_eq!(my_key(crate::Node::from(ids[2]).get()), (3, 0));
}

#[test]
fn append_fresh_appends_a_new_node_as_indextree_does() {
    // random trees, each new node appended with `append_fresh` here and
    // with indextree's `append` there, and nodes removed between, so that
    // new nodes take their slots
    let mut rng = Rng::new(5);
    for _ in 0..100 {
        let mut it = indextree::Arena::new();
        let mut arena = Arena::new();
        let mut live = vec![Pair {
            it: it.new_node(0),
            my: arena.new_node(0),
        }];
        for value in 1..300u32 {
            if live.len() > 1 && rng.chance(15) {
                // any node but the root
                let pair = live[1 + rng.below(live.len() - 1)];
                if rng.chance(50) {
                    pair.it.remove(&mut it);
                    arena.remove(pair.my);
                } else {
                    pair.it.remove_subtree(&mut it);
                    arena.remove_subtree(pair.my);
                }
                live.retain(|pair| !arena.is_removed(pair.my));
            } else {
                let parent = live[rng.below(live.len())];
                let node = Pair {
                    it: it.new_node(value),
                    my: arena.new_node(value),
                };
                assert_eq!(it_key(node.it), my_key(node.my));
                parent.it.append(node.it, &mut it);
                arena.append_fresh(parent.my, node.my);
                live.push(node);
            }
            for (position, node) in it.as_slice().iter().enumerate() {
                let slot = arena.slot(position as u32 + 1);
                assert_eq!(node.is_removed(), slot.is_free());
                assert_eq!(it_fields(node), slot.link);
            }
            for pair in &live {
                assert_eq!(it_read(&it, pair.it), my_read(&arena, pair.my));
            }
        }
        assert_eq!(
            it_edges(live[0].it.traverse(&it)),
            my_edges(arena.traverse(live[0].my))
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn append_fresh_checks_its_arguments_with_debug_assertions() {
    let mut arena = Arena::new();
    let parent = arena.new_node(0);
    let child = arena.new_node(1);
    arena.append_fresh(parent, child);
    let removed = arena.new_node(2);
    arena.remove(removed);
    for (parent, child, message) in [
        (parent, parent, "a node is appended to itself"),
        (removed, child, "the parent is removed"),
        (parent, removed, "the new child is removed"),
        // a child already
        (parent, child, "the new child is not a new node"),
        // the parent of a child
        (child, parent, "the new child is not a new node"),
    ] {
        let mut arena = arena.clone();
        let panic = catch(move || arena.append_fresh(parent, child)).unwrap_err();
        assert!(panic.contains(message), "{message}: {panic}");
    }
}

/// The edges and the data that `traverse_step` gives for a traversal of
/// `root`, taking one step after another.
fn stepped(arena: &Arena<u32>, root: NodeId) -> Vec<(bool, (u32, u32), Option<u32>)> {
    let mut state = arena.traverse_start(root);
    let mut steps = Vec::new();
    while let Some((edge, data)) = arena.traverse_step(&mut state) {
        let (start, node_id) = match edge {
            NodeEdge::Start(node_id) => (true, node_id),
            NodeEdge::End(node_id) => (false, node_id),
        };
        steps.push((start, my_key(node_id), data.copied()));
    }
    steps
}

#[test]
fn traverse_step_gives_the_edges_of_traverse_with_their_data() {
    let mut rng = Rng::new(11);
    for _ in 0..100 {
        let mut arena = Arena::new();
        let mut made = vec![arena.new_node(0)];
        let mut live = made.clone();
        for value in 1..200u32 {
            if live.len() > 1 && rng.chance(15) {
                // any node but the root, which is the first
                let node = live[1 + rng.below(live.len() - 1)];
                if rng.chance(50) {
                    arena.remove(node);
                    live.retain(|&h| h != node);
                } else {
                    let removed: Vec<NodeId> = arena.descendants(node).collect();
                    arena.remove_subtree(node);
                    live.retain(|h| !removed.contains(h));
                }
            } else {
                let parent = live[rng.below(live.len())];
                let node = arena.new_node(value);
                arena.checked_append(parent, node).unwrap();
                made.push(node);
                live.push(node);
            }
        }
        // every handle: live, removed, and removed with its slot taken again
        for &node in &made {
            let edges = my_edges(arena.traverse(node));
            let steps = stepped(&arena, node);
            assert_eq!(steps.len(), edges.len());
            for ((start, key, data), edge) in steps.into_iter().zip(edges) {
                assert_eq!((start, key), edge);
                let node_id = NodeId {
                    index1: NonZeroU32::new(key.0).unwrap(),
                    generation: key.1,
                };
                if arena.is_removed(node_id) {
                    assert_eq!(data, None);
                } else {
                    assert_eq!(data, Some(*arena.get(node_id)));
                }
            }
        }
    }
}

/// The nodes after `node`, as the crate's `Following` iterator gave them
/// before the arena had [`Arena::following`]: by `first_child`, else by
/// `next_sibling` of the node or of its nearest ancestor that has one, one
/// read for each step.
fn following_by_fields(arena: &Arena<u32>, node: NodeId) -> Vec<(NodeId, Option<u32>)> {
    let after = |node: NodeId| {
        if let Some(next) = arena.next_sibling(node) {
            return Some(next);
        }
        let mut current = node;
        while let Some(parent) = arena.parent(current) {
            if let Some(next) = arena.next_sibling(parent) {
                return Some(next);
            }
            current = parent;
        }
        None
    };
    let mut nodes = Vec::new();
    let mut current = after(node);
    while let Some(node) = current {
        let data = (!arena.is_removed(node)).then(|| *arena.get(node));
        nodes.push((node, data));
        current = arena.first_child(node).or_else(|| after(node));
    }
    nodes
}

#[test]
fn following_gives_the_nodes_that_the_fields_lead_to() {
    let mut rng = Rng::new(17);
    for _ in 0..100 {
        let mut arena = Arena::new();
        let mut made = vec![arena.new_node(0)];
        let mut live = made.clone();
        for value in 1..200u32 {
            if live.len() > 1 && rng.chance(15) {
                let node = live[1 + rng.below(live.len() - 1)];
                if rng.chance(50) {
                    arena.remove(node);
                    live.retain(|&h| h != node);
                } else {
                    let removed: Vec<NodeId> = arena.descendants(node).collect();
                    arena.remove_subtree(node);
                    live.retain(|h| !removed.contains(h));
                }
            } else {
                let parent = live[rng.below(live.len())];
                let node = arena.new_node(value);
                if rng.chance(20) {
                    arena.checked_prepend(parent, node).unwrap();
                } else {
                    arena.checked_append(parent, node).unwrap();
                }
                made.push(node);
                live.push(node);
            }
        }
        // every handle: live, removed, and removed with its slot taken again
        for &node in &made {
            let following: Vec<(NodeId, Option<u32>)> = arena
                .following(node)
                .map(|(node_id, data)| (node_id, data.copied()))
                .collect();
            assert_eq!(following, following_by_fields(&arena, node));
        }
    }
}

#[test]
fn traverse_step_reads_the_tree_as_it_is_when_new_nodes_take_free_slots() {
    let mut arena = Arena::new();
    let root = arena.new_node(0);
    let mut nodes = vec![root];
    for value in 1..100u32 {
        let node = arena.new_node(value);
        arena
            .checked_append(nodes[(value as usize * 5) % nodes.len()], node)
            .unwrap();
        nodes.push(node);
    }
    // free slots, from another tree
    let other = arena.new_node(1000);
    for value in 0..50 {
        let node = arena.new_node(1001 + value);
        arena.checked_append(other, node).unwrap();
    }
    arena.remove_subtree(other);
    let before = stepped(&arena, root);
    // a new node after each step, in a free slot while there is one
    let mut state = arena.traverse_start(root);
    let mut steps = Vec::new();
    let mut made: Vec<NodeId> = Vec::new();
    while let Some((edge, data)) = arena.traverse_step(&mut state) {
        let (start, node_id) = match edge {
            NodeEdge::Start(node_id) => (true, node_id),
            NodeEdge::End(node_id) => (false, node_id),
        };
        steps.push((start, my_key(node_id), data.copied()));
        let node = arena.new_node(2000);
        if let Some(&last) = made.last() {
            arena.checked_append(last, node).unwrap();
        }
        made.push(node);
    }
    assert_eq!(steps, before);
    // the new nodes took the 51 free slots and then new ones
    assert_eq!(made.len(), 200);
    assert!(made[..51].iter().all(|&node| node.generation == 1));
    assert!(made[51..].iter().all(|&node| node.generation == 0));
}

#[test]
fn get_pair_mut_gives_the_data_of_two_slots() {
    let mut arena = Arena::new();
    let nodes: Vec<NodeId> = (0..70_000u32).map(|value| arena.new_node(value)).collect();
    assert_eq!(arena.chunks.len(), 2);
    // in one chunk and in two, either way round
    for (a, b) in [(1, 2), (2, 1), (5, 66_000), (66_000, 5), (65_535, 65_536)] {
        let (x, y) = arena.get_pair_mut(nodes[a], nodes[b]).unwrap();
        assert_eq!((*x, *y), (a as u32, b as u32));
        *x += 1_000_000;
        *y += 2_000_000;
        assert_eq!(*arena.get(nodes[a]), a as u32 + 1_000_000);
        assert_eq!(*arena.get(nodes[b]), b as u32 + 2_000_000);
        *arena.get_mut(nodes[a]) = a as u32;
        *arena.get_mut(nodes[b]) = b as u32;
    }
    // one slot, also through the handle of a node that was in it before
    assert!(arena.get_pair_mut(nodes[3], nodes[3]).is_none());
    arena.remove(nodes[7]);
    let again = arena.new_node(7);
    assert_ne!(again, nodes[7]);
    assert!(arena.get_pair_mut(again, nodes[7]).is_none());
    // a removed node panics, as with get_mut
    arena.remove(nodes[9]);
    for (a, b) in [(nodes[9], nodes[10]), (nodes[10], nodes[9])] {
        let mut arena = arena.clone();
        let panic = catch(move || {
            arena.get_pair_mut(a, b);
        })
        .unwrap_err();
        assert!(panic.contains("Try to access a freed node"), "{panic}");
    }
}

/// `find_child` of `node` gives the children and the data that
/// `children_data` gives, and ends at the child for which `visit` gives a
/// result: at each child in turn if `each`, else at one picked by `rng`. The
/// children, as slot positions plus one.
fn check_find_child(arena: &Arena<u32>, node: NodeId, rng: &mut Rng, each: bool) -> Vec<u32> {
    let expected: Vec<((u32, u32), u32)> = arena
        .children_data(node)
        .map(|(child, data)| (my_key(child), *live(data)))
        .collect();
    let mut visited = Vec::new();
    let found: Option<()> = arena.find_child(node, |child, data| {
        visited.push((my_key(child), *data));
        None
    });
    assert_eq!(found, None);
    assert_eq!(visited, expected);
    let ends = if each || expected.is_empty() {
        0..expected.len()
    } else {
        let end = rng.below(expected.len());
        end..end + 1
    };
    for end in ends {
        let mut steps = 0;
        let found = arena.find_child(node, |child, _| {
            steps += 1;
            (steps > end).then_some(my_key(child))
        });
        assert_eq!(found, Some(expected[end].0));
        assert_eq!(steps, end + 1);
    }
    expected.iter().map(|((index1, _), _)| *index1).collect()
}

#[test]
fn find_child_visits_the_children_that_children_data_gives() {
    // random trees with nodes appended, prepended and put next to others, and
    // removed between, so that new nodes take free slots and the children of
    // a node are not in the order of their slots; and runs of nodes made one
    // after another for one parent, which are mostly in slots one after
    // another, in order or (prepended) in reverse
    let mut rng = Rng::new(13);
    let (mut runs, mut steps_to_the_next_slot) = (0, 0);
    for _ in 0..100 {
        let mut arena = Arena::new();
        let mut made = vec![arena.new_node(0)];
        let mut alive = made.clone();
        let mut value = 1;
        while value < 200u32 {
            if alive.len() > 1 && rng.chance(15) {
                // any node but the root, which is the first
                let node = alive[1 + rng.below(alive.len() - 1)];
                if rng.chance(50) {
                    arena.remove(node);
                } else {
                    arena.remove_subtree(node);
                }
                alive.retain(|&node| !arena.is_removed(node));
                continue;
            }
            let parent = alive[rng.below(alive.len())];
            let (kind, count) = (rng.below(3), if rng.chance(25) { 8 } else { 1 });
            let mut previous = None;
            for _ in 0..count {
                let node = arena.new_node(value);
                value += 1;
                let children: Vec<NodeId> = arena.children(parent).collect();
                match (kind, children.first(), previous) {
                    (0, Some(_), _) => arena.checked_prepend(parent, node).unwrap(),
                    // in a run, after the node made before
                    (1, _, Some(previous)) => arena.checked_insert_after(previous, node).unwrap(),
                    (1, Some(_), None) => {
                        let sibling = children[rng.below(children.len())];
                        arena.checked_insert_after(sibling, node).unwrap();
                    }
                    _ => arena.checked_append(parent, node).unwrap(),
                }
                previous = Some(node);
                made.push(node);
                alive.push(node);
            }
            runs += usize::from(count > 1);
        }
        // every handle whose slot holds a node: those of live nodes, and
        // those of removed nodes whose slot a later node took
        for &node in &made {
            if arena.is_removed(node) {
                continue;
            }
            let children = check_find_child(&arena, node, &mut rng, false);
            steps_to_the_next_slot += children.windows(2).filter(|w| w[1] == w[0] + 1).count();
        }
    }
    assert!(runs > 500, "{runs}");
    assert!(steps_to_the_next_slot > 5000, "{steps_to_the_next_slot}");
}

#[test]
fn find_child_follows_children_across_the_end_of_a_chunk() {
    let mut rng = Rng::new(17);
    // an arena with `parents` parents and other nodes up to 50 slots before
    // the end of the first chunk
    let near_the_end = |parents: u32| {
        let mut arena = Arena::new();
        let parents: Vec<NodeId> = (0..parents).map(|value| arena.new_node(value)).collect();
        while (arena.len as usize) < CHUNK_LEN - 50 {
            arena.new_node(0);
        }
        (arena, parents)
    };
    let crosses = |children: &[u32]| {
        children.iter().any(|&index1| index1 as usize <= CHUNK_LEN)
            && children.iter().any(|&index1| index1 as usize > CHUNK_LEN)
    };
    // children in slots one after another, in order
    let (mut arena, parents) = near_the_end(1);
    tree(&mut arena, parents[0], &(1..=100).collect::<Vec<_>>());
    let children = check_find_child(&arena, parents[0], &mut rng, true);
    assert!(crosses(&children));
    assert!(children.windows(2).all(|w| w[1] == w[0] + 1));
    // in reverse
    let (mut arena, parents) = near_the_end(1);
    for value in 1..=100 {
        let child = arena.new_node(value);
        arena.checked_prepend(parents[0], child).unwrap();
    }
    let children = check_find_child(&arena, parents[0], &mut rng, true);
    assert!(crosses(&children));
    assert!(children.windows(2).all(|w| w[0] == w[1] + 1));
    // in every other slot: two parents get children in turn
    let (mut arena, parents) = near_the_end(2);
    for value in 1..=100 {
        tree(&mut arena, parents[value as usize % 2], &[value]);
    }
    for &parent in &parents {
        let children = check_find_child(&arena, parent, &mut rng, true);
        assert!(crosses(&children));
        assert!(children.windows(2).all(|w| w[1] == w[0] + 2));
    }
    // a parent in the second chunk with children in both, some in slots
    // taken again, with the next generation, and its last child in the slot
    // before its first
    let (mut arena, _) = near_the_end(0);
    let children = (1..=100)
        .map(|value| arena.new_node(value))
        .collect::<Vec<_>>();
    let parent = arena.new_node(0);
    assert!(parent.index1.get() as usize > CHUNK_LEN);
    for &child in &children {
        arena.checked_append(parent, child).unwrap();
    }
    for &child in &children[45..55] {
        arena.remove(child);
    }
    let again: Vec<NodeId> = (0..10).map(|value| arena.new_node(value)).collect();
    for &child in &again {
        arena.checked_insert_after(children[44], child).unwrap();
    }
    let last = arena.new_node(7);
    arena.checked_append(parent, last).unwrap();
    let first = arena.new_node(8);
    arena.checked_prepend(parent, first).unwrap();
    let children = check_find_child(&arena, parent, &mut rng, true);
    assert!(crosses(&children));
    assert_eq!(children.len(), 102);
    assert_eq!(arena.get(again[0]), &0);
    assert_eq!(again[0].generation, 1);
    assert_eq!(first.index1.get(), last.index1.get() + 1);
}

// ---------------------------------------------------------------- the oracle

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Op {
    New,
    Append,
    Prepend,
    InsertAfter,
    InsertBefore,
    Detach,
    Remove,
    RemoveSubtree,
}

/// The result of an operation: `Err` for a panic with its message, then
/// `Err` for an error with its text.
type Ran = Result<Result<(), String>, String>;

/// A handle on each side, made by one `new_node`.
#[derive(Clone, Copy, Debug)]
struct Pair {
    it: indextree::NodeId,
    my: NodeId,
}

struct Sequence {
    it: indextree::Arena<u32>,
    my: Arena<u32>,
    pairs: Vec<Pair>,
    /// The index in `pairs` of the last handle made for each slot.
    latest: Vec<usize>,
    value: u32,
}

/// Counts of a run of the oracle.
#[derive(Default, Debug)]
struct Tally {
    sequences: usize,
    operations: BTreeMap<Op, usize>,
    errors: usize,
    panics: usize,
    cycles: usize,
}

impl Tally {
    fn steps(&self) -> usize {
        self.operations.values().sum()
    }
}

impl Sequence {
    fn new() -> Self {
        Sequence {
            it: indextree::Arena::new(),
            my: Arena::new(),
            pairs: Vec::new(),
            latest: Vec::new(),
            value: 0,
        }
    }

    /// The pairs of nodes that are not removed, and of removed nodes whose
    /// slot no later node took.
    fn current(&self) -> (Vec<Pair>, Vec<Pair>) {
        let (mut live, mut removed) = (Vec::new(), Vec::new());
        for &index in &self.latest {
            let pair = self.pairs[index];
            if self.it[pair.it].is_removed() {
                removed.push(pair);
            } else {
                live.push(pair);
            }
        }
        (live, removed)
    }

    fn new_node(&mut self) -> Result<(), String> {
        self.value += 1;
        let a = self.it.new_node(self.value);
        let b = self.my.new_node(self.value);
        if it_key(a) != my_key(b) {
            return Err(format!("new_node: {:?} against {:?}", it_key(a), my_key(b)));
        }
        let index1: usize = a.into();
        let position = index1 - 1;
        if position == self.latest.len() {
            self.latest.push(self.pairs.len());
        } else {
            self.latest[position] = self.pairs.len();
        }
        self.pairs.push(Pair { it: a, my: b });
        Ok(())
    }

    fn apply(&mut self, op: Op, x: Pair, y: Pair) -> (Ran, Ran) {
        let it = &mut self.it;
        let my = &mut self.my;
        let error = |e: indextree::NodeError| e.to_string();
        match op {
            Op::New => unreachable!(),
            Op::Append => (
                catch(|| x.it.checked_append(y.it, it).map_err(error)),
                catch(|| my.checked_append(x.my, y.my).map_err(error)),
            ),
            Op::Prepend => (
                catch(|| x.it.checked_prepend(y.it, it).map_err(error)),
                catch(|| my.checked_prepend(x.my, y.my).map_err(error)),
            ),
            Op::InsertAfter => (
                catch(|| x.it.checked_insert_after(y.it, it).map_err(error)),
                catch(|| my.checked_insert_after(x.my, y.my).map_err(error)),
            ),
            Op::InsertBefore => (
                catch(|| x.it.checked_insert_before(y.it, it).map_err(error)),
                catch(|| my.checked_insert_before(x.my, y.my).map_err(error)),
            ),
            Op::Detach => (
                catch(|| {
                    x.it.detach(it);
                    Ok(())
                }),
                catch(|| {
                    my.detach(x.my);
                    Ok(())
                }),
            ),
            Op::Remove => (
                catch(|| {
                    x.it.remove(it);
                    Ok(())
                }),
                catch(|| {
                    my.remove(x.my);
                    Ok(())
                }),
            ),
            Op::RemoveSubtree => (
                catch(|| {
                    x.it.remove_subtree(it);
                    Ok(())
                }),
                catch(|| {
                    my.remove_subtree(x.my);
                    Ok(())
                }),
            ),
        }
    }

    /// Whether indextree's nodes form a cycle: a walk up from a node, or
    /// along the children of a node, longer than there are slots.
    fn has_cycle(&self) -> bool {
        let n = self.it.count();
        for node in self.it.as_slice() {
            if node.is_removed() {
                continue;
            }
            let mut steps = 0;
            let mut up = node.parent();
            while let Some(id) = up {
                steps += 1;
                if steps > n {
                    return true;
                }
                up = self.it[id].parent();
            }
            let mut steps = 0;
            let mut along = node.first_child();
            while let Some(id) = along {
                steps += 1;
                if steps > n {
                    return true;
                }
                along = self.it[id].next_sibling();
            }
        }
        false
    }

    /// Compare the slots of both sides.
    fn compare_slots(&self) -> Result<(), String> {
        if self.it.count() != self.my.len as usize {
            return Err(format!("{} slots against {}", self.it.count(), self.my.len));
        }
        for (position, node) in self.it.as_slice().iter().enumerate() {
            let slot = self.my.slot(position as u32 + 1);
            if node.is_removed() != slot.is_free() {
                return Err(format!("slot {position}: removed {}", node.is_removed()));
            }
            if it_fields(node) != slot.link {
                return Err(format!(
                    "slot {position}: fields {:?} against {:?}",
                    it_fields(node),
                    slot.link
                ));
            }
            if let SlotData::Live(data) = &slot.data {
                if node.get() != data {
                    return Err(format!("slot {position}: data"));
                }
            }
        }
        Ok(())
    }

    /// Compare what each handle reads, of all of them or of a sample, and the
    /// iterators from a few of them.
    fn compare_reads(&self, rng: &mut Rng, all: bool) -> Result<(), String> {
        let sample: Vec<Pair> = if all || self.pairs.len() <= 24 {
            self.pairs.clone()
        } else {
            (0..24)
                .map(|_| self.pairs[rng.below(self.pairs.len())])
                .collect()
        };
        for pair in &sample {
            if it_key(pair.it) != my_key(pair.my) {
                return Err(format!("handle {:?} against {:?}", pair.it, pair.my));
            }
            let (a, b) = (it_read(&self.it, pair.it), my_read(&self.my, pair.my));
            if a != b {
                return Err(format!("read through {:?}: {a:?} against {b:?}", pair.it));
            }
            if self.it[pair.it].is_removed() != self.my.is_removed(pair.my) {
                return Err(format!("is_removed of {:?}", pair.it));
            }
        }
        let bound = self.it.count() + 2;
        let (it, my) = (&self.it, &self.my);
        for _ in 0..3.min(self.pairs.len()) {
            let pair = self.pairs[rng.below(self.pairs.len())];
            let (a, b) = (pair.it, pair.my);
            let lists = [
                (
                    "children",
                    it_keys(a.children(it).take(bound)),
                    my_keys(my.children(b).take(bound)),
                ),
                (
                    "descendants",
                    it_keys(a.descendants(it).take(bound)),
                    my_keys(my.descendants(b).take(bound)),
                ),
                (
                    "ancestors",
                    it_keys(a.ancestors(it).take(bound)),
                    my_keys(my.ancestors(b).take(bound)),
                ),
                (
                    "following_siblings",
                    it_keys(a.following_siblings(it).take(bound)),
                    my_keys(my.following_siblings(b).take(bound)),
                ),
                (
                    "preceding_siblings",
                    it_keys(a.preceding_siblings(it).take(bound)),
                    my_keys(my.preceding_siblings(b).take(bound)),
                ),
            ];
            for (name, x, y) in lists {
                if x != y {
                    return Err(format!("{name} of {a:?}: {x:?} against {y:?}"));
                }
            }
            let x = it_edges(a.traverse(it).take(2 * bound));
            let y = my_edges(my.traverse(b).take(2 * bound));
            if x != y {
                return Err(format!("traverse of {a:?}: {x:?} against {y:?}"));
            }
            let x = it_edges(a.reverse_traverse(it).take(2 * bound));
            let y = my_edges(my.reverse_traverse(b).take(2 * bound));
            if x != y {
                return Err(format!("reverse_traverse of {a:?}: {x:?} against {y:?}"));
            }
            // the data of the node in the slot, or the same panic
            let x = catch(|| *it[a].get());
            let y = catch(|| *my.get(b));
            if x != y {
                return Err(format!("get of {a:?}: {x:?} against {y:?}"));
            }
            // the children with their data, or the same panic
            let x = catch(|| {
                a.children(it)
                    .take(bound)
                    .map(|child| (it_key(child), *it[child].get()))
                    .collect::<Vec<_>>()
            });
            let y = catch(|| {
                my.children_data(b)
                    .take(bound)
                    .map(|(child, data)| (my_key(child), *live(data)))
                    .collect::<Vec<_>>()
            });
            if x != y {
                return Err(format!("children data of {a:?}: {x:?} against {y:?}"));
            }
        }
        Ok(())
    }
}

/// One random sequence of up to `steps` operations; `Err` describes the
/// first difference.
fn sequence(seed: u64, steps: usize, all_reads: bool, tally: &mut Tally) -> Result<(), String> {
    let mut rng = Rng::new(seed);
    let mut s = Sequence::new();
    tally.sequences += 1;
    // some sequences remove and make many nodes, so that slots are taken
    // again and again
    let churn = rng.chance(30);
    let weights: [(Op, usize); 8] = if churn {
        [
            (Op::New, 34),
            (Op::Append, 16),
            (Op::Prepend, 6),
            (Op::InsertAfter, 6),
            (Op::InsertBefore, 6),
            (Op::Detach, 6),
            (Op::Remove, 12),
            (Op::RemoveSubtree, 14),
        ]
    } else {
        [
            (Op::New, 24),
            (Op::Append, 22),
            (Op::Prepend, 10),
            (Op::InsertAfter, 10),
            (Op::InsertBefore, 10),
            (Op::Detach, 8),
            (Op::Remove, 8),
            (Op::RemoveSubtree, 8),
        ]
    };
    for step in 0..steps {
        let (live, removed) = s.current();
        let op = if live.len() < 2 {
            Op::New
        } else {
            let mut roll = rng.below(100);
            weights
                .iter()
                .find(|(_, weight)| {
                    let found = roll < *weight;
                    roll = roll.saturating_sub(*weight);
                    found
                })
                .map_or(Op::New, |(op, _)| *op)
        };
        *tally.operations.entry(op).or_default() += 1;
        let context = |message: String| format!("seed {seed}, step {step}, {op:?}: {message}");
        if op == Op::New {
            s.new_node().map_err(context)?;
        } else {
            // a live node, or for the checked operations sometimes a removed
            // one, and sometimes the same node twice
            let checked = matches!(
                op,
                Op::Append | Op::Prepend | Op::InsertAfter | Op::InsertBefore
            );
            let pick = |rng: &mut Rng| {
                if checked && !removed.is_empty() && rng.chance(10) {
                    removed[rng.below(removed.len())]
                } else {
                    live[rng.below(live.len())]
                }
            };
            let x = pick(&mut rng);
            let y = if rng.chance(5) { x } else { pick(&mut rng) };
            let (a, b) = s.apply(op, x, y);
            if a != b {
                return Err(context(format!(
                    "{:?} {:?}: indextree {a:?}, arena {b:?}",
                    x.it, y.it
                )));
            }
            match a {
                Err(_) => {
                    tally.panics += 1;
                    return Ok(());
                }
                Ok(Err(_)) => tally.errors += 1,
                Ok(Ok(())) => {}
            }
        }
        s.compare_slots().map_err(context)?;
        if s.has_cycle() {
            tally.cycles += 1;
            return Ok(());
        }
        s.compare_reads(&mut rng, all_reads).map_err(context)?;
    }
    Ok(())
}

#[test]
fn oracle_against_indextree() {
    let mut tally = Tally::default();
    for seed in 0..1000 {
        if let Err(message) = sequence(seed, 100, false, &mut tally) {
            panic!("{message}");
        }
    }
    // the sequences reach every operation, errors, panics and cycles
    assert_eq!(tally.sequences, 1000);
    assert_eq!(tally.operations.len(), 8, "{tally:?}");
    assert!(
        tally.errors > 0 && tally.panics > 0 && tally.cycles > 0,
        "{tally:?}"
    );
    assert!(tally.steps() > 20_000, "{tally:?}");
}

/// The soak: at least 200,000 operations, with reads through every handle
/// after every step. Run it with `cargo test --release --lib oracle_soak --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn oracle_soak() {
    let mut tally = Tally::default();
    let mut seed = 1_000_000;
    while tally.steps() < 200_000 {
        if let Err(message) = sequence(seed, 150, true, &mut tally) {
            panic!("{message}");
        }
        seed += 1;
    }
    println!("oracle soak: {tally:?}, {} operations", tally.steps());
}
