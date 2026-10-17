//! Tests of the inserts of the node maps against the code they replace, and
//! of `Extend` against inserts one at a time.
//!
//! `insert` and `insert_node` looked for the key among the nodes of the map,
//! and then for a new node again for its insertion point, from the first
//! child each time. The comparisons (`*_as_before`) run the old code
//! (`insert_as_before` and `insert_node_as_before`, as they were) and the new
//! code on clones of one `Xot`, made by random edits (see
//! [`crate::manipulation::tests::Trees`]), and require the same result
//! (value, node or panic message) and the same slots: values, fields,
//! generations and the free list. `extend` is compared in the same way with
//! `insert` of its pairs one at a time, and with the old `insert`.
//!
//! The elements have namespace and attribute nodes after other children, a
//! namespace node after an attribute, and keys that two nodes of one map
//! share; some get no element, a removed node or a node whose slot was
//! taken again. The keys include ids that share their bit in the filter of
//! `extend` (ids 64 apart), so that it looks for keys that the map does not
//! have; the pairs given to `extend` repeat keys, and have keys that the map
//! has.
use std::hash::Hash;

use super::{MutableNodeMap, ValueAdapter};
use crate::manipulation::tests::{catch, slot_difference, Rng, Trees};
use crate::nodemap::attribute::AttributeAdapter;
use crate::nodemap::namespace::NamespaceAdapter;
use crate::xmlvalue::{Attribute, Namespace, ValueCategory};
use crate::{NameId, NamespaceId, Node, PrefixId, Value, Xot};

// ------------------------------------------------- the code it replaces

/// The insertion point of a new node of a map, as each adapter found it.
trait InsertionPointAsBefore {
    fn insertion_point_as_before(xot: &Xot, parent: Node) -> Option<Node>;
}

impl InsertionPointAsBefore for NamespaceAdapter {
    fn insertion_point_as_before(xot: &Xot, node: Node) -> Option<Node> {
        let last_child = <Self as ValueAdapter<PrefixId, NamespaceId>>::children(xot, node).last();
        // if there is a last child, insert after it
        if let Some(last_child) = last_child {
            return Some(last_child);
        }
        // if there is no namespace node, we want to prepend
        None
    }
}

impl InsertionPointAsBefore for AttributeAdapter {
    fn insertion_point_as_before(xot: &Xot, node: Node) -> Option<Node> {
        let last_child = <Self as ValueAdapter<NameId, String>>::children(xot, node).last();
        // if there is a last child, insert after it
        if let Some(last_child) = last_child {
            return Some(last_child);
        }
        // if there is no last child, then insert after the last namespace node
        let namespaces = xot
            .all_children_values(node)
            .take_while(|(_, value)| value.value_category() == ValueCategory::Namespace);
        if let Some((last_namespace, _)) = namespaces.last() {
            return Some(last_namespace);
        }
        // if there is no namespace node, we want to prepend
        None
    }
}

/// `MutableNodeMap::insert` as it was.
fn insert_as_before<K, V, A>(
    map: &mut MutableNodeMap<'_, K, V, A>,
    key: impl Into<K> + Copy,
    value: V,
) -> Option<V>
where
    K: PartialEq + Eq + Clone + Copy + Hash,
    V: Clone,
    A: ValueAdapter<K, V> + InsertionPointAsBefore,
{
    let node = map.get_node(key);
    if let Some(node) = node {
        // if we already have a node
        let node_value = map.xot.value_mut(node);
        A::update(node_value, value)
    } else {
        // we need to insert a new node
        let new_value = A::create(key.into(), value);
        let node = map.xot.arena.new_node(new_value);
        let insertion_point = A::insertion_point_as_before(map.xot, map.parent);
        if let Some(insertion_point) = insertion_point {
            map.xot
                .arena
                .checked_insert_after(insertion_point.get(), node)
                .unwrap();
        } else {
            map.xot
                .arena
                .checked_prepend(map.parent.get(), node)
                .unwrap();
        }
        None
    }
}

/// `MutableNodeMap::insert_node` as it was.
fn insert_node_as_before<K, V, A>(map: &mut MutableNodeMap<'_, K, V, A>, node: Node) -> Node
where
    K: PartialEq + Eq + Clone + Copy + Hash,
    V: Clone,
    A: ValueAdapter<K, V> + InsertionPointAsBefore,
{
    let node_value = map.xot.value(node);
    if !A::matches(node_value) {
        panic!("Tried to insert unexpected node value into the node map");
    }
    let key = A::key(node_value);
    let value = A::value(node_value).clone();

    let existing_node = map.get_node(key);
    if let Some(existing_node) = existing_node {
        // if we already have a node
        let node_value = map.xot.value_mut(existing_node);
        A::update(node_value, value);
        existing_node
    } else {
        let insertion_point = A::insertion_point_as_before(map.xot, map.parent);
        if let Some(insertion_point) = insertion_point {
            map.xot
                .arena
                .checked_insert_after(insertion_point.get(), node.get())
                .unwrap();
        } else {
            map.xot
                .arena
                .checked_prepend(map.parent.get(), node.get())
                .unwrap();
        }
        node
    }
}

/// `Xot::append_namespace_node` and `Xot::append_attribute_node` (which
/// call `insert_node`) with `insert_node_as_before`.
fn append_map_node_as_before(xot: &mut Xot, parent: Node, child: Node, namespace: bool) -> Node {
    if !xot.is_element(parent) {
        panic!("not an element");
    }
    if namespace {
        if !xot.is_namespace_node(child) {
            panic!("not a namespace node");
        }
        insert_node_as_before(&mut xot.namespaces_mut(parent), child)
    } else {
        if !xot.is_attribute_node(child) {
            panic!("not an attribute node");
        }
        insert_node_as_before(&mut xot.attributes_mut(parent), child)
    }
}

/// `append_map_node_as_before` with the crate's code.
fn append_map_node(xot: &mut Xot, parent: Node, child: Node, namespace: bool) -> Node {
    let appended = if namespace {
        xot.append_namespace_node(parent, child)
    } else {
        xot.append_attribute_node(parent, child)
    };
    appended.unwrap_or_else(|_| {
        panic!(
            "{}",
            if !xot.is_element(parent) {
                "not an element"
            } else if namespace {
                "not a namespace node"
            } else {
                "not an attribute node"
            }
        )
    })
}

// ------------------------------------------------- keys and what was seen

/// The keys to insert: those of the trees, and enough others that some ids
/// are 64 apart.
struct Keys {
    prefixes: Vec<PrefixId>,
    namespaces: Vec<NamespaceId>,
    names: Vec<NameId>,
}

impl Keys {
    fn new(xot: &mut Xot) -> Self {
        let (mut names, mut prefixes, namespaces) = Trees::names(xot);
        for i in 0..70 {
            prefixes.push(xot.add_prefix(&format!("k{i}")));
            names.push(xot.add_name(&format!("k{i}")));
        }
        Keys {
            prefixes,
            namespaces,
            names,
        }
    }

    /// Mostly a key of the trees, so that maps have it.
    fn prefix(&self, rng: &mut Rng) -> PrefixId {
        if rng.chance(60) {
            self.prefixes[rng.below(4)]
        } else {
            rng.pick(&self.prefixes).unwrap()
        }
    }

    fn name(&self, rng: &mut Rng) -> NameId {
        if rng.chance(60) {
            self.names[rng.below(5)]
        } else {
            rng.pick(&self.names).unwrap()
        }
    }

    fn namespace(&self, rng: &mut Rng) -> NamespaceId {
        rng.pick(&self.namespaces).unwrap()
    }
}

/// The trees of `seed`, the keys, and a generator for the comparisons. Some
/// elements of the trees get nodes in their maps, and then a node of
/// another kind after a node of a map, which ends the map there, or a node
/// of a map gets the key of another one.
fn prepare(seed: u64) -> (Trees, Keys, Rng) {
    let mut rng = Rng::new(seed);
    let mut trees = Trees::new(seed, 60);
    let keys = Keys::new(&mut trees.xot);
    for _ in 0..6 {
        let xot = &mut trees.xot;
        let elements: Vec<Node> = trees
            .live
            .iter()
            .copied()
            .filter(|&node| xot.is_element(node))
            .collect();
        let Some(element) = rng.pick(&elements) else {
            break;
        };
        let namespace = rng.chance(50);
        for _ in 0..1 + rng.below(4) {
            if namespace {
                let (prefix, ns) = (keys.prefix(&mut rng), keys.namespace(&mut rng));
                xot.namespaces_mut(element).insert(prefix, ns);
            } else {
                let name = keys.name(&mut rng);
                xot.attributes_mut(element).insert(name, "s".to_string());
            }
        }
        let nodes: Vec<Node> = if namespace {
            xot.namespaces(element).nodes().collect()
        } else {
            xot.attributes(element).nodes().collect()
        };
        let (a, b) = (rng.pick(&nodes).unwrap(), rng.pick(&nodes).unwrap());
        if rng.chance(50) {
            let new = xot.new_element(keys.names[0]);
            xot.insert_after(a, new).unwrap();
            trees.live.push(new);
        } else if a != b {
            let value = match (xot.value(a), xot.value(b)) {
                (Value::Namespace(a), Value::Namespace(b)) => Value::Namespace(Namespace {
                    prefix_id: a.prefix(),
                    namespace_id: b.namespace(),
                }),
                (Value::Attribute(a), Value::Attribute(b)) => Value::Attribute(Attribute {
                    name_id: a.name(),
                    value: b.value().to_string(),
                }),
                _ => unreachable!(),
            };
            *xot.value_mut(b) = value;
        }
    }
    (trees, keys, rng)
}

/// The element of a map: mostly an element of the trees, else what
/// `Trees::node` gives (removed nodes, nodes whose slot was taken again, and
/// nodes of any kind).
fn element(trees: &Trees, rng: &mut Rng) -> Node {
    if rng.chance(85) {
        let elements: Vec<Node> = trees
            .live
            .iter()
            .copied()
            .filter(|&node| trees.xot.is_element(node))
            .collect();
        if let Some(element) = rng.pick(&elements) {
            return element;
        }
    }
    trees.node(rng)
}

/// How often the comparisons got the cases they are about.
#[derive(Default, Debug)]
struct Seen {
    /// a panic, the same with both
    panics: usize,
    /// a key that the map has, which is updated
    updates: usize,
    /// a new node first among the children
    first: usize,
    /// a new node after the last node of the map
    after_map: usize,
    /// a new attribute after the last namespace node
    after_namespaces: usize,
    /// a map with a node of another kind after it, which ends it
    ended: usize,
    /// a map with two nodes of one key
    shared_keys: usize,
    /// pairs for `extend` with a key twice, or a key whose bit another key
    /// set before
    shared_bits: usize,
}

/// A summary of the children of `element` for `Seen`: whether the map
/// (namespace or attribute nodes) has a node of another kind after it, and
/// whether two of its nodes share a key.
fn shape(xot: &Xot, element: Node, namespace: bool) -> (bool, bool) {
    let values: Vec<&Value> = xot
        .all_children_values(element)
        .map(|(_, value)| value)
        .collect();
    let map: Vec<&Value> = values
        .iter()
        .copied()
        .skip_while(|value| !namespace && matches!(value, Value::Namespace(_)))
        .take_while(|value| {
            if namespace {
                matches!(value, Value::Namespace(_))
            } else {
                matches!(value, Value::Attribute(_))
            }
        })
        .collect();
    let more_of_kind = values.iter().any(|value| {
        if namespace {
            matches!(value, Value::Namespace(_))
        } else {
            matches!(value, Value::Attribute(_))
        }
    }) && values
        .iter()
        .filter(|value| {
            if namespace {
                matches!(value, Value::Namespace(_))
            } else {
                matches!(value, Value::Attribute(_))
            }
        })
        .count()
        > map.len();
    let mut shared = false;
    for (i, a) in map.iter().enumerate() {
        for b in &map[i + 1..] {
            shared |= match (a, b) {
                (Value::Namespace(a), Value::Namespace(b)) => a.prefix() == b.prefix(),
                (Value::Attribute(a), Value::Attribute(b)) => a.name() == b.name(),
                _ => false,
            };
        }
    }
    (more_of_kind, shared)
}

/// Where a new node went: first, after a node of the map, or after a
/// namespace node (for an attribute); `None` if no node was made.
fn placement(old_xot: &Xot, xot: &Xot, element: Node, namespace: bool) -> Option<usize> {
    let before: Vec<Node> = catch(|| old_xot.all_children(element).collect()).ok()?;
    let after: Vec<Node> = catch(|| xot.all_children(element).collect()).ok()?;
    if after.len() <= before.len() {
        return None;
    }
    let new = after.iter().position(|node| !before.contains(node))?;
    if new == 0 {
        return Some(0);
    }
    let previous = xot.value(after[new - 1]);
    Some(match (namespace, previous) {
        (false, Value::Namespace(_)) => 2,
        _ => 1,
    })
}

// ------------------------------------------------- the comparisons

/// Inserts into random maps of the trees of `seeds`, with the old code and
/// the new; panics at the first difference.
fn compare_inserts(seeds: std::ops::Range<u64>, seen: &mut Seen) {
    for seed in seeds {
        let (trees, keys, mut rng) = prepare(seed);
        for _ in 0..12 {
            let element = element(&trees, &mut rng);
            let namespace = rng.chance(50);
            let mut old = trees.xot.clone();
            let mut new = trees.xot.clone();
            let (old_result, new_result) = if namespace {
                let (prefix, ns) = (keys.prefix(&mut rng), keys.namespace(&mut rng));
                (
                    catch(|| insert_as_before(&mut old.namespaces_mut(element), prefix, ns))
                        .map(|r| format!("{r:?}")),
                    catch(|| new.namespaces_mut(element).insert(prefix, ns))
                        .map(|r| format!("{r:?}")),
                )
            } else {
                let name = keys.name(&mut rng);
                let value = format!("v{}", rng.below(3));
                let other = value.clone();
                (
                    catch(|| insert_as_before(&mut old.attributes_mut(element), name, value))
                        .map(|r| format!("{r:?}")),
                    catch(|| new.attributes_mut(element).insert(name, other))
                        .map(|r| format!("{r:?}")),
                )
            };
            let context = format!("seed {seed}, {element:?}, namespace {namespace}");
            assert_eq!(old_result, new_result, "{context}");
            if let Some(difference) = slot_difference(&old, &new) {
                panic!("{context}: {difference}");
            }
            tally(seen, &trees.xot, &new, element, namespace, &new_result);
        }
    }
}

/// `append_namespace_node` and `append_attribute_node` of random nodes to
/// random maps of the trees of `seeds`, with the old `insert_node` and the
/// new; panics at the first difference.
fn compare_insert_nodes(seeds: std::ops::Range<u64>, seen: &mut Seen) {
    for seed in seeds {
        let (trees, keys, mut rng) = prepare(seed);
        for _ in 0..12 {
            let element = element(&trees, &mut rng);
            let namespace = rng.chance(50);
            // a node of the trees, mostly of the kind of the map, or a new one
            let xot = &trees.xot;
            let of_kind: Vec<Node> = trees
                .live
                .iter()
                .copied()
                .filter(|&node| {
                    if namespace {
                        xot.is_namespace_node(node)
                    } else {
                        xot.is_attribute_node(node)
                    }
                })
                .collect();
            let mut base = trees.xot.clone();
            let child = match rng.below(10) {
                0..=3 => rng.pick(&of_kind).unwrap_or_else(|| trees.node(&mut rng)),
                4 => trees.node(&mut rng),
                _ if namespace => {
                    base.new_namespace_node(keys.prefix(&mut rng), keys.namespace(&mut rng))
                }
                _ => base.new_attribute_node(keys.name(&mut rng), format!("n{}", rng.below(3))),
            };
            let mut old = base.clone();
            let mut new = base.clone();
            let old_result =
                catch(|| append_map_node_as_before(&mut old, element, child, namespace));
            let new_result = catch(|| append_map_node(&mut new, element, child, namespace));
            let context = format!("seed {seed}, {element:?}, {child:?}, namespace {namespace}");
            assert_eq!(old_result, new_result, "{context}");
            if let Some(difference) = slot_difference(&old, &new) {
                panic!("{context}: {difference}");
            }
            let result = new_result.map(|node| format!("{:?}", node == child));
            tally(seen, &base, &new, element, namespace, &result);
        }
    }
}

/// `extend` of random pairs into random maps of the trees of `seeds`,
/// against `insert` of the pairs one at a time, with the new code and with
/// the old; panics at the first difference.
fn compare_extends(seeds: std::ops::Range<u64>, seen: &mut Seen) {
    for seed in seeds {
        let (trees, keys, mut rng) = prepare(seed);
        for _ in 0..12 {
            let element = element(&trees, &mut rng);
            let namespace = rng.chance(50);
            let mut by_extend = trees.xot.clone();
            let mut one_by_one = trees.xot.clone();
            let mut as_before = trees.xot.clone();
            let count = rng.below(9);
            let context = format!("seed {seed}, {element:?}, namespace {namespace}");
            let results = if namespace {
                let pairs: Vec<(PrefixId, NamespaceId)> = (0..count)
                    .map(|_| (keys.prefix(&mut rng), keys.namespace(&mut rng)))
                    .collect();
                note_bits(seen, pairs.iter().map(|(prefix, _)| prefix.0));
                let given = pairs.clone();
                [
                    catch(|| by_extend.namespaces_mut(element).extend(given)),
                    catch(|| {
                        let mut map = one_by_one.namespaces_mut(element);
                        for &(prefix, ns) in &pairs {
                            map.insert(prefix, ns);
                        }
                    }),
                    catch(|| {
                        let mut map = as_before.namespaces_mut(element);
                        for &(prefix, ns) in &pairs {
                            insert_as_before(&mut map, prefix, ns);
                        }
                    }),
                ]
            } else {
                let pairs: Vec<(NameId, String)> = (0..count)
                    .map(|_| (keys.name(&mut rng), format!("e{}", rng.below(3))))
                    .collect();
                note_bits(seen, pairs.iter().map(|(name, _)| name.0));
                let given = pairs.clone();
                [
                    catch(|| by_extend.attributes_mut(element).extend(given)),
                    catch(|| {
                        let mut map = one_by_one.attributes_mut(element);
                        for (name, value) in &pairs {
                            map.insert(*name, value.clone());
                        }
                    }),
                    catch(|| {
                        let mut map = as_before.attributes_mut(element);
                        for (name, value) in &pairs {
                            insert_as_before(&mut map, *name, value.clone());
                        }
                    }),
                ]
            };
            assert_eq!(results[0], results[1], "{context}");
            assert_eq!(results[0], results[2], "{context}");
            if let Some(difference) = slot_difference(&by_extend, &one_by_one) {
                panic!("{context}: extend against insert: {difference}");
            }
            if let Some(difference) = slot_difference(&by_extend, &as_before) {
                panic!("{context}: extend against the old insert: {difference}");
            }
            let result = results[0]
                .as_ref()
                .map(|_| String::new())
                .map_err(|e| e.clone());
            tally(seen, &trees.xot, &by_extend, element, namespace, &result);
        }
    }
}

/// Count a key twice among the pairs, or two keys with one bit.
fn note_bits(seen: &mut Seen, ids: impl Iterator<Item = u16>) {
    let ids: Vec<u16> = ids.collect();
    let shared = ids
        .iter()
        .enumerate()
        .any(|(i, a)| ids[i + 1..].iter().any(|b| a % 64 == b % 64));
    seen.shared_bits += usize::from(shared);
}

/// Count what a comparison got.
fn tally(
    seen: &mut Seen,
    before: &Xot,
    after: &Xot,
    element: Node,
    namespace: bool,
    result: &Result<String, String>,
) {
    if result.is_err() {
        seen.panics += 1;
        return;
    }
    if let Ok(true) = catch(|| before.is_element(element)) {
        let (ended, shared) = shape(before, element, namespace);
        seen.ended += usize::from(ended);
        seen.shared_keys += usize::from(shared);
    }
    match placement(before, after, element, namespace) {
        Some(0) => seen.first += 1,
        Some(1) => seen.after_map += 1,
        Some(_) => seen.after_namespaces += 1,
        None => seen.updates += 1,
    }
}

fn check_seen(seen: &Seen) {
    let Seen {
        panics,
        updates,
        first,
        after_map,
        after_namespaces,
        ended,
        shared_keys,
        shared_bits,
    } = *seen;
    for count in [
        panics,
        updates,
        first,
        after_map,
        after_namespaces,
        ended,
        shared_keys,
    ] {
        assert!(count > 50, "{seen:?}");
    }
    assert!(shared_bits > 50 || shared_bits == 0, "{seen:?}");
}

#[test]
fn insert_finds_the_key_and_the_insertion_point_as_before() {
    let mut seen = Seen::default();
    compare_inserts(0..1000, &mut seen);
    check_seen(&seen);
}

#[test]
fn insert_node_finds_the_key_and_the_insertion_point_as_before() {
    let mut seen = Seen::default();
    compare_insert_nodes(3000..4000, &mut seen);
    check_seen(&seen);
}

#[test]
fn extend_inserts_as_insert_does_one_pair_at_a_time() {
    let mut seen = Seen::default();
    compare_extends(6000..7000, &mut seen);
    check_seen(&seen);
    assert!(seen.shared_bits > 50, "{seen:?}");
}

/// The soak: thirty times as many trees. Run it with `cargo test --release
/// --lib nodemap -- --ignored --nocapture`.
#[test]
#[ignore]
fn inserts_soak() {
    let mut seen = Seen::default();
    compare_inserts(1_000_000..1_030_000, &mut seen);
    compare_insert_nodes(2_000_000..2_030_000, &mut seen);
    compare_extends(3_000_000..3_030_000, &mut seen);
    println!("{seen:?}");
}

// ------------------------------------------------- examples

/// The namespace and attribute nodes of `element` with their keys, and
/// `*` for any other child.
fn map_children(xot: &Xot, element: Node) -> Vec<String> {
    xot.all_children(element)
        .map(|child| match xot.value(child) {
            Value::Namespace(namespace) => format!(
                "xmlns:{}={}",
                xot.prefix_str(namespace.prefix()),
                xot.namespace_str(namespace.namespace())
            ),
            Value::Attribute(attribute) => format!(
                "{}={}",
                xot.local_name_str(attribute.name()),
                attribute.value()
            ),
            _ => "*".to_string(),
        })
        .collect()
}

#[test]
fn extend_updates_the_keys_the_map_has_in_place_and_adds_the_others_in_order() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns:a="urn:1" xmlns:b="urn:2" x="1"><c/></doc>"#)
        .unwrap();
    let doc = xot.document_element(root).unwrap();
    let (a, b, c, d) = (
        xot.add_prefix("a"),
        xot.add_prefix("b"),
        xot.add_prefix("c"),
        xot.add_prefix("d"),
    );
    let (one, two, three) = (
        xot.add_namespace("urn:1"),
        xot.add_namespace("urn:2"),
        xot.add_namespace("urn:3"),
    );
    xot.namespaces_mut(doc)
        .extend([(d, one), (b, three), (c, two), (d, three), (a, two)]);
    assert_eq!(
        map_children(&xot, doc),
        [
            "xmlns:a=urn:2",
            "xmlns:b=urn:3",
            "xmlns:d=urn:3",
            "xmlns:c=urn:2",
            "x=1",
            "*"
        ]
    );
    let (x, y, z) = (xot.add_name("x"), xot.add_name("y"), xot.add_name("z"));
    xot.attributes_mut(doc).extend([
        (z, "z".to_string()),
        (x, "x".to_string()),
        (y, "y".to_string()),
        (z, "w".to_string()),
    ]);
    assert_eq!(
        map_children(&xot, doc),
        [
            "xmlns:a=urn:2",
            "xmlns:b=urn:3",
            "xmlns:d=urn:3",
            "xmlns:c=urn:2",
            "x=x",
            "z=w",
            "y=y",
            "*"
        ]
    );
    assert_eq!(
        xot.to_string(root).unwrap(),
        concat!(
            r#"<doc xmlns:a="urn:2" xmlns:b="urn:3" xmlns:d="urn:3" xmlns:c="urn:2" "#,
            r#"x="x" z="w" y="y"><c/></doc>"#
        )
    );
}

#[test]
fn extend_adds_attributes_after_the_namespace_nodes_or_first() {
    let mut xot = Xot::new();
    let root = xot.parse(r#"<doc xmlns:a="urn:1"><e>t</e></doc>"#).unwrap();
    let doc = xot.document_element(root).unwrap();
    let e = xot.first_child(doc).unwrap();
    let (x, y) = (xot.add_name("x"), xot.add_name("y"));
    xot.attributes_mut(doc)
        .extend([(x, "1".to_string()), (y, "2".to_string())]);
    xot.attributes_mut(e)
        .extend([(y, "3".to_string()), (x, "4".to_string())]);
    assert_eq!(
        map_children(&xot, doc),
        ["xmlns:a=urn:1", "x=1", "y=2", "*"]
    );
    assert_eq!(map_children(&xot, e), ["y=3", "x=4", "*"]);
    // nothing to add changes nothing
    let before = xot.clone();
    xot.attributes_mut(e).extend([]);
    xot.namespaces_mut(e).extend([]);
    assert_eq!(slot_difference(&before, &xot), None);
}

#[test]
fn extend_puts_new_nodes_where_insert_does_when_another_node_ends_the_map() {
    // a namespace node after an element, which ends the map: the new nodes go
    // after the first namespace node, and a key that only the namespace node
    // after the element has is added again, as insert does
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns:a="urn:1" xmlns:b="urn:2"/>"#)
        .unwrap();
    let doc = xot.document_element(root).unwrap();
    let first = xot.namespaces(doc).nodes().next().unwrap();
    let e_name = xot.add_name("e");
    let e = xot.new_element(e_name);
    xot.insert_after(first, e).unwrap();
    let (b, c) = (xot.add_prefix("b"), xot.add_prefix("c"));
    let two = xot.add_namespace("urn:2");
    assert_eq!(
        map_children(&xot, doc),
        ["xmlns:a=urn:1", "*", "xmlns:b=urn:2"]
    );
    let mut by_insert = xot.clone();
    xot.namespaces_mut(doc).extend([(c, two), (b, two)]);
    {
        let mut namespaces = by_insert.namespaces_mut(doc);
        namespaces.insert(c, two);
        namespaces.insert(b, two);
    }
    assert_eq!(
        map_children(&xot, doc),
        [
            "xmlns:a=urn:1",
            "xmlns:c=urn:2",
            "xmlns:b=urn:2",
            "*",
            "xmlns:b=urn:2"
        ]
    );
    assert_eq!(slot_difference(&by_insert, &xot), None);
}

#[test]
fn extend_looks_for_a_key_whose_bit_another_key_set() {
    // prefixes whose ids are 64 apart have one bit in the filter of extend
    let mut xot = Xot::new();
    let root = xot.parse(r#"<doc/>"#).unwrap();
    let doc = xot.document_element(root).unwrap();
    let prefixes: Vec<PrefixId> = (0..140).map(|i| xot.add_prefix(&format!("p{i}"))).collect();
    let ns = xot.add_namespace("urn:1");
    let other = xot.add_namespace("urn:2");
    let (low, high, top) = (prefixes[2], prefixes[66], prefixes[130]);
    assert_eq!(high.0, low.0 + 64);
    assert_eq!(top.0, low.0 + 128);
    let mut by_insert = xot.clone();
    let pairs = [
        (low, ns),
        (high, ns),
        (top, ns),
        (high, other),
        (low, other),
    ];
    xot.namespaces_mut(doc).extend(pairs);
    {
        let mut namespaces = by_insert.namespaces_mut(doc);
        for (prefix, namespace) in pairs {
            namespaces.insert(prefix, namespace);
        }
    }
    assert_eq!(slot_difference(&by_insert, &xot), None);
    assert_eq!(
        map_children(&xot, doc),
        ["xmlns:p2=urn:2", "xmlns:p66=urn:2", "xmlns:p130=urn:1"]
    );
}

#[test]
fn insert_and_insert_node_return_what_they_did() {
    let mut xot = Xot::new();
    let root = xot.parse(r#"<doc xmlns:a="urn:1" x="1"/>"#).unwrap();
    let doc = xot.document_element(root).unwrap();
    let (a, b) = (xot.add_prefix("a"), xot.add_prefix("b"));
    let (one, two) = (xot.add_namespace("urn:1"), xot.add_namespace("urn:2"));
    let x = xot.add_name("x");
    assert_eq!(xot.namespaces_mut(doc).insert(a, two), Some(one));
    assert_eq!(xot.namespaces_mut(doc).insert(b, one), None);
    assert_eq!(
        xot.attributes_mut(doc).insert(x, "2".to_string()),
        Some("1".to_string())
    );
    let node = xot.new_namespace_node(b, two);
    let updated = xot.append_namespace_node(doc, node).unwrap();
    assert_ne!(updated, node);
    assert_eq!(xot.namespaces(doc).get(b), Some(&two));
    let y = xot.add_name("y");
    let node = xot.new_attribute_node(y, "3".to_string());
    assert_eq!(xot.append_attribute_node(doc, node).unwrap(), node);
    assert_eq!(
        map_children(&xot, doc),
        ["xmlns:a=urn:2", "xmlns:b=urn:2", "x=2", "y=3"]
    );
}
