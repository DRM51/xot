use std::borrow::Borrow;
use std::hash::{Hash, Hasher};

use super::idmap::{id_number, IdIndex, IdMap};
use super::namespace::NamespaceId;

/// Id uniquely identifying a name and namespace.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct NameId(pub(crate) u32);

impl IdIndex<NameId> for NameId {
    fn to_id(index: usize) -> NameId {
        NameId(id_number(index))
    }

    fn from_id(id: NameId) -> usize {
        id.0 as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Name {
    pub(crate) name: String,
    pub(crate) namespace_id: NamespaceId,
}

impl Name {
    pub(crate) fn new<S: Into<String>>(name: S, namespace_id: NamespaceId) -> Self {
        Self {
            name: name.into(),
            namespace_id,
        }
    }
}

/// A name as a local name and a namespace, borrowed or owned, so that a
/// name can be looked up without making a `String` for it.
pub(crate) trait NameKey {
    fn key(&self) -> (&str, NamespaceId);
}

impl NameKey for Name {
    fn key(&self) -> (&str, NamespaceId) {
        (&self.name, self.namespace_id)
    }
}

impl NameKey for (&str, NamespaceId) {
    fn key(&self) -> (&str, NamespaceId) {
        (self.0, self.1)
    }
}

impl<'a> Borrow<dyn NameKey + 'a> for Name {
    fn borrow(&self) -> &(dyn NameKey + 'a) {
        self
    }
}

impl Hash for dyn NameKey + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (name, namespace_id) = self.key();
        name.hash(state);
        namespace_id.hash(state);
    }
}

impl PartialEq for dyn NameKey + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for dyn NameKey + '_ {}

// a `Name` hashes and compares as its key, as `Borrow` requires
impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self as &dyn NameKey).hash(state)
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Name {}

pub(crate) type NameLookup = IdMap<NameId, Name>;

impl NameLookup {
    /// The id of the name, made if there is none yet.
    pub(crate) fn name_id_mut(&mut self, name: &str, namespace_id: NamespaceId) -> NameId {
        self.get_id_or_insert_with(&(name, namespace_id) as &dyn NameKey, || {
            Name::new(name, namespace_id)
        })
    }

    /// The id of the name, if there is one.
    pub(crate) fn name_id(&self, name: &str, namespace_id: NamespaceId) -> Option<NameId> {
        self.get_id(&(name, namespace_id) as &dyn NameKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Xot;

    /// The hash of `value` with fixed seeds.
    fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
        ahash::RandomState::with_seeds(1, 2, 3, 4).hash_one(value)
    }

    #[test]
    fn existing_name_gives_its_id() {
        let mut xot = Xot::new();
        let ns = xot.add_namespace("urn:example:a");
        let first = xot.add_name_ns("item", ns);
        assert_eq!(xot.add_name_ns("item", ns), first);
        assert_eq!(xot.name_ns("item", ns), Some(first));
        let root = xot.parse(r#"<item xmlns="urn:example:a"/>"#).unwrap();
        let item = xot.document_element(root).unwrap();
        assert_eq!(xot.element(item).unwrap().name(), first);
    }

    #[test]
    fn new_names_get_the_next_ids_in_first_seen_order() {
        let mut xot = Xot::new();
        let ns = xot.add_namespace("urn:example:a");
        let start = xot.add_name_ns("record", ns).0;
        let names = ["item", "tag", "", "r\u{e9}cord", "\u{65e5}\u{672c}", "row"];
        for (i, name) in names.iter().enumerate() {
            assert_eq!(xot.add_name_ns(name, ns).0, start + 1 + i as u32);
        }
        // asked again, in another order, they keep their ids
        for (i, name) in names.iter().enumerate().rev() {
            assert_eq!(xot.add_name_ns(name, ns).0, start + 1 + i as u32);
            assert_eq!(xot.name_ns_str(NameId(start + 1 + i as u32)).0, *name);
        }
    }

    #[test]
    fn name_ns_of_a_missing_name_does_not_add_it() {
        let mut xot = Xot::new();
        let ns = xot.add_namespace("urn:example:a");
        let before = xot.add_name_ns("record", ns).0;
        assert_eq!(xot.name_ns("item", ns), None);
        assert_eq!(xot.name_ns("item", ns), None);
        assert_eq!(xot.name_ns("", ns), None);
        // the next name made still gets the next id
        assert_eq!(xot.add_name_ns("tag", ns).0, before + 1);
        assert_eq!(xot.name_ns("item", ns), None);
        assert_eq!(xot.add_name_ns("item", ns).0, before + 2);
    }

    #[test]
    fn a_local_name_in_two_namespaces_has_two_ids() {
        let mut xot = Xot::new();
        let a = xot.add_namespace("urn:example:a");
        let b = xot.add_namespace("urn:example:b");
        let in_a = xot.add_name_ns("item", a);
        let in_b = xot.add_name_ns("item", b);
        let in_none = xot.add_name("item");
        assert_ne!(in_a, in_b);
        assert_ne!(in_a, in_none);
        assert_ne!(in_b, in_none);
        assert_eq!(xot.name_ns("item", a), Some(in_a));
        assert_eq!(xot.name_ns("item", b), Some(in_b));
        assert_eq!(xot.name("item"), Some(in_none));
        assert_eq!(xot.namespace_for_name(in_a), a);
        assert_eq!(xot.namespace_for_name(in_b), b);
    }

    #[test]
    fn ids_interleave_with_add_name() {
        let mut xot = Xot::new();
        let ns = xot.add_namespace("urn:example:a");
        let a = xot.add_name("a").0;
        assert_eq!(xot.add_name_ns("b", ns).0, a + 1);
        assert_eq!(xot.add_name("c").0, a + 2);
        assert_eq!(xot.add_name_ns("a", ns).0, a + 3);
        // `add_name` is `add_name_ns` in no namespace
        assert_eq!(xot.add_name_ns("c", xot.no_namespace()).0, a + 2);
        assert_eq!(xot.add_name("a").0, a);
        assert_eq!(xot.name("b"), None);
        assert_eq!(xot.add_name("b").0, a + 4);
    }

    #[test]
    fn name_hashes_and_compares_as_its_borrowed_key() {
        let mut xot = Xot::new();
        let a = xot.add_namespace("urn:example:a");
        let b = xot.add_namespace("urn:example:b");
        let locals = [
            "",
            "a",
            "item",
            "r\u{e9}cord",
            "\u{65e5}\u{672c}",
            "a\u{0}b",
        ];
        for local in locals {
            for ns in [a, b] {
                let name = Name::new(local, ns);
                let key: &dyn NameKey = &(local, ns);
                assert_eq!(hash_of(&name), hash_of(key));
                // as the derived `Hash` of the fields did
                assert_eq!(hash_of(&name), hash_of(&(local.to_string(), ns)));
                assert!(<Name as Borrow<dyn NameKey>>::borrow(&name) == key);
                assert_eq!(name, Name::new(local.to_string(), ns));
                for other in locals {
                    let other_key: &dyn NameKey = &(other, b);
                    assert_eq!(
                        <Name as Borrow<dyn NameKey>>::borrow(&name) == other_key,
                        local == other && ns == b
                    );
                    assert_eq!(name == Name::new(other, b), local == other && ns == b);
                }
            }
        }
    }
}
