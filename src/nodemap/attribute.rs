use crate::xmlvalue::{Attribute, Value, ValueCategory};
use crate::{NameId, Node, Xot};

use super::core::{key_bit, MutableNodeMap, NodeMap, ValueAdapter};

pub struct AttributeAdapter {}

impl ValueAdapter<NameId, String> for AttributeAdapter {
    #[inline]
    fn matches(value: &Value) -> bool {
        matches!(value, Value::Attribute(_))
    }

    #[inline]
    fn entries(xot: &Xot, node: Node) -> impl Iterator<Item = (Node, &Value)> + '_ {
        xot.all_children_values(node)
            .skip_while(|(_, value)| value.value_category() == ValueCategory::Namespace)
            .take_while(|(_, value)| value.value_category() == ValueCategory::Attribute)
    }

    // the map is the attribute nodes after the namespace nodes at the start
    // of the children; a new one goes after the last of them, if there is
    // none after the last namespace node, and if there is none of these
    // either, first
    #[inline]
    fn find_or_end(
        xot: &Xot,
        node: Node,
        mut found: impl FnMut(&Value) -> bool,
    ) -> Result<Node, Option<Node>> {
        let mut last = None;
        let mut attributes = false;
        let found = xot.arena.find_child(node.get(), |child, value| {
            match value {
                Value::Namespace(_) if !attributes => {}
                Value::Attribute(_) => {
                    attributes = true;
                    if found(value) {
                        return Some(Some(child));
                    }
                }
                _ => return Some(None),
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
    fn key(value: &Value) -> NameId {
        match value {
            Value::Attribute(Attribute { name_id, .. }) => *name_id,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn key_bit(key: NameId) -> u64 {
        key_bit(key.0)
    }

    #[inline]
    fn value(value: &Value) -> &String {
        match value {
            Value::Attribute(Attribute { value, .. }) => value,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn value_mut(value: &mut Value) -> &mut String {
        match value {
            Value::Attribute(Attribute { value, .. }) => value,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn create(key: NameId, value: String) -> Value {
        Value::Attribute(Attribute {
            name_id: key,
            value,
        })
    }

    #[inline]
    fn update(value: &mut Value, new_value: String) -> Option<String> {
        match value {
            Value::Attribute(Attribute {
                value: old_value, ..
            }) => {
                let old_value = std::mem::replace(old_value, new_value);
                Some(old_value)
            }
            _ => unreachable!(),
        }
    }
}

/// Attributes of an element.
///
/// Behaves like a HashMap, but stores the data in the tree, so that namespace
/// nodes have a parent and can exist unattached.
///
/// Access is linear time. Insertion order is preserved.
///
/// Obtained using [`Xot::attributes`].
///
/// See [`NodeMap`] for details.
pub type Attributes<'a> = NodeMap<'a, NameId, String, AttributeAdapter>;

/// Mutable attributes of an element.
///
/// Obtained using [`Xot::attributes_mut`].
///
/// See [`MutableNodeMap`] for details.
///
/// See also [`Attributes`].
pub type MutableAttributes<'a> = MutableNodeMap<'a, NameId, String, AttributeAdapter>;
