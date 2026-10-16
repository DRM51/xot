//! Tests of `clone_node` and of the joining of adjacent texts, against the
//! code they replace, and with a few examples.
//!
//! `clone_node` collected all the edges of the tree it copies before it made
//! the copies, and texts were joined through copies of both strings. The
//! comparisons (`*_as_before`) run the old code (`clone_node_by_edges`,
//! `add_consolidate_by_copies` and `remove_consolidate_by_copies`, as they
//! were) and the new code on clones of one `Xot`, made by random edits with
//! text consolidation on and off, and require the same result (node, `bool`
//! or panic message) and the same slots: values, fields, generations and the
//! free list. The nodes they get are live, removed, or removed with their
//! slot taken again, and the trees have namespace and attribute nodes after
//! other children, keys that two namespace or attribute nodes of one element
//! share, adjacent texts, document nodes below elements, and other nodes
//! whose kind `value_mut` changed. `clone_node_by_edges` puts the copies in
//! place with the crate's `any_append`, whose joining of texts the other
//! comparison checks. The edits make no cycles, which `insert_after`,
//! `insert_before`, `prepend` and `any_append` can make: walks through such
//! trees never end, with the old code and the new.
use std::panic::{self, AssertUnwindSafe};

use crate::xmlvalue::{Attribute, Comment, Element, Namespace, ProcessingInstruction, Text};
use crate::{NameId, NamespaceId, Node, NodeEdge, PrefixId, Value, ValueType, Xot};

// ---------------------------------------------------------------- helpers

/// Run `f`; `Err` holds the message of its panic. (The test harness keeps
/// what the panic prints, unless the test fails.)
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|cause| {
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

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            Some(items[self.below(items.len())])
        }
    }
}

/// The first difference between the slots of two `Xot`s, if any.
fn slot_difference(a: &Xot, b: &Xot) -> Option<String> {
    a.arena.slot_difference(&b.arena)
}

// ------------------------------------------------- the code it replaces

/// `clone_node` as it was: the edges of the tree first, then a copy of each
/// node, which `any_append` puts in place.
fn clone_node_by_edges(xot: &mut Xot, node: Node) -> Node {
    let value = xot.value(node);
    let top = match value {
        Value::Document => xot.new_document(),
        Value::Element(element) => xot.new_element(element.name()),
        _ => {
            return xot.new_node(value.clone());
        }
    };
    let edges = xot.all_traverse(node).collect::<Vec<_>>();
    let mut current = top;
    for open_close in edges {
        match open_close {
            NodeEdge::Start(node) => {
                let value = xot.value(node);
                let value_type = value.value_type();
                if value_type == ValueType::Document {
                    continue;
                }
                let new_node = xot.new_node(value.clone());
                xot.any_append(current, new_node).unwrap();
                if value_type == ValueType::Element {
                    current = new_node;
                }
            }
            NodeEdge::End(node) => {
                if xot.value_type(node) != ValueType::Element {
                    continue;
                }
                current = xot.parent(current).unwrap();
            }
        }
    }
    if xot.is_element(top) {
        let cloned_node = xot.first_child(top).unwrap();
        xot.remove_dangerously(top);
        cloned_node
    } else {
        top
    }
}

/// `clone_with_prefixes` with `clone_node_by_edges`.
fn clone_with_prefixes_by_edges(xot: &mut Xot, node: Node) -> Node {
    let declarations = xot.inherited_declarations(node);
    let clone = clone_node_by_edges(xot, node);
    if xot.is_element(clone) {
        let mut namespaces = xot.namespaces_mut(clone);
        for (prefix, ns) in declarations {
            if namespaces.contains_key(prefix) {
                continue;
            }
            namespaces.insert(prefix, ns);
        }
    }
    clone
}

/// `add_consolidate_text_nodes` as it was, with a copy of each string.
fn add_consolidate_by_copies(
    xot: &mut Xot,
    node: Node,
    prev_node: Option<Node>,
    next_node: Option<Node>,
) -> bool {
    if !xot.text_consolidation {
        return false;
    }
    let Value::Text(t) = xot.value(node) else {
        return false;
    };
    let added_text = t.get().to_string();
    if let Some(prev_node) = prev_node {
        if let Value::Text(prev) = xot.value_mut(prev_node) {
            let mut s = prev.get().to_string();
            s.push_str(&added_text);
            prev.set(s);
            xot.remove_dangerously(node);
            return true;
        }
    }
    if let Some(next_node) = next_node {
        if let Value::Text(next) = xot.value_mut(next_node) {
            let mut s = added_text;
            s.push_str(next.get());
            next.set(s);
            xot.remove_dangerously(node);
            return true;
        }
    }
    false
}

/// `remove_consolidate_text_nodes` as it was, with a copy of each string.
fn remove_consolidate_by_copies(
    xot: &mut Xot,
    prev_node: Option<Node>,
    next_node: Option<Node>,
) -> bool {
    if !xot.text_consolidation {
        return false;
    }
    let (Some(prev_node), Some(next_node)) = (prev_node, next_node) else {
        return false;
    };
    let prev_text = xot.text(prev_node);
    let next_text = xot.text(next_node);
    let (Some(_), Some(next_text)) = (prev_text, next_text) else {
        return false;
    };
    let to_add = next_text.get().to_string();
    let prev_text_mut = xot.text_mut(prev_node).unwrap();
    let mut s = prev_text_mut.get().to_string();
    s.push_str(&to_add);
    prev_text_mut.set(s);
    xot.remove_dangerously(next_node);
    true
}

// ------------------------------------------------- random trees

const DOCUMENTS: &[&str] = &[
    r#"<a xmlns:p="urn:p" p:x="1" y="2">t<b>u<!--c--><?pi d?></b>v<c/></a>"#,
    r#"<a xmlns="urn:q" xmlns:p="urn:p"><p:b q="1" r="2"><c>w</c>x<d/>y</p:b>z</a>"#,
    r#"<!--before--><a><b/>t<c xmlns:r="urn:r" r:s="3">u<e>v</e></c></a><?after?>"#,
];

const TEXTS: &[&str] = &["", "a", "bc", " ", "def\n"];

/// A `Xot` with a few trees, made by random edits, and the nodes they
/// made: those that are not removed, and those that are, some with their
/// slot taken again.
struct Trees {
    xot: Xot,
    live: Vec<Node>,
    removed: Vec<Node>,
}

impl Trees {
    fn new(seed: u64, edits: usize) -> Self {
        let mut rng = Rng::new(seed);
        let mut xot = Xot::new();
        let document = xot.parse(DOCUMENTS[rng.below(DOCUMENTS.len())]).unwrap();
        let live = xot.all_descendants(document).collect();
        let mut trees = Trees {
            xot,
            live,
            removed: Vec::new(),
        };
        for _ in 0..edits {
            // an edit that panics is undone
            let before = trees.xot.clone();
            let made = catch(AssertUnwindSafe(|| trees.edit(&mut rng)));
            match made {
                Ok(made) => trees.live.extend(made),
                Err(_) => trees.xot = before,
            }
            // the nodes the edit removed, texts joined with others included
            let xot = &trees.xot;
            let (live, removed): (Vec<Node>, Vec<Node>) =
                trees.live.iter().partition(|&&node| !xot.is_removed(node));
            trees.live = live;
            trees.removed.extend(removed);
        }
        trees
    }

    fn names(xot: &mut Xot) -> (Vec<NameId>, Vec<PrefixId>, Vec<NamespaceId>) {
        let p = xot.add_namespace("urn:p");
        let q = xot.add_namespace("urn:q");
        let names = vec![
            xot.add_name("a"),
            xot.add_name("b"),
            xot.add_name("x"),
            xot.add_name_ns("x", p),
            xot.add_name_ns("y", q),
        ];
        let prefixes = vec![
            xot.empty_prefix(),
            xot.add_prefix("p"),
            xot.add_prefix("q"),
            xot.add_prefix("r"),
        ];
        (names, prefixes, vec![p, q])
    }

    /// A value of any kind.
    fn value(xot: &mut Xot, rng: &mut Rng) -> Value {
        let (names, prefixes, namespaces) = Self::names(xot);
        let name = rng.pick(&names).unwrap();
        match rng.below(8) {
            0 => Value::Document,
            1 | 2 => Value::Element(Element::new(name)),
            3 => Value::Text(Text::new(rng.pick(TEXTS).unwrap().to_string())),
            4 => Value::Comment(Comment::new("c".to_string())),
            5 => Value::ProcessingInstruction(ProcessingInstruction::new(name, None)),
            6 => Value::Attribute(Attribute {
                name_id: name,
                value: rng.pick(TEXTS).unwrap().to_string(),
            }),
            _ => Value::Namespace(Namespace {
                prefix_id: rng.pick(&prefixes).unwrap(),
                namespace_id: rng.pick(&namespaces).unwrap(),
            }),
        }
    }

    /// Whether `node` is `reference` or one of its ancestors: putting it next
    /// to `reference` would make a cycle.
    fn encloses(&self, node: Node, reference: Node) -> bool {
        self.xot
            .ancestors(reference)
            .any(|ancestor| ancestor == node)
    }

    /// One random edit; the nodes it made.
    fn edit(&mut self, rng: &mut Rng) -> Vec<Node> {
        if self.live.is_empty() {
            // all was removed: a new document
            let document = self
                .xot
                .parse(DOCUMENTS[rng.below(DOCUMENTS.len())])
                .unwrap();
            return self.xot.all_descendants(document).collect();
        }
        let live = self.live.clone();
        let xot = &self.xot;
        let elements: Vec<Node> = live
            .iter()
            .copied()
            .filter(|&n| xot.is_element(n))
            .collect();
        let parents: Vec<Node> = live
            .iter()
            .copied()
            .filter(|&n| xot.is_element(n) || xot.is_document(n))
            .collect();
        let any = rng.pick(&live).unwrap();
        let other = rng.pick(&live).unwrap();
        let (names, prefixes, namespaces) = Self::names(&mut self.xot);
        let xot = &mut self.xot;
        let mut made = Vec::new();
        // the namespace and attribute nodes of an element
        let maps: Vec<Node> = live
            .iter()
            .copied()
            .filter(|&n| {
                matches!(xot.value(n), Value::Namespace(_) | Value::Attribute(_))
                    && xot.parent(n).is_some()
            })
            .collect();
        match rng.below(24) {
            0 => made.push(xot.new_element(rng.pick(&names).unwrap())),
            1 | 2 => made.push(xot.new_text(rng.pick(TEXTS).unwrap())),
            3 => {
                let value = Self::value(xot, rng);
                made.push(xot.new_node(value));
            }
            4 | 5 => {
                if let Some(parent) = rng.pick(&parents) {
                    let _ = xot.append(parent, other);
                }
            }
            // `prepend` puts a node after the namespace and attribute nodes
            // without the check of `append`
            6 => {
                if let Some(parent) = rng.pick(&parents) {
                    if !self.encloses(other, parent) {
                        let _ = self.xot.prepend(parent, other);
                    }
                }
            }
            7 | 8 => {
                if !self.encloses(other, any) {
                    let xot = &mut self.xot;
                    let _ = if rng.chance(50) {
                        xot.insert_after(any, other)
                    } else {
                        xot.insert_before(any, other)
                    };
                }
            }
            9 => {
                let _ = xot.remove(any);
            }
            10 => {
                let _ = xot.detach(any);
            }
            11 => {
                if let Some(element) = rng.pick(&elements) {
                    let prefix = rng.pick(&prefixes).unwrap();
                    let namespace = rng.pick(&namespaces).unwrap();
                    xot.namespaces_mut(element).insert(prefix, namespace);
                }
            }
            12 => {
                if let Some(element) = rng.pick(&elements) {
                    let name = rng.pick(&names).unwrap();
                    let value = rng.pick(TEXTS).unwrap().to_string();
                    xot.attributes_mut(element).insert(name, value);
                }
            }
            // and so does `any_append` for a namespace or an attribute node,
            // which `value_mut` can make of an element
            13 => {
                if let Some(parent) = rng.pick(&parents) {
                    if !self.encloses(other, parent) {
                        let _ = self.xot.any_append(parent, other);
                    }
                }
            }
            14 => {
                let value = Self::value(xot, rng);
                *xot.value_mut(any) = value;
            }
            15 => xot.set_text_consolidation(rng.chance(50)),
            16 => {
                if let Some(element) = rng.pick(&elements) {
                    let _ = xot.element_unwrap(element);
                }
            }
            17 => {
                if let Ok(wrapper) = xot.element_wrap(any, rng.pick(&names).unwrap()) {
                    made.push(wrapper);
                }
            }
            18 => {
                if let Some(parent) = rng.pick(&parents) {
                    let _ = xot.append_text(parent, rng.pick(TEXTS).unwrap());
                }
            }
            // a namespace or attribute node gets the key of another one of
            // its element
            20 => {
                if let Some(node) = rng.pick(&maps) {
                    let parent = xot.parent(node).unwrap();
                    let others: Vec<Node> = xot.all_children(parent).collect();
                    let other = rng.pick(&others).unwrap();
                    let value = match (xot.value(node), xot.value(other)) {
                        (Value::Namespace(node), Value::Namespace(other)) => {
                            Value::Namespace(Namespace {
                                prefix_id: other.prefix(),
                                namespace_id: node.namespace(),
                            })
                        }
                        (Value::Attribute(node), Value::Attribute(other)) => {
                            Value::Attribute(Attribute {
                                name_id: other.name(),
                                value: node.value().to_string(),
                            })
                        }
                        (value, _) => value.clone(),
                    };
                    *xot.value_mut(node) = value;
                }
            }
            // a node goes after a namespace or attribute node
            21 => {
                if let Some(node) = rng.pick(&maps) {
                    let new = if rng.chance(50) {
                        xot.new_text(rng.pick(TEXTS).unwrap())
                    } else {
                        xot.new_element(rng.pick(&names).unwrap())
                    };
                    made.push(new);
                    let _ = xot.insert_after(node, new);
                }
            }
            _ => made.push(if rng.chance(70) {
                xot.clone_node(any)
            } else {
                xot.clone_with_prefixes(any)
            }),
        }
        // the copies, and any other node it made, as far as they are found
        let found: Vec<Node> = made
            .iter()
            .flat_map(|&node| self.xot.all_descendants(node).collect::<Vec<_>>())
            .collect();
        found
    }

    /// A node to give to the code under test: mostly one that is not
    /// removed, often an element or a document, sometimes a removed one,
    /// whose slot may be taken again.
    fn node(&self, rng: &mut Rng) -> Node {
        if !self.removed.is_empty() && rng.chance(10) {
            return rng.pick(&self.removed).unwrap();
        }
        if rng.chance(50) {
            let xot = &self.xot;
            let parents: Vec<Node> = self
                .live
                .iter()
                .copied()
                .filter(|&n| xot.is_element(n) || xot.is_document(n))
                .collect();
            if let Some(parent) = rng.pick(&parents) {
                return parent;
            }
        }
        rng.pick(&self.live).unwrap()
    }
}

// ------------------------------------------------- the comparisons

/// How often the comparison of `clone_node` got the cases it treats in its
/// own way.
#[derive(Default, Debug)]
struct Seen {
    /// a removed node, which panics
    removed: usize,
    /// a removed node whose slot a later node took
    reused: usize,
    /// an element with a namespace or attribute node after a child of
    /// another kind, or a namespace node after an attribute
    unordered: usize,
    /// an element with two namespace nodes of one prefix, or two
    /// attributes of one name
    shared_keys: usize,
    /// two texts next to each other
    adjacent_texts: usize,
    /// a document node below the node
    inner_document: usize,
}

impl Seen {
    fn add(&mut self, trees: &Trees, node: Node) {
        let xot = &trees.xot;
        if trees.removed.contains(&node) {
            if xot.is_removed(node) {
                self.removed += 1;
                return;
            }
            self.reused += 1;
        }
        let mut unordered = false;
        let mut shared_keys = false;
        let mut adjacent_texts = false;
        let mut inner_document = false;
        for descendant in xot.all_descendants(node) {
            if descendant != node && xot.is_document(descendant) {
                inner_document = true;
            }
            let children: Vec<&Value> =
                xot.all_children(descendant).map(|c| xot.value(c)).collect();
            let rank = |value: &Value| match value {
                Value::Namespace(_) => 0,
                Value::Attribute(_) => 1,
                _ => 2,
            };
            unordered |= children.windows(2).any(|w| rank(w[0]) > rank(w[1]));
            adjacent_texts |= children
                .windows(2)
                .any(|w| matches!((w[0], w[1]), (Value::Text(_), Value::Text(_))));
            for (i, a) in children.iter().enumerate() {
                for b in &children[i + 1..] {
                    shared_keys |= match (a, b) {
                        (Value::Namespace(a), Value::Namespace(b)) => a.prefix() == b.prefix(),
                        (Value::Attribute(a), Value::Attribute(b)) => a.name() == b.name(),
                        _ => false,
                    };
                }
            }
        }
        self.unordered += usize::from(unordered);
        self.shared_keys += usize::from(shared_keys);
        self.adjacent_texts += usize::from(adjacent_texts);
        self.inner_document += usize::from(inner_document);
    }
}

/// Copies of random nodes of the trees of `seeds`, with the old code and
/// with the new; panics at the first difference. How many copies had more
/// than one node, one node, or panicked, and the cases seen.
fn compare_clones(seeds: std::ops::Range<u64>) -> ([usize; 3], Seen) {
    let mut runs = [0usize; 3];
    let mut seen = Seen::default();
    for seed in seeds {
        let mut rng = Rng::new(seed);
        let trees = Trees::new(seed, 60);
        for _ in 0..12 {
            let node = trees.node(&mut rng);
            let consolidation = rng.chance(70);
            let with_prefixes = rng.chance(30);
            let mut old = trees.xot.clone();
            let mut new = trees.xot.clone();
            old.set_text_consolidation(consolidation);
            new.set_text_consolidation(consolidation);
            let (old_result, new_result) = if with_prefixes {
                (
                    catch(|| clone_with_prefixes_by_edges(&mut old, node)),
                    catch(|| new.clone_with_prefixes(node)),
                )
            } else {
                (
                    catch(|| clone_node_by_edges(&mut old, node)),
                    catch(|| new.clone_node(node)),
                )
            };
            seen.add(&trees, node);
            let context = format!("seed {seed}, {node:?}, consolidation {consolidation}");
            assert_eq!(old_result, new_result, "{context}");
            if let Some(difference) = slot_difference(&old, &new) {
                panic!("{context}: {difference}");
            }
            let kind = match &new_result {
                Ok(copy) if new.all_descendants(*copy).count() > 1 => 0,
                Ok(_) => 1,
                Err(_) => 2,
            };
            runs[kind] += 1;
        }
    }
    (runs, seen)
}

/// Joins of random texts and other nodes of the trees of `seeds`, with the
/// old code and with the new; panics at the first difference. How many
/// times the removing join and the adding one joined texts.
fn compare_joins(seeds: std::ops::Range<u64>) -> [usize; 2] {
    let mut joined = [0usize; 2];
    for seed in seeds {
        let mut rng = Rng::new(seed);
        let trees = Trees::new(seed, 40);
        let texts: Vec<Node> = trees
            .live
            .iter()
            .copied()
            .filter(|&n| trees.xot.is_text(n))
            .collect();
        for _ in 0..20 {
            // mostly texts, sometimes another node, a removed one or none
            let pick = |rng: &mut Rng| -> Option<Node> {
                match rng.below(10) {
                    0 => None,
                    1 | 2 => Some(trees.node(rng)),
                    _ => rng.pick(&texts).or_else(|| Some(trees.node(rng))),
                }
            };
            let node = pick(&mut rng).unwrap_or_else(|| trees.node(&mut rng));
            let prev_node = if rng.chance(10) {
                Some(node)
            } else {
                pick(&mut rng)
            };
            let next_node = if rng.chance(10) {
                Some(node)
            } else {
                pick(&mut rng)
            };
            let consolidation = rng.chance(85);
            let mut old = trees.xot.clone();
            let mut new = trees.xot.clone();
            old.set_text_consolidation(consolidation);
            new.set_text_consolidation(consolidation);
            let adding = rng.chance(50);
            let (old_result, new_result) = if adding {
                (
                    catch(|| add_consolidate_by_copies(&mut old, node, prev_node, next_node)),
                    catch(|| new.add_consolidate_text_nodes(node, prev_node, next_node)),
                )
            } else {
                (
                    catch(|| remove_consolidate_by_copies(&mut old, prev_node, next_node)),
                    catch(|| new.remove_consolidate_text_nodes(prev_node, next_node)),
                )
            };
            let context =
                format!("seed {seed}, adding {adding}, {node:?}, {prev_node:?}, {next_node:?}");
            assert_eq!(old_result, new_result, "{context}");
            if let Some(difference) = slot_difference(&old, &new) {
                panic!("{context}: {difference}");
            }
            if new_result == Ok(true) {
                joined[usize::from(adding)] += 1;
            }
        }
    }
    joined
}

#[test]
fn clone_node_copies_as_before() {
    let (runs, seen) = compare_clones(0..1000);
    // trees were copied, single nodes too, and some copies panicked
    assert!(runs.iter().all(|&count| count > 200), "{runs:?}");
    // and the trees had each of the cases above
    let Seen {
        removed,
        reused,
        unordered,
        shared_keys,
        adjacent_texts,
        inner_document,
    } = seen;
    for count in [
        removed,
        reused,
        unordered,
        shared_keys,
        adjacent_texts,
        inner_document,
    ] {
        assert!(count > 50, "{seen:?}");
    }
}

#[test]
fn texts_join_as_before() {
    let joined = compare_joins(10_000..11_000);
    // both functions joined texts many times
    assert!(joined.iter().all(|&count| count > 500), "{joined:?}");
}

/// The soak: thirty times as many trees. Run it with `cargo test --release
/// --lib manipulation -- --ignored --nocapture`.
#[test]
#[ignore]
fn clone_and_join_soak() {
    let (runs, seen) = compare_clones(1_000_000..1_030_000);
    let joined = compare_joins(2_000_000..2_030_000);
    println!("clone_node: {runs:?} {seen:?}; joined: {joined:?}");
}

// ------------------------------------------------- examples

/// The children of `node`, namespace and attribute nodes included, as text.
fn children(xot: &Xot, node: Node) -> Vec<String> {
    xot.all_children(node)
        .map(|child| match xot.value(child) {
            Value::Text(text) => format!("text {:?}", text.get()),
            Value::Element(element) => format!("element {}", xot.local_name_str(element.name())),
            Value::Attribute(attribute) => format!(
                "attribute {}={:?}",
                xot.local_name_str(attribute.name()),
                attribute.value()
            ),
            Value::Namespace(namespace) => format!(
                "namespace {}={}",
                xot.prefix_str(namespace.prefix()),
                xot.namespace_str(namespace.namespace())
            ),
            other => format!("{:?}", other.value_type()),
        })
        .collect()
}

#[test]
fn a_clone_of_mixed_content_has_the_same_nodes_in_the_same_order() {
    let mut xot = Xot::new();
    let text = r#"<r><a xmlns:p="urn:p" xmlns="urn:d" p:x="1" y="2">t<p:b>u<!--c--><?pi d?></p:b>v<c/>w</a></r>"#;
    let root = xot.parse(text).unwrap();
    let r = xot.document_element(root).unwrap();
    let a = xot.first_child(r).unwrap();
    let source: Vec<Node> = xot.all_descendants(root).collect();
    let copy = xot.clone_node(a);
    // the source is as it was
    assert_eq!(xot.to_string(root).unwrap(), text);
    assert_eq!(xot.all_descendants(root).collect::<Vec<_>>(), source);
    // the copy has no parent, the same nodes in the same order, and none of
    // the source's
    assert_eq!(xot.parent(copy), None);
    let copied: Vec<Node> = xot.all_descendants(copy).collect();
    let originals: Vec<Node> = xot.all_descendants(a).collect();
    assert_eq!(copied.len(), originals.len());
    for (copy, original) in copied.iter().zip(&originals) {
        assert_ne!(copy, original);
        assert!(!source.contains(copy));
        assert_eq!(xot.value(*copy), xot.value(*original));
        assert_eq!(children(&xot, *copy), children(&xot, *original));
    }
    assert_eq!(
        children(&xot, copy),
        [
            "namespace p=urn:p",
            "namespace =urn:d",
            "attribute x=\"1\"",
            "attribute y=\"2\"",
            "text \"t\"",
            "element b",
            "text \"v\"",
            "element c",
            "text \"w\"",
        ]
    );
    assert_eq!(
        xot.to_string(copy).unwrap(),
        r#"<a xmlns:p="urn:p" xmlns="urn:d" p:x="1" y="2">t<p:b>u<!--c--><?pi d?></p:b>v<c/>w</a>"#
    );
    // a document is copied with its document node
    let document = xot.clone_node(root);
    assert!(xot.is_document(document));
    assert_eq!(xot.to_string(document).unwrap(), text);
}

#[test]
fn a_clone_joins_adjacent_texts_if_consolidation_is_on() {
    let mut xot = Xot::new();
    let x = xot.add_name("x");
    let e = xot.add_name("e");
    xot.set_text_consolidation(false);
    let element = xot.new_element(e);
    for text in ["a", "b", "c"] {
        xot.append_text(element, text).unwrap();
    }
    xot.append_element(element, x).unwrap();
    for text in ["d", "e"] {
        xot.append_text(element, text).unwrap();
    }
    assert_eq!(xot.children(element).count(), 6);
    // consolidation off: the copy has the same texts
    let copy = xot.clone_node(element);
    assert_eq!(
        children(&xot, copy),
        [
            "text \"a\"",
            "text \"b\"",
            "text \"c\"",
            "element x",
            "text \"d\"",
            "text \"e\""
        ]
    );
    // on: each run of texts is one text in the copy, in the first text node
    // of the run; the others were made and removed
    xot.set_text_consolidation(true);
    let copy = xot.clone_node(element);
    assert_eq!(
        children(&xot, copy),
        ["text \"abc\"", "element x", "text \"de\""]
    );
    assert_eq!(xot.children(element).count(), 6);
}

#[test]
fn a_text_joins_the_text_before_it_or_else_after_it() {
    let mut xot = Xot::new();
    let root = xot.parse("<a>one<b/>two</a>").unwrap();
    let a = xot.document_element(root).unwrap();
    let one = xot.first_child(a).unwrap();
    let b = xot.next_sibling(one).unwrap();
    let two = xot.next_sibling(b).unwrap();

    // appended after a text: the text before it grows, the new node goes
    let added = xot.new_text("+three");
    xot.append(a, added).unwrap();
    assert!(xot.is_removed(added));
    assert_eq!(xot.text_str(two), Some("two+three"));

    // inserted before a text, with no text before it: the text after it
    // grows at the start
    let added = xot.new_text("zero+");
    xot.insert_before(one, added).unwrap();
    assert!(xot.is_removed(added));
    assert_eq!(xot.text_str(one), Some("zero+one"));
    let added = xot.new_text("<");
    xot.prepend(a, added).unwrap();
    assert!(xot.is_removed(added));
    assert_eq!(xot.text_str(one), Some("<zero+one"));

    // inserted after a text that has an element after it
    let added = xot.new_text(">");
    xot.insert_after(one, added).unwrap();
    assert!(xot.is_removed(added));
    assert_eq!(xot.text_str(one), Some("<zero+one>"));

    // removing the element between two texts: the first grows, the second
    // goes
    xot.remove(b).unwrap();
    assert!(xot.is_removed(two));
    assert_eq!(xot.text_str(one), Some("<zero+one>two+three"));
    assert_eq!(xot.children(a).collect::<Vec<_>>(), [one]);

    // the same with detach and element_unwrap
    let root = xot.parse("<a>1<b/>2<c>3</c>4</a>").unwrap();
    let a = xot.document_element(root).unwrap();
    let first = xot.first_child(a).unwrap();
    let b = xot.next_sibling(first).unwrap();
    xot.detach(b).unwrap();
    assert_eq!(xot.text_str(first), Some("12"));
    let c = xot.next_sibling(first).unwrap();
    xot.element_unwrap(c).unwrap();
    assert_eq!(xot.text_str(first), Some("1234"));
    assert_eq!(xot.children(a).collect::<Vec<_>>(), [first]);
    assert_eq!(xot.to_string(root).unwrap(), "<a>1234</a>");
}

#[test]
fn texts_are_joined_in_the_string_of_the_text_that_stays() {
    let mut xot = Xot::new();
    let root = xot.parse("<a>one</a>").unwrap();
    let a = xot.document_element(root).unwrap();
    let one = xot.first_child(a).unwrap();
    let mut room = String::with_capacity(100);
    room.push_str("one");
    xot.text_mut(one).unwrap().set(room);
    let start = xot.text_str(one).unwrap().as_ptr();
    xot.append_text(a, "+two").unwrap();
    xot.append_text(a, "+three").unwrap();
    // the string had room, so it did not move
    assert_eq!(xot.text_str(one), Some("one+two+three"));
    assert_eq!(xot.text_str(one).unwrap().as_ptr(), start);
}

#[test]
fn a_text_joined_with_itself_is_doubled() {
    // a text node is joined with itself where it is given as its own
    // neighbour, also through the handle of a removed node in its slot: its
    // text is doubled, as when the text that was added was a copy
    let mut xot = Xot::new();
    let one = xot.new_text("one");
    xot.join_texts(one, one, false);
    assert_eq!(xot.text_str(one), Some("oneone"));
    xot.join_texts(one, one, true);
    assert_eq!(xot.text_str(one), Some("oneoneoneone"));
    xot.remove(one).unwrap();
    let two = xot.new_text("two");
    assert!(!xot.is_removed(one));
    xot.join_texts(two, one, true);
    assert_eq!(xot.text_str(two), Some("twotwo"));
}
