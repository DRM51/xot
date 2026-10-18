//! The namespaces in scope of the XML writer: one flat stack of declarations,
//! instead of a copied list for each element that declares namespaces. It
//! picks the same prefixes as [`FullnameSerializer`](super::FullnameSerializer).
//!
//! `FullnameSerializer` keeps, for each open element that declares
//! namespaces, the list of the declarations in scope: the list of the parent
//! without the entries whose prefix this element declares, followed by the
//! declarations of this element. That list is the flat stack of all
//! declarations with every entry dropped whose prefix is declared again by a
//! later *level* (the declarations of one element are one level, so two
//! declarations of one prefix on one element both stay). An entry is live
//! when no later level declares its prefix, and a lookup reads the live
//! entries from the last to the first, as the copied lists are read.
//!
//! The levels are numbers: the namespaces in scope of the node that is
//! serialized are level 1, and the element that opens the k-th level of
//! declarations has level k + 1. For each prefix, a table holds the highest
//! level that declares it, or 0 when no level does.

use crate::error::Error;
use crate::id::{NameId, NamespaceId, PrefixId};
use crate::xotdata::Xot;

/// Prefixes with a number below this are looked up in a table indexed by the
/// number; the others in a short list.
const DENSE_PREFIXES: u32 = 4096;

/// The level of the namespaces in scope of the node that is serialized.
const BASE_LEVEL: u32 = 1;

#[derive(Debug, Clone, Copy)]
struct ScopeEntry {
    prefix: PrefixId,
    namespace: NamespaceId,
    level: u32,
    // the level that declared the prefix before this entry did, 0 for none
    previous: u32,
}

#[derive(Debug)]
pub(crate) struct NameScope {
    entries: Vec<ScopeEntry>,
    // the start in `entries` of each level above the base
    marks: Vec<usize>,
    // by prefix number, for numbers below DENSE_PREFIXES: the highest level
    // that declares the prefix
    dense: Vec<u32>,
    // the same for the other prefixes, as (prefix number, level)
    wide: Vec<(u32, u32)>,
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
            dense: Vec::new(),
            wide: Vec::new(),
            empty: xot.empty_prefix_id,
            no_namespace: xot.no_namespace_id,
            base_popped: false,
        };
        for &(prefix, namespace) in base {
            scope.declare(prefix, namespace, BASE_LEVEL);
        }
        scope
    }

    #[inline]
    fn level(&self, prefix: PrefixId) -> u32 {
        let number = prefix.0;
        if number < DENSE_PREFIXES {
            self.dense.get(number as usize).copied().unwrap_or(0)
        } else {
            self.wide
                .iter()
                .find(|(p, _)| *p == number)
                .map_or(0, |(_, level)| *level)
        }
    }

    #[inline]
    fn set_level(&mut self, prefix: PrefixId, level: u32) {
        let number = prefix.0;
        if number < DENSE_PREFIXES {
            let index = number as usize;
            if index >= self.dense.len() {
                self.dense.resize(index + 1, 0);
            }
            self.dense[index] = level;
        } else if let Some(entry) = self.wide.iter_mut().find(|(p, _)| *p == number) {
            entry.1 = level;
        } else {
            self.wide.push((number, level));
        }
    }

    #[inline]
    fn declare(&mut self, prefix: PrefixId, namespace: NamespaceId, level: u32) {
        let previous = self.level(prefix);
        self.set_level(prefix, level);
        self.entries.push(ScopeEntry {
            prefix,
            namespace,
            level,
            previous,
        });
    }

    /// Add the declarations of an element as a new level, in their order.
    ///
    /// Returns whether there were any: only then is there a level to
    /// [`pop`](NameScope::pop) at the end of the element.
    #[inline]
    pub(crate) fn push(
        &mut self,
        declarations: impl Iterator<Item = (PrefixId, NamespaceId)>,
    ) -> bool {
        let mark = self.entries.len();
        let level = self.marks.len() as u32 + BASE_LEVEL + 1;
        for (prefix, namespace) in declarations {
            if self.entries.len() == mark {
                if self.base_popped {
                    panic!("{}", BASE_POPPED);
                }
                self.marks.push(mark);
            }
            self.declare(prefix, namespace, level);
        }
        self.entries.len() != mark
    }

    /// Remove the last level that [`push`](NameScope::push) added.
    ///
    /// The walk of a node that was changed in an unusual way can give the end
    /// tag of an element whose start tag it did not give, when it goes on
    /// after the node that is serialized. [`FullnameSerializer`] then removes
    /// its namespaces in scope, so that later names in a namespace and later
    /// declarations make it panic; this does the same, with the same message.
    #[inline]
    pub(crate) fn pop(&mut self) {
        let Some(mark) = self.marks.pop() else {
            self.base_popped = true;
            return;
        };
        for index in (mark..self.entries.len()).rev() {
            let entry = self.entries[index];
            self.set_level(entry.prefix, entry.previous);
        }
        self.entries.truncate(mark);
    }

    /// Whether no later level declares the prefix of the entry.
    #[inline]
    fn live(&self, entry: &ScopeEntry) -> bool {
        self.level(entry.prefix) <= entry.level
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
        for entry in self.entries.iter().rev() {
            if entry.namespace != namespace || !self.live(entry) {
                continue;
            }
            if entry.prefix == self.empty {
                return Ok(None);
            }
            if found.is_none() {
                found = Some(entry.prefix);
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
        for entry in self.entries.iter().rev() {
            if entry.namespace == namespace && entry.prefix != self.empty && self.live(entry) {
                return Ok(Some(entry.prefix));
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
            }
        }
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
