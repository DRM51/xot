//! The namespaces in scope of the XML and HTML 5 writers: one list of
//! declarations that changes as elements open and close, instead of a copied
//! list for each element that declares namespaces. It picks the same
//! prefixes as `FullnameSerializer`.
//!
//! `FullnameSerializer` keeps, for each open element that declares
//! namespaces, the list of the declarations in scope: the list of the parent
//! without the entries whose prefix this element declares, followed by the
//! declarations of this element (two declarations of one prefix on one
//! element both stay). `NameScope` keeps only the list of the innermost
//! such element. When an element declares namespaces, it appends the
//! declarations, then takes the entries before them whose prefix the
//! element declares out of the list in one pass, noting each with its
//! place; at the end of the element it cuts the declarations off and puts
//! the noted entries back in one pass from the end. A lookup reads the list
//! from the last entry to the first, as a copied list is read.
//!
//! The list holds only entries in scope, so a lookup costs no more than in a
//! copied list, however deep the elements are. A count of the entries for
//! each prefix tells whether a declaration takes anything out of scope. An
//! element that declares only prefixes that are not in scope yet costs one
//! count update per declaration. An element that takes entries out costs,
//! besides that, one pass over the list from the first entry it takes out,
//! however many prefixes it declares again: at most the order of the
//! copy of the list that `FullnameSerializer` makes for it.

use std::collections::BTreeMap;

use crate::error::Error;
use crate::id::{NameId, NamespaceId, PrefixId};
use crate::xotdata::Xot;

/// Prefixes with a number below this are counted in a vector indexed by the
/// number; the others in a sorted map.
const DENSE_PREFIXES: u32 = 4096;

type Entry = (PrefixId, NamespaceId);

#[derive(Debug)]
pub(crate) struct NameScope {
    // the declarations in scope, in the order of the list of
    // `FullnameSerializer`
    entries: Vec<Entry>,
    // for each element that declares namespaces and is open: where its
    // declarations begin in `entries`, and where the entries it took out
    // begin in `taken`
    marks: Vec<(usize, usize)>,
    // the entries that the open elements took out of scope, each with the
    // place it had in `entries`, in the order they were taken out
    taken: Vec<(usize, Entry)>,
    // by prefix number, for numbers below DENSE_PREFIXES: the number of
    // entries for the prefix
    dense: Vec<u32>,
    // the same for the other prefixes, without the prefixes that have none
    wide: BTreeMap<u32, u32>,
    empty: PrefixId,
    no_namespace: NamespaceId,
    // whether the namespaces in scope of the node that is serialized were
    // popped, which only an end tag without its start tag does; see `pop`
    base_popped: bool,
}

const BASE_POPPED: &str = "called `Option::unwrap()` on a `None` value";

impl NameScope {
    /// A scope with `base`, the namespaces in scope of the node that is
    /// serialized, in their order.
    pub(crate) fn new(xot: &Xot, base: &[(PrefixId, NamespaceId)]) -> Self {
        let mut scope = NameScope {
            entries: Vec::with_capacity(base.len() + 8),
            marks: Vec::new(),
            taken: Vec::new(),
            dense: Vec::new(),
            wide: BTreeMap::new(),
            empty: xot.empty_prefix_id,
            no_namespace: xot.no_namespace_id,
            base_popped: false,
        };
        for &entry in base {
            scope.add(entry);
        }
        scope
    }

    /// The number of entries for `prefix`.
    #[inline]
    fn count(&self, prefix: PrefixId) -> u32 {
        let number = prefix.0;
        if number < DENSE_PREFIXES {
            self.dense.get(number as usize).copied().unwrap_or(0)
        } else {
            self.wide.get(&number).copied().unwrap_or(0)
        }
    }

    #[inline]
    fn set_count(&mut self, prefix: PrefixId, count: u32) {
        let number = prefix.0;
        if number < DENSE_PREFIXES {
            let index = number as usize;
            if index >= self.dense.len() {
                self.dense.resize(index + 1, 0);
            }
            self.dense[index] = count;
        } else if count == 0 {
            self.wide.remove(&number);
        } else {
            self.wide.insert(number, count);
        }
    }

    /// Append an entry.
    #[inline]
    fn add(&mut self, entry: Entry) {
        let count = self.count(entry.0);
        self.set_count(entry.0, count + 1);
        self.entries.push(entry);
    }

    /// Add the declarations of an element, in their order, after taking the
    /// entries out of scope whose prefix one of them declares.
    ///
    /// Returns whether there were any: only then is there something to
    /// [`pop`](NameScope::pop) at the end of the element.
    #[inline]
    pub(crate) fn push(
        &mut self,
        declarations: impl Iterator<Item = (PrefixId, NamespaceId)>,
    ) -> bool {
        let start = self.entries.len();
        // the number of entries before this element whose prefix it declares
        let mut shadowed = 0usize;
        for (prefix, namespace) in declarations {
            if self.entries.len() == start && self.base_popped {
                panic!("{}", BASE_POPPED);
            }
            // Until the counts are updated below, they count only the
            // entries before this element. The count of a prefix that is in
            // scope goes to 0 here, so that a second declaration of it does
            // not count those entries again, and so that `take_out` can tell
            // them: every other entry before `start` has a count above 0.
            let count = self.count(prefix);
            if count > 0 {
                shadowed += count as usize;
                self.set_count(prefix, 0);
            }
            self.entries.push((prefix, namespace));
        }
        if self.entries.len() == start {
            return false;
        }
        let taken = self.taken.len();
        let own = if shadowed > 0 {
            self.take_out(start, shadowed)
        } else {
            start
        };
        for index in own..self.entries.len() {
            let prefix = self.entries[index].0;
            let count = self.count(prefix);
            self.set_count(prefix, count + 1);
        }
        self.marks.push((own, taken));
        true
    }

    /// Take the `shadowed` entries before `start` whose prefix has a count of
    /// 0 out of the list, noting each with its place in `taken`, in the order
    /// of the list. Returns where the entries from `start` on begin then.
    ///
    /// It looks from `start` to the front for the first of them, since a
    /// prefix declared again was most often declared by a near ancestor, and
    /// then moves the entries from there on in one pass.
    fn take_out(&mut self, start: usize, shadowed: usize) -> usize {
        let mut left = shadowed;
        let mut first = start;
        while left > 0 {
            first -= 1;
            if self.count(self.entries[first].0) == 0 {
                left -= 1;
            }
        }
        let mut kept = first;
        for index in first..start {
            let entry = self.entries[index];
            if self.count(entry.0) == 0 {
                self.taken.push((index, entry));
            } else {
                self.entries[kept] = entry;
                kept += 1;
            }
        }
        self.entries.drain(kept..start);
        kept
    }

    /// Remove the declarations that the last [`push`](NameScope::push) that
    /// returned `true` added, with the entries that were added to them since,
    /// and put back the entries it took out of scope.
    ///
    /// The walk of a node that was changed in an unusual way can give the end
    /// tag of an element whose start tag it did not give, when it goes on
    /// after the node that is serialized. `FullnameSerializer` then removes
    /// its namespaces in scope, so that later names in a namespace and later
    /// declarations make it panic; this does the same, with the same message.
    #[inline]
    pub(crate) fn pop(&mut self) {
        let Some((start, taken)) = self.marks.pop() else {
            self.base_popped = true;
            return;
        };
        while self.entries.len() > start {
            let (prefix, _) = self.entries.pop().unwrap();
            let count = self.count(prefix);
            self.set_count(prefix, count - 1);
        }
        if self.taken.len() > taken {
            self.put_back(taken);
        }
    }

    /// Put the entries of `taken` from `taken` on back in their places, from
    /// the last to the first, moving each entry between them once.
    fn put_back(&mut self, taken: usize) {
        let mut read = self.entries.len();
        let mut end = read + (self.taken.len() - taken);
        self.entries.resize(end, (self.empty, self.no_namespace));
        while self.taken.len() > taken {
            let (index, entry) = self.taken.pop().unwrap();
            // the entries that go between this one and the one put back
            // before it
            let after = end - index - 1;
            self.entries.copy_within(read - after..read, index + 1);
            read -= after;
            self.entries[index] = entry;
            end = index;
            let count = self.count(entry.0);
            self.set_count(entry.0, count + 1);
        }
        debug_assert_eq!(read, end);
    }

    /// Add the empty prefix for `namespace` after the last entry, as
    /// `FullnameSerializer::add_empty_prefix` adds it to the last list. The
    /// HTML 5 writer uses this for a namespace that it writes as the default
    /// namespace without a declaration in the tree.
    ///
    /// The entry takes nothing out of scope. A later element that declares
    /// the empty prefix takes it out of scope, as that element leaves it out
    /// of its list. It goes away with the declarations of the element that
    /// the last [`push`](NameScope::push) that returned `true` was for, or
    /// stays with the base when there is none.
    #[inline]
    pub(crate) fn add_unprefixed(&mut self, namespace: NamespaceId) {
        if self.base_popped {
            panic!("{}", BASE_POPPED);
        }
        self.add((self.empty, namespace));
    }

    /// Whether the empty prefix is in scope for `namespace`, as
    /// `FullnameSerializer::has_empty_prefix` says.
    #[inline]
    pub(crate) fn has_unprefixed(&self, namespace: NamespaceId) -> bool {
        if self.base_popped {
            panic!("{}", BASE_POPPED);
        }
        self.entries.contains(&(self.empty, namespace))
    }

    /// The prefix to write an element name with: `None` for a name in no
    /// namespace, and for a namespace that the empty prefix is declared for.
    /// Otherwise the latest declared prefix.
    #[inline]
    pub(crate) fn element_prefix(
        &self,
        xot: &Xot,
        name: NameId,
    ) -> Result<Option<PrefixId>, Error> {
        let namespace = xot.namespace_for_name(name);
        if namespace == self.no_namespace {
            return Ok(None);
        }
        if self.base_popped {
            panic!("{}", BASE_POPPED);
        }
        let mut found = None;
        for &(prefix, entry_namespace) in self.entries.iter().rev() {
            if entry_namespace != namespace {
                continue;
            }
            if prefix == self.empty {
                return Ok(None);
            }
            if found.is_none() {
                found = Some(prefix);
            }
        }
        match found {
            Some(prefix) => Ok(Some(prefix)),
            None => Err(Error::MissingPrefix(
                xot.namespace_str(namespace).to_string(),
            )),
        }
    }

    /// The prefix to write an attribute name with: `None` for a name in no
    /// namespace, otherwise the latest declared prefix that is not empty.
    #[inline]
    pub(crate) fn attribute_prefix(
        &self,
        xot: &Xot,
        name: NameId,
    ) -> Result<Option<PrefixId>, Error> {
        let namespace = xot.namespace_for_name(name);
        if namespace == self.no_namespace {
            return Ok(None);
        }
        if self.base_popped {
            panic!("{}", BASE_POPPED);
        }
        for &(prefix, entry_namespace) in self.entries.iter().rev() {
            if entry_namespace == namespace && prefix != self.empty {
                return Ok(Some(prefix));
            }
        }
        Err(Error::MissingPrefix(
            xot.namespace_str(namespace).to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::output::FullnameSerializer;

    // the number of namespaces, and of prefixes beyond the three named ones
    const NAMESPACES: usize = 5;
    const EXTRA_PREFIXES: usize = 5000;

    struct World {
        xot: Xot,
        namespaces: Vec<NamespaceId>,
        // the names: in each of the namespaces, then in none
        names: Vec<NameId>,
        // the empty prefix, xml, p, q, r, and two prefixes with a number
        // above DENSE_PREFIXES
        prefixes: Vec<PrefixId>,
    }

    fn world() -> World {
        let mut xot = Xot::new();
        let mut namespaces = Vec::new();
        let mut names = Vec::new();
        for ns in ["urn:a", "urn:b", "urn:c", "urn:d", "urn:e"] {
            let ns = xot.add_namespace(ns);
            names.push(xot.add_name_ns("n", ns));
            namespaces.push(ns);
        }
        names.push(xot.add_name("n"));
        let mut prefixes = vec![xot.empty_prefix(), xot.xml_prefix()];
        for p in ["p", "q", "r"] {
            prefixes.push(xot.add_prefix(p));
        }
        let mut high = Vec::new();
        for i in 0..EXTRA_PREFIXES {
            high.push(xot.add_prefix(&format!("x{}", i)));
        }
        assert!(high.last().unwrap().0 > DENSE_PREFIXES);
        prefixes.push(high[DENSE_PREFIXES as usize + 10]);
        prefixes.push(high[EXTRA_PREFIXES - 1]);
        World {
            xot,
            namespaces,
            names,
            prefixes,
        }
    }

    #[derive(Debug, Clone)]
    enum Op {
        // each declaration is a prefix index and a namespace index; the
        // namespace index NAMESPACES is the namespace for no namespace
        Push(Vec<(usize, usize)>),
        Pop,
        Element(usize),
        Attribute(usize),
        // a namespace index, as for a declaration
        AddUnprefixed(usize),
        HasUnprefixed(usize),
    }

    fn arb_declaration() -> impl Strategy<Value = (usize, usize)> {
        (0..7usize, 0..=NAMESPACES)
    }

    fn arb_op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => proptest::collection::vec(arb_declaration(), 0..5).prop_map(Op::Push),
            2 => Just(Op::Pop),
            3 => (0..=NAMESPACES).prop_map(Op::Element),
            3 => (0..=NAMESPACES).prop_map(Op::Attribute),
            1 => (0..=NAMESPACES).prop_map(Op::AddUnprefixed),
            2 => (0..=NAMESPACES).prop_map(Op::HasUnprefixed),
        ]
    }

    // the base: each prefix at most once, as namespaces_in_scope gives them
    fn arb_base() -> impl Strategy<Value = Vec<(usize, usize)>> {
        proptest::collection::vec(0..=NAMESPACES, 7).prop_flat_map(|namespaces| {
            proptest::collection::vec(any::<bool>(), 7).prop_map(move |present| {
                (0..7usize)
                    .filter(|i| present[*i])
                    .map(|i| (i, namespaces[i]))
                    .collect()
            })
        })
    }

    fn declaration(world: &World, (p, ns): (usize, usize)) -> (PrefixId, NamespaceId) {
        let namespace = if ns == NAMESPACES {
            world.xot.no_namespace()
        } else {
            world.namespaces[ns]
        };
        (world.prefixes[p], namespace)
    }

    fn run(base: &[(usize, usize)], ops: &[Op]) {
        let world = world();
        let xot = &world.xot;
        let base: Vec<_> = base.iter().map(|d| declaration(&world, *d)).collect();
        let mut expected = FullnameSerializer::new(xot, base.clone());
        let mut scope = NameScope::new(xot, &base);
        // for each push that did something, whether it did
        let mut open: Vec<bool> = Vec::new();
        for op in ops {
            match op {
                Op::Push(declarations) => {
                    let declarations: Vec<_> = declarations
                        .iter()
                        .map(|d| declaration(&world, *d))
                        .collect();
                    let pushed = !declarations.is_empty();
                    expected.push(declarations.clone());
                    assert_eq!(scope.push(declarations.iter().copied()), pushed);
                    open.push(pushed);
                }
                Op::Pop => {
                    if let Some(pushed) = open.pop() {
                        expected.pop(pushed);
                        if pushed {
                            scope.pop();
                        }
                    }
                }
                Op::Element(i) => {
                    let name = world.names[*i];
                    assert_eq!(
                        scope.element_prefix(xot, name).map_err(|e| e.to_string()),
                        expected.element_prefix(name).map_err(|e| e.to_string())
                    );
                }
                Op::Attribute(i) => {
                    let name = world.names[*i];
                    assert_eq!(
                        scope.attribute_prefix(xot, name).map_err(|e| e.to_string()),
                        expected.attribute_prefix(name).map_err(|e| e.to_string())
                    );
                }
                Op::AddUnprefixed(ns) => {
                    let (_, namespace) = declaration(&world, (0, *ns));
                    expected.add_empty_prefix(namespace);
                    scope.add_unprefixed(namespace);
                }
                Op::HasUnprefixed(ns) => {
                    let (_, namespace) = declaration(&world, (0, *ns));
                    assert_eq!(
                        scope.has_unprefixed(namespace),
                        expected.has_empty_prefix(namespace)
                    );
                }
            }
        }
        check_counts(&scope);
        // leave every level, and look again at the base
        while let Some(pushed) = open.pop() {
            expected.pop(pushed);
            if pushed {
                scope.pop();
            }
        }
        for name in &world.names {
            assert_eq!(
                scope.element_prefix(xot, *name).map_err(|e| e.to_string()),
                expected.element_prefix(*name).map_err(|e| e.to_string())
            );
            assert_eq!(
                scope
                    .attribute_prefix(xot, *name)
                    .map_err(|e| e.to_string()),
                expected.attribute_prefix(*name).map_err(|e| e.to_string())
            );
        }
        for ns in 0..=NAMESPACES {
            let (_, namespace) = declaration(&world, (0, ns));
            assert_eq!(
                scope.has_unprefixed(namespace),
                expected.has_empty_prefix(namespace)
            );
        }
    }

    proptest! {
        #[test]
        fn test_scope_equals_fullname_serializer(
            base in arb_base(),
            ops in proptest::collection::vec(arb_op(), 0..60),
        ) {
            run(&base, &ops);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100_000))]
        #[test]
        #[ignore]
        fn test_scope_equals_fullname_serializer_soak(
            base in arb_base(),
            ops in proptest::collection::vec(arb_op(), 0..60),
        ) {
            run(&base, &ops);
        }
    }

    fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> Option<String> {
        std::panic::catch_unwind(f).err().map(|cause| {
            cause
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default()
        })
    }

    // an end tag without its start tag pops the namespaces in scope
    #[test]
    fn test_pop_beyond_the_base_is_as_in_fullname_serializer() {
        let mut xot = Xot::new();
        let ns = xot.add_namespace("urn:1");
        let a = xot.add_name_ns("a", ns);
        let lone = xot.add_name("lone");
        let p = xot.add_prefix("p");
        let base = [(p, ns)];
        let mut expected = FullnameSerializer::new(&xot, base.to_vec());
        let mut scope = NameScope::new(&xot, &base);
        for _ in 0..2 {
            expected.pop(true);
            scope.pop();
            assert_eq!(scope.element_prefix(&xot, lone).unwrap(), None);
            assert_eq!(scope.attribute_prefix(&xot, lone).unwrap(), None);
            assert_eq!(expected.element_prefix(lone).unwrap(), None);
            let old = panic_message(std::panic::AssertUnwindSafe(|| {
                let _ = expected.element_prefix(a);
            }));
            let new = panic_message(std::panic::AssertUnwindSafe(|| {
                let _ = scope.element_prefix(&xot, a);
            }));
            assert!(old.is_some());
            assert_eq!(old, new);
            let old = panic_message(std::panic::AssertUnwindSafe(|| {
                let _ = expected.attribute_prefix(a);
            }));
            let new = panic_message(std::panic::AssertUnwindSafe(|| {
                let _ = scope.attribute_prefix(&xot, a);
            }));
            assert!(old.is_some());
            assert_eq!(old, new);
            let old = panic_message(std::panic::AssertUnwindSafe(|| {
                expected.push(vec![(p, ns)]);
            }));
            let new = panic_message(std::panic::AssertUnwindSafe(|| {
                scope.push([(p, ns)].into_iter());
            }));
            assert!(old.is_some());
            assert_eq!(old, new);
            let old = panic_message(std::panic::AssertUnwindSafe(|| {
                expected.has_empty_prefix(ns);
            }));
            let new = panic_message(std::panic::AssertUnwindSafe(|| {
                scope.has_unprefixed(ns);
            }));
            assert!(old.is_some());
            assert_eq!(old, new);
            let old = panic_message(std::panic::AssertUnwindSafe(|| {
                expected.add_empty_prefix(ns);
            }));
            let new = panic_message(std::panic::AssertUnwindSafe(|| {
                scope.add_unprefixed(ns);
            }));
            assert!(old.is_some());
            assert_eq!(old, new);
            // no declarations: nothing happens
            expected.push(Vec::new());
            assert!(!scope.push(std::iter::empty()));
        }
    }

    #[test]
    fn test_pop_restores_levels() {
        let mut xot = Xot::new();
        let ns1 = xot.add_namespace("urn:1");
        let ns2 = xot.add_namespace("urn:2");
        let a1 = xot.add_name_ns("a", ns1);
        let a2 = xot.add_name_ns("a", ns2);
        let p = xot.add_prefix("p");
        let mut scope = NameScope::new(&xot, &[(p, ns1)]);
        assert!(scope.push([(p, ns2)].into_iter()));
        assert_eq!(scope.element_prefix(&xot, a2).unwrap(), Some(p));
        assert!(scope.element_prefix(&xot, a1).is_err());
        assert!(!scope.push(std::iter::empty()));
        scope.pop();
        assert_eq!(scope.element_prefix(&xot, a1).unwrap(), Some(p));
        assert!(scope.attribute_prefix(&xot, a2).is_err());
    }

    #[test]
    fn test_add_unprefixed() {
        let mut xot = Xot::new();
        let ns1 = xot.add_namespace("urn:1");
        let ns2 = xot.add_namespace("urn:2");
        let a1 = xot.add_name_ns("a", ns1);
        let p = xot.add_prefix("p");
        let empty = xot.empty_prefix();
        let mut scope = NameScope::new(&xot, &[(p, ns1)]);
        assert!(!scope.has_unprefixed(ns1));
        assert!(scope.push([(p, ns2)].into_iter()));
        scope.add_unprefixed(ns1);
        assert!(scope.has_unprefixed(ns1));
        assert_eq!(scope.element_prefix(&xot, a1).unwrap(), None);
        // a later level that declares the empty prefix takes it out of scope
        assert!(scope.push([(empty, ns2)].into_iter()));
        assert!(!scope.has_unprefixed(ns1));
        assert!(scope.element_prefix(&xot, a1).is_err());
        scope.pop();
        assert!(scope.has_unprefixed(ns1));
        // it goes away with its level, which leaves the levels as they were
        scope.pop();
        assert!(!scope.has_unprefixed(ns1));
        assert_eq!(scope.element_prefix(&xot, a1).unwrap(), Some(p));
        assert!(scope.push([(empty, ns2)].into_iter()));
        assert!(scope.has_unprefixed(ns2));
        scope.pop();
        assert!(!scope.has_unprefixed(ns2));
    }

    // the entries and the counts agree with each other
    fn check_counts(scope: &NameScope) {
        let mut counts = std::collections::HashMap::new();
        for (prefix, _) in &scope.entries {
            *counts.entry(prefix.0).or_insert(0u32) += 1;
        }
        for (number, count) in scope.dense.iter().enumerate() {
            assert_eq!(*count, counts.get(&(number as u32)).copied().unwrap_or(0));
        }
        for (number, count) in &scope.wide {
            assert!(*count > 0);
            assert_eq!(Some(count), counts.get(number));
        }
        for (number, count) in &counts {
            assert_eq!(scope.count(PrefixId(*number)), *count);
        }
    }

    // a prefix declared again by each of many nested elements leaves one
    // entry in scope, so that a lookup does not grow with the depth
    #[test]
    fn test_deep_redeclaration_keeps_one_entry() {
        let mut xot = Xot::new();
        let ns = [xot.add_namespace("urn:1"), xot.add_namespace("urn:2")];
        let names = [xot.add_name_ns("a", ns[0]), xot.add_name_ns("a", ns[1])];
        let p = xot.add_prefix("p");
        let q = xot.add_prefix("q");
        let base = [(p, ns[0]), (q, ns[1])];
        let mut expected = FullnameSerializer::new(&xot, base.to_vec());
        let mut scope = NameScope::new(&xot, &base);
        let depth = 10_000;
        for level in 0..depth {
            let declaration = (p, ns[(level + 1) % 2]);
            expected.push(vec![declaration]);
            assert!(scope.push([declaration].into_iter()));
            assert_eq!(scope.entries.len(), 2);
            if level % 1000 == 0 {
                for name in names {
                    assert_eq!(
                        scope.element_prefix(&xot, name).map_err(|e| e.to_string()),
                        expected.element_prefix(name).map_err(|e| e.to_string())
                    );
                }
            }
        }
        for _ in 0..depth {
            expected.pop(true);
            scope.pop();
        }
        assert_eq!(scope.entries, base.to_vec());
        assert!(scope.marks.is_empty());
        assert!(scope.taken.is_empty());
        check_counts(&scope);
    }

    // many prefixes for one namespace, with numbers on both sides of
    // DENSE_PREFIXES, declared again by nested elements
    #[test]
    fn test_many_prefixes_for_one_namespace() {
        let mut xot = Xot::new();
        let ns1 = xot.add_namespace("urn:1");
        let ns2 = xot.add_namespace("urn:2");
        let names = [xot.add_name_ns("a", ns1), xot.add_name_ns("a", ns2)];
        let prefixes: Vec<_> = (0..5000)
            .map(|i| xot.add_prefix(&format!("p{}", i)))
            .collect();
        let base: Vec<_> = prefixes.iter().map(|p| (*p, ns1)).collect();
        let mut expected = FullnameSerializer::new(&xot, base.clone());
        let mut scope = NameScope::new(&xot, &base);
        check_counts(&scope);
        let mut open = 0;
        for (i, k) in [4999, 10, 4500, 4999, 0, 4096, 4095]
            .into_iter()
            .enumerate()
        {
            let mut declarations = vec![(prefixes[k], ns2)];
            if i % 2 == 0 {
                declarations.push((prefixes[k], ns1));
                declarations.push((xot.empty_prefix(), ns2));
            }
            expected.push(declarations.clone());
            assert!(scope.push(declarations.into_iter()));
            open += 1;
            check_counts(&scope);
            for name in names {
                assert_eq!(
                    scope.element_prefix(&xot, name).unwrap(),
                    expected.element_prefix(name).unwrap()
                );
                assert_eq!(
                    scope.attribute_prefix(&xot, name).unwrap(),
                    expected.attribute_prefix(name).unwrap()
                );
            }
            if i == 3 {
                scope.pop();
                expected.pop(true);
                open -= 1;
                check_counts(&scope);
            }
        }
        for _ in 0..open {
            scope.pop();
            expected.pop(true);
        }
        assert_eq!(scope.entries, base);
        check_counts(&scope);
    }

    // nested elements that each declare again many prefixes of a wide scope,
    // in the order of the list, in reverse, every few, twice on one element
    // and mixed with new prefixes, on both sides of DENSE_PREFIXES
    #[test]
    fn test_wide_redeclaration() {
        let mut xot = Xot::new();
        let namespaces: Vec<_> = (0..4)
            .map(|i| xot.add_namespace(&format!("urn:{}", i)))
            .collect();
        let names: Vec<_> = namespaces
            .iter()
            .map(|ns| xot.add_name_ns("a", *ns))
            .collect();
        let prefixes: Vec<_> = (0..4200)
            .map(|i| xot.add_prefix(&format!("p{}", i)))
            .collect();
        let in_scope = |i: usize| (prefixes[4200 - 1000 + i], namespaces[i % 3]);
        let base: Vec<_> = (0..1000).map(in_scope).collect();
        let mut expected = FullnameSerializer::new(&xot, base.clone());
        let mut scope = NameScope::new(&xot, &base);
        let shapes: Vec<Vec<(PrefixId, NamespaceId)>> = vec![
            (0..1000).map(|i| (in_scope(i).0, namespaces[3])).collect(),
            (0..1000)
                .rev()
                .map(|i| (in_scope(i).0, namespaces[(i + 1) % 4]))
                .collect(),
            (0..1000)
                .step_by(50)
                .map(|i| (in_scope(i).0, namespaces[3]))
                .collect(),
            (0..1000)
                .step_by(7)
                .flat_map(|i| {
                    [
                        (in_scope(i).0, namespaces[3]),
                        (prefixes[i], namespaces[1]),
                        (in_scope(i).0, namespaces[2]),
                    ]
                })
                .collect(),
            vec![(xot.empty_prefix(), namespaces[0])],
            (990..1000)
                .map(|i| (in_scope(i).0, namespaces[0]))
                .collect(),
        ];
        let compare = |scope: &NameScope, expected: &FullnameSerializer| {
            check_counts(scope);
            for name in &names {
                assert_eq!(
                    scope.element_prefix(&xot, *name).map_err(|e| e.to_string()),
                    expected.element_prefix(*name).map_err(|e| e.to_string())
                );
                assert_eq!(
                    scope
                        .attribute_prefix(&xot, *name)
                        .map_err(|e| e.to_string()),
                    expected.attribute_prefix(*name).map_err(|e| e.to_string())
                );
            }
        };
        // the entries before each open push
        let mut open = Vec::new();
        for (step, k) in [0, 1, 2, 3, 2, 4, 5, 1, 0, 3, 5, 5, 4]
            .into_iter()
            .enumerate()
        {
            open.push(scope.entries.clone());
            expected.push(shapes[k].clone());
            assert!(scope.push(shapes[k].iter().copied()));
            compare(&scope, &expected);
            if step % 3 == 2 {
                expected.pop(true);
                scope.pop();
                assert_eq!(scope.entries, open.pop().unwrap());
                compare(&scope, &expected);
            }
        }
        while let Some(entries) = open.pop() {
            expected.pop(true);
            scope.pop();
            assert_eq!(scope.entries, entries);
            compare(&scope, &expected);
        }
        assert_eq!(scope.entries, base);
        assert!(scope.taken.is_empty());
    }

    #[test]
    fn test_prefix_above_dense_table() {
        let mut xot = Xot::new();
        let ns1 = xot.add_namespace("urn:1");
        let ns2 = xot.add_namespace("urn:2");
        let a1 = xot.add_name_ns("a", ns1);
        let a2 = xot.add_name_ns("a", ns2);
        let mut high = None;
        for i in 0..(DENSE_PREFIXES as usize + 5) {
            high = Some(xot.add_prefix(&format!("h{}", i)));
        }
        let high = high.unwrap();
        assert!(high.0 >= DENSE_PREFIXES);
        let mut scope = NameScope::new(&xot, &[(high, ns1)]);
        assert_eq!(scope.element_prefix(&xot, a1).unwrap(), Some(high));
        assert!(scope.push([(high, ns2)].into_iter()));
        assert!(scope.element_prefix(&xot, a1).is_err());
        assert_eq!(scope.element_prefix(&xot, a2).unwrap(), Some(high));
        scope.pop();
        assert_eq!(scope.element_prefix(&xot, a1).unwrap(), Some(high));
        assert!(scope.element_prefix(&xot, a2).is_err());
    }
}
