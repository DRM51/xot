// implements some of the HashMap API, based on nodes in the tree

use ahash::AHashMap;

use crate::arena::NodeId;
use crate::{Node, Value, Xot};

use super::entry::{Entry, OccupiedEntry, VacantEntry};

pub trait ValueAdapter<K, V> {
    fn matches(value: &Value) -> bool;
    // the nodes of the map with their values, in order
    fn entries(xot: &Xot, parent: Node) -> impl Iterator<Item = (Node, &Value)> + '_;
    #[inline]
    fn children(xot: &Xot, parent: Node) -> impl Iterator<Item = Node> + '_ {
        Self::entries(xot, parent).map(|(node, _)| node)
    }
    // in one pass over the children of `parent`, an element that is not
    // removed (see `MutableNodeMap::attach`): the first node of the map whose
    // value `found` accepts, `found` getting the values in order; otherwise
    // the insertion point of a new node of the map, the node it goes after,
    // or `None` if it goes first among the children
    fn find_or_end(
        xot: &Xot,
        parent: Node,
        found: impl FnMut(&Value) -> bool,
    ) -> Result<Node, Option<Node>>;
    fn key(value: &Value) -> K;
    // the bit of a key in a filter of 64 bits (see `key_bit`)
    fn key_bit(key: K) -> u64;
    fn value(value: &Value) -> &V;
    fn value_mut(value: &mut Value) -> &mut V;
    fn create(key: K, value: V) -> Value;
    fn update(value: &mut Value, value: V) -> Option<V>;
}

/// The bit of a prefix or name id in a filter of 64 bits. A filter with the
/// bits of a set of keys may have the bit of a key that is not in the set,
/// but a key whose bit it does not have is not in the set.
#[inline]
pub(crate) fn key_bit(id: u32) -> u64 {
    1 << (id % 64)
}

/// A `NodeMap` is a struct with a hash-map like API and is used to
/// expose attribute and namespace prefix information.
///
/// You obtain one through the APIs [`Xot::attributes`] and [`Xot::namespaces`].
#[derive(Debug)]
pub struct NodeMap<'a, K, V, A: ValueAdapter<K, V>>
where
    K: PartialEq + Eq + Clone + Copy,
    V: Clone,
{
    xot: &'a Xot,
    parent: Node,
    _k: std::marker::PhantomData<K>,
    _v: std::marker::PhantomData<V>,
    _a: std::marker::PhantomData<A>,
}

impl<'a, K, V, A: ValueAdapter<K, V>> NodeMap<'a, K, V, A>
where
    K: PartialEq + Eq + Clone + Copy + std::hash::Hash,
    V: Clone + 'a,
{
    pub(crate) fn new(xot: &'a Xot, parent: Node) -> Self {
        Self {
            xot,
            parent,
            _k: std::marker::PhantomData,
            _v: std::marker::PhantomData,
            _a: std::marker::PhantomData,
        }
    }

    fn children(&self) -> impl Iterator<Item = Node> + '_ {
        A::children(self.xot, self.parent)
    }

    fn entries(&self) -> impl Iterator<Item = (Node, &'a Value)> + '_ {
        A::entries(self.xot, self.parent)
    }

    /// Get the node representing the value in the map.
    ///
    /// This is an attribute or namespace node. This node has the
    /// element node as a parent, even though it's not in `xot.children(parent)`.
    pub fn get_node(&self, key: impl Into<K> + Copy) -> Option<Node> {
        self.entries()
            .find(|(_, value)| A::key(value) == key.into())
            .map(|(node, _)| node)
    }

    /// Returns the number of entries in the map, also referred to as its 'length'.
    pub fn len(&self) -> usize {
        self.children().count()
    }

    /// Returns `true` if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.children().next().is_none()
    }

    // TODO: retain, drain, sort_keys, sort_unstable_keys, sort_by, sort_unstable_by,

    /// Return `true` if an equivalent to `key` exists in the map.
    pub fn contains_key(&self, key: impl Into<K> + Copy) -> bool {
        for (_, value) in self.entries() {
            if A::key(value) == key.into() {
                return true;
            }
        }
        false
    }

    /// Return a reference to the value stored for `key`, if it is present, else `None`.
    pub fn get(&self, key: impl Into<K> + Copy) -> Option<&'a V> {
        self.entries()
            .find(|(_, value)| A::key(value) == key.into())
            .map(|(_, value)| A::value(value))
    }

    fn iter_value(&self) -> impl Iterator<Item = &'a Value> + '_ {
        self.entries().map(|(_, value)| value)
    }

    /// An iterator visiting all key-value pairs in insertion order. The iterator element type is
    /// `(&'a K, &'a V)`.
    pub fn iter(&self) -> impl Iterator<Item = (K, &'a V)> + '_ {
        self.iter_value()
            .map(|value| (A::key(value), A::value(value)))
    }

    /// Copies the map entries into a new `Vec<(K, V)>`.
    pub fn to_vec(&self) -> Vec<(K, V)> {
        self.iter().map(|(k, v)| (k, v.clone())).collect()
    }

    /// An iterator visiting all keys in insertion order. The iterator element type is `&'a K`.
    pub fn keys(&self) -> impl Iterator<Item = K> + '_ {
        self.iter_value().map(move |value| A::key(value))
    }

    /// An iterator visiting all values in insertion order. The iterator element type is `&'a V`.
    pub fn values(&self) -> impl Iterator<Item = &'a V> + '_ {
        self.iter_value().map(move |value| A::value(value))
    }

    /// An iterator visiting all the nodes in insertion order.
    pub fn nodes(&self) -> impl Iterator<Item = Node> + '_ {
        self.children()
    }

    /// Convert into a hashmap
    pub fn to_hashmap(&self) -> AHashMap<K, V> {
        let mut m = AHashMap::new();
        for (key, value) in self.iter() {
            m.insert(key, value.clone());
        }
        m
    }
}

/// A `MutableNodeMap` is a struct with a hash-map like API and is used to
/// expose attribute and namespace prefix information in a mutable way.
///
/// You obtain one through the APIs [`Xot::attributes_mut`] and
/// [`Xot::namespaces_mut`].
///
#[derive(Debug)]
pub struct MutableNodeMap<'a, K, V, A: ValueAdapter<K, V>>
where
    K: PartialEq + Eq + Clone + Copy,
    V: Clone,
{
    xot: &'a mut Xot,
    parent: Node,
    _k: std::marker::PhantomData<K>,
    _v: std::marker::PhantomData<V>,
    _a: std::marker::PhantomData<A>,
}

impl<'a, K, V, A: ValueAdapter<K, V>> MutableNodeMap<'a, K, V, A>
where
    K: PartialEq + Eq + Clone + Copy + std::hash::Hash,
    V: Clone,
{
    pub(crate) fn new(xot: &'a mut Xot, parent: Node) -> Self {
        MutableNodeMap {
            xot,
            parent,
            _k: std::marker::PhantomData,
            _v: std::marker::PhantomData,
            _a: std::marker::PhantomData,
        }
    }

    // TODO argh duplication
    fn children(&self) -> impl Iterator<Item = Node> + '_ {
        A::children(self.xot, self.parent)
    }

    fn entries(&self) -> impl Iterator<Item = (Node, &Value)> + '_ {
        A::entries(self.xot, self.parent)
    }

    /// Get the node representing the value in the map.
    ///
    /// This is an attribute or namespace node. This node has the
    /// element node as a parent, even though it's not in `xot.children(parent)`.
    pub fn get_node(&self, key: impl Into<K> + Copy) -> Option<Node> {
        self.entries()
            .find(|(_, value)| A::key(value) == key.into())
            .map(|(node, _)| node)
    }

    /// Returns the number of entries in the map, also referred to as its 'length'.
    pub fn len(&self) -> usize {
        self.children().count()
    }

    /// Returns `true` if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.children().next().is_some()
    }

    // TODO: retain, drain, sort_keys, sort_unstable_keys, sort_by, sort_unstable_by,

    /// Return `true` if an equivalent to `key` exists in the map.
    pub fn contains_key(&self, key: impl Into<K> + Copy) -> bool {
        for (_, value) in self.entries() {
            if A::key(value) == key.into() {
                return true;
            }
        }
        false
    }

    /// Return a reference to the value stored for `key`, if it is present, else `None`.
    pub fn get(&self, key: impl Into<K> + Copy) -> Option<&V> {
        self.entries()
            .find(|(_, value)| A::key(value) == key.into())
            .map(|(_, value)| A::value(value))
    }

    fn iter_value(&'a self) -> impl Iterator<Item = &'a Value> + 'a {
        self.entries().map(|(_, value)| value)
    }

    /// An iterator visiting all key-value pairs in insertion order. The iterator element type is
    /// `(&'a K, &'a V)`.
    pub fn iter(&'a self) -> impl Iterator<Item = (K, &'a V)> + 'a {
        self.iter_value()
            .map(move |value| (A::key(value), A::value(value)))
    }

    /// Copies the map entries into a new `Vec<(K, V)>`.
    pub fn to_vec(&self) -> Vec<(K, V)> {
        self.iter().map(|(k, v)| (k, v.clone())).collect()
    }

    /// An iterator visiting all keys in insertion order. The iterator element type is `&'a K`.
    pub fn keys(&'a self) -> impl Iterator<Item = K> + 'a {
        self.iter_value().map(move |value| A::key(value))
    }

    /// An iterator visiting all values in insertion order. The iterator element type is `&'a V`.
    pub fn values(&'a self) -> impl Iterator<Item = &'a V> + 'a {
        self.iter_value().map(move |value| A::value(value))
    }

    /// An iterator visiting all the nodes in insertion order.
    pub fn nodes(&self) -> impl Iterator<Item = Node> + '_ {
        self.children()
    }

    /// Convert into a hashmap.
    pub fn to_hashmap(&self) -> AHashMap<K, V> {
        let mut m = AHashMap::new();
        for (key, value) in self.iter() {
            m.insert(key, value.clone());
        }
        m
    }

    // TODO: end of duplication

    /// Return a mutable reference to the value stored for `key`, if it is present, else `None`.
    pub fn get_mut(&mut self, key: impl Into<K> + Copy) -> Option<&mut V> {
        let node = self.get_node(key)?;
        Some(A::value_mut(self.xot.value_mut(node)))
    }

    /// Clears the map, removing all entries.
    pub fn clear(&mut self) {
        let to_remove = self.children().collect::<Vec<_>>();

        for child in to_remove {
            self.xot.remove(child).unwrap();
        }
    }

    /// Insert a key-value pair in the map.
    ///
    /// If an equivalent key already exists in the map: the key remains and retains in its place
    /// in the order, its corresponding value is updated with `value` and the older value is
    /// returned inside `Some(_)`.
    ///
    /// If no equivalent key existed in the map: the new key-value pair is inserted, last in
    /// order, and `None` is returned.
    ///
    /// See also [`entry`](#method.entry) if you you want to insert *or* modify or if you need to
    /// get the index of the corresponding key-value pair.
    pub fn insert(&mut self, key: impl Into<K> + Copy, value: V) -> Option<V> {
        let key = key.into();
        match A::find_or_end(self.xot, self.parent, |value| A::key(value) == key) {
            // if we already have a node
            Ok(node) => A::update(self.xot.value_mut(node), value),
            // we need to insert a new node
            Err(insertion_point) => {
                let node = self.xot.arena.new_node(A::create(key, value));
                self.attach(insertion_point, node);
                None
            }
        }
    }

    /// Insert a node into the map. This node has to be of the right type,
    /// so a namespace node for a namespaces map, and an attribute node
    /// for an attribute map, if not, this function will panic.
    ///
    /// If an equivalent key already exists in the map: existing node is updated
    /// with the new value, and returned.
    ///
    /// If no equivalent key existed in the map: the new node is inserted, and
    /// returned as the inserted node.
    pub(crate) fn insert_node(&mut self, node: Node) -> Node {
        let node_value = self.xot.value(node);
        if !A::matches(node_value) {
            panic!("Tried to insert unexpected node value into the node map");
        }
        let key = A::key(node_value);

        match A::find_or_end(self.xot, self.parent, |value| A::key(value) == key) {
            // if we already have a node, it gets the value of `node`
            Ok(existing_node) => {
                let value = A::value(self.xot.value(node)).clone();
                A::update(self.xot.value_mut(existing_node), value);
                existing_node
            }
            Err(insertion_point) => {
                self.attach(insertion_point, node.get());
                node
            }
        }
    }

    /// Put `node`, which has a key that the map does not have, at the
    /// insertion point that [`ValueAdapter::find_or_end`] gave: after
    /// `insertion_point`, or first among the children if it is `None`.
    ///
    /// The slot of the parent holds an element, which is not removed:
    /// [`Xot::namespaces_mut`] and [`Xot::attributes_mut`] check that it is
    /// an element, which reads its value and panics for a removed node, and
    /// the map removes no node but those of the map. So its children are not
    /// removed either, and `Arena::new_node`, which takes a free slot, changes
    /// none of the nodes that `find_or_end` read: the insertion point it gave
    /// before a new node was made is the one it gives after.
    fn attach(&mut self, insertion_point: Option<Node>, node: NodeId) {
        if let Some(insertion_point) = insertion_point {
            self.xot
                .arena
                .checked_insert_after(insertion_point.get(), node)
                .unwrap();
        } else {
            self.xot
                .arena
                .checked_prepend(self.parent.get(), node)
                .unwrap();
        }
    }

    /// Remove a key-value pair from the map, if it exists.
    ///
    /// Returns the value corresponding to the key if the key was previously in the map.
    pub fn remove(&mut self, key: impl Into<K> + Copy) -> Option<V> {
        let node = self.get_node(key);
        if let Some(node) = node {
            let value = A::value(self.xot.value(node)).clone();
            self.xot.remove(node).unwrap();
            Some(value)
        } else {
            None
        }
    }

    /// Get the given key's corresponding entry in the map for insertion and/or in-place
    /// manipulation.
    pub fn entry(&'a mut self, key: impl Into<K> + Copy) -> Entry<'a, K, V, A> {
        match self.get(key) {
            Some(_value) => Entry::Occupied(OccupiedEntry::new(self, key.into())),
            None => Entry::Vacant(VacantEntry::new(self, key.into())),
        }
    }
}

impl<K, V, A: ValueAdapter<K, V>> Extend<(K, V)> for MutableNodeMap<'_, K, V, A>
where
    K: PartialEq + Eq + Clone + Copy + std::hash::Hash,
    V: Clone,
{
    /// Insert the key-value pairs in order, as
    /// [`insert`](MutableNodeMap::insert) does each in turn: a key that the
    /// map already has, or that an earlier pair gave, keeps its place in the
    /// order and gets the new value; any other key is inserted last.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse(r#"<doc xmlns:a="urn:a"/>"#)?;
    /// let doc_el = xot.document_element(root)?;
    /// let (a, b, c) = (xot.add_prefix("a"), xot.add_prefix("b"), xot.add_prefix("c"));
    /// let (one, two, three) = (
    ///     xot.add_namespace("urn:1"),
    ///     xot.add_namespace("urn:2"),
    ///     xot.add_namespace("urn:3"),
    /// );
    /// xot.namespaces_mut(doc_el)
    ///     .extend([(b, one), (a, two), (c, three), (b, two)]);
    /// assert_eq!(
    ///     xot.to_string(root)?,
    ///     r#"<doc xmlns:a="urn:2" xmlns:b="urn:2" xmlns:c="urn:3"/>"#
    /// );
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// Where `insert` looks through the nodes of the map for each pair,
    /// `extend` looks through them once, and again only for a key that may
    /// be in the map: it keeps a bit for each key of the map (the id of the
    /// key modulo 64), and looks for a key whose bit is set as `insert` does.
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, pairs: I) {
        // the bits of the keys in the map, and the insertion point
        let mut keys = 0u64;
        let Err(mut insertion_point) = A::find_or_end(self.xot, self.parent, |value| {
            keys |= A::key_bit(A::key(value));
            false
        }) else {
            unreachable!("no node is looked for");
        };
        for (key, value) in pairs {
            let bit = A::key_bit(key);
            if keys & bit != 0 {
                // the map may have the key
                match A::find_or_end(self.xot, self.parent, |value| A::key(value) == key) {
                    Ok(node) => {
                        A::update(self.xot.value_mut(node), value);
                        continue;
                    }
                    Err(end) => debug_assert_eq!(end, insertion_point),
                }
            }
            // a new node goes after the last node of the map, which is the
            // node inserted last, if any
            keys |= bit;
            let node = self.xot.arena.new_node(A::create(key, value));
            self.attach(insertion_point, node);
            insertion_point = Some(Node::new(node));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Xot;

    #[test]
    fn test_attribute_get() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc a="A"></doc>"#);
        let a = xot.add_name("a");
        let document_element = xot.document_element(root.unwrap()).unwrap();
        let attributes = xot.attributes(document_element);
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
    }

    #[test]
    fn test_attribute_insert() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc a="A"></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let mut attributes = xot.attributes_mut(document_element);
        attributes.insert(a, "B".to_string());
        assert_eq!(attributes.get(a), Some(&"B".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc a="B"/>"#);
    }

    #[test]
    fn test_attribute_insert_node() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc a="A"></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let a_node = xot.new_attribute_node(a, "B".to_string());
        let mut attributes = xot.attributes_mut(document_element);
        let inserted_node = attributes.insert_node(a_node);
        assert_ne!(inserted_node, a_node);
        assert_eq!(attributes.get(a), Some(&"B".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc a="B"/>"#);
    }

    #[test]
    fn test_attribute_insert_new_blank() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let mut attributes = xot.attributes_mut(document_element);
        attributes.insert(a, "A".to_string());
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc a="A"/>"#);
    }

    #[test]
    fn test_attribute_insert_node_new_blank() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let a_node = xot.new_attribute_node(a, "A".to_string());
        let mut attributes = xot.attributes_mut(document_element);
        let inserted_node = attributes.insert_node(a_node);
        assert_eq!(inserted_node, a_node);
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc a="A"/>"#);
    }

    #[test]
    fn test_attribute_insert_new_existing_attributes() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc c="C"></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let mut attributes = xot.attributes_mut(document_element);
        attributes.insert(a, "A".to_string());
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc c="C" a="A"/>"#);
    }

    #[test]
    fn test_attribute_insert_node_new_existing_attributes() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc c="C"></doc>"#).unwrap();
        let a = xot.add_name("a");
        let document_element = xot.document_element(root).unwrap();
        let a_node = xot.new_attribute_node(a, "A".to_string());
        let mut attributes = xot.attributes_mut(document_element);
        let inserted_node = attributes.insert_node(a_node);
        assert_eq!(inserted_node, a_node);
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
        assert_eq!(xot.to_string(root).unwrap(), r#"<doc c="C" a="A"/>"#);
    }

    #[test]
    fn test_attribute_entry_modify() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc c="C"></doc>"#);
        let c = xot.add_name("c");
        let document_element = xot.document_element(root.unwrap()).unwrap();
        let mut attributes = xot.attributes_mut(document_element);
        attributes
            .entry(c)
            .and_modify(|e| *e = "C!".to_string())
            .or_insert("New".to_string());
        let attributes = xot.attributes(document_element);
        assert_eq!(attributes.get(c), Some(&"C!".to_string()));
    }

    #[test]
    fn test_attribute_entry_create() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc></doc>"#);
        let c = xot.add_name("c");
        let document_element = xot.document_element(root.unwrap()).unwrap();
        let mut attributes = xot.attributes_mut(document_element);
        attributes
            .entry(c)
            .and_modify(|e| *e = "C!".to_string())
            .or_insert("New".to_string());
        let attributes = xot.attributes(document_element);
        assert_eq!(attributes.get(c), Some(&"New".to_string()));
    }

    #[test]
    fn test_attributes_and_namespaces() {
        let mut xot = Xot::new();

        let root = xot.parse(r#"<doc xmlns:foo="FOO" a="A"></doc>"#);
        let a = xot.add_name("a");
        let foo_prefix = xot.add_prefix("foo");
        let foo_ns = xot.add_namespace("FOO");
        let document_element = xot.document_element(root.unwrap()).unwrap();
        let attributes = xot.attributes(document_element);
        assert_eq!(attributes.get(a), Some(&"A".to_string()));
        let namespaces = xot.namespaces(document_element);
        assert_eq!(namespaces.get(foo_prefix), Some(&foo_ns));
    }
}

#[cfg(test)]
mod insert_tests;
