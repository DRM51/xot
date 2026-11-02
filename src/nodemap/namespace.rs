use crate::xmlvalue::{Namespace, Value, ValueCategory};
use crate::{NamespaceId, Node, PrefixId, Xot};

use super::core::{key_bit, MutableNodeMap, NodeMap, ValueAdapter};

pub struct NamespaceAdapter {}

impl ValueAdapter<PrefixId, NamespaceId> for NamespaceAdapter {
    type Read<'a> = NamespaceId;

    #[inline]
    fn matches(value: &Value) -> bool {
        matches!(value, Value::Namespace(_))
    }

    #[inline]
    fn entries(xot: &Xot, node: Node) -> impl Iterator<Item = (Node, &Value)> + '_ {
        xot.all_children_values(node)
            .take_while(|(_, value)| value.value_category() == ValueCategory::Namespace)
    }

    // the map is the namespace nodes at the start of the children; a new one
    // goes after the last of them, or first if there is none
    #[inline]
    fn find_or_end(
        xot: &Xot,
        node: Node,
        mut found: impl FnMut(&Value) -> bool,
    ) -> Result<Node, Option<Node>> {
        let mut last = None;
        let found = xot.arena.find_child(node.get(), |child, value| {
            if !matches!(value, Value::Namespace(_)) {
                return Some(None);
            }
            if found(value) {
                return Some(Some(child));
            }
            last = Some(child);
            None
        });
        match found.flatten() {
            Some(child) => Ok(Node::new(child)),
            None => Err(last.map(Node::new)),
        }
    }

    #[inline]
    fn key(value: &Value) -> PrefixId {
        match value {
            Value::Namespace(Namespace { prefix_id, .. }) => *prefix_id,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn key_bit(key: PrefixId) -> u64 {
        key_bit(key.0)
    }

    #[inline]
    fn value(value: &Value) -> &NamespaceId {
        match value {
            Value::Namespace(Namespace { namespace_id, .. }) => namespace_id,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn read(value: &Value) -> Self::Read<'_> {
        match value {
            Value::Namespace(Namespace { namespace_id, .. }) => *namespace_id,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn value_mut(value: &mut Value) -> &mut NamespaceId {
        match value {
            Value::Namespace(Namespace { namespace_id, .. }) => namespace_id,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn create(key: PrefixId, value: NamespaceId) -> Value {
        Value::Namespace(Namespace {
            prefix_id: key,
            namespace_id: value,
        })
    }

    #[inline]
    fn update(value: &mut Value, new_value: NamespaceId) -> Option<NamespaceId> {
        match value {
            Value::Namespace(Namespace {
                namespace_id: old_value,
                ..
            }) => {
                let old_value = std::mem::replace(old_value, new_value);
                Some(old_value)
            }
            _ => unreachable!(),
        }
    }
}

/// A map of namespace prefixes to namespace ids.
///
/// Behaves like a HashMap, but stores the data in the tree, so that namespace
/// nodes have a parent and can exist unattached.
///
/// Access is linear time. Insertion order is preserved.
///
/// Obtained using [`Xot::namespaces`].
///
/// The values are read by value, as namespace ids:
///
/// ```rust
/// use xot::{NamespaceId, PrefixId, Xot};
///
/// let mut xot = Xot::new();
/// let root = xot.parse(r#"<doc xmlns:p="urn:example:p"/>"#)?;
/// let doc_el = xot.document_element(root).unwrap();
/// let p = xot.prefix("p").unwrap();
/// let urn_p = xot.namespace("urn:example:p").unwrap();
///
/// let namespaces = xot.namespaces(doc_el);
/// let namespace: Option<NamespaceId> = namespaces.get(p);
/// assert_eq!(namespace, Some(urn_p));
/// let pairs: Vec<(PrefixId, NamespaceId)> = namespaces.iter().collect();
/// assert_eq!(pairs, [(p, urn_p)]);
/// # Ok::<(), xot::Error>(())
/// ```
///
/// See [`NodeMap`] for details.
pub type Namespaces<'a> = NodeMap<'a, PrefixId, NamespaceId, NamespaceAdapter>;

/// A mutable map of namespace prefixes to namespace ids.
///
/// Obtained using [`Xot::namespaces_mut`].
///
/// See [`MutableNodeMap`] for details.
///
/// See also [`Namespaces`].
pub type MutableNamespaces<'a> = MutableNodeMap<'a, PrefixId, NamespaceId, NamespaceAdapter>;
