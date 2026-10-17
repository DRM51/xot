use super::idmap::{id_number, IdIndex, IdMap};

/// Id uniquely identifying namespace.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct NamespaceId(u32);

impl IdIndex<NamespaceId> for NamespaceId {
    fn to_id(index: usize) -> NamespaceId {
        NamespaceId(id_number(index))
    }

    fn from_id(id: NamespaceId) -> usize {
        id.0 as usize
    }
}

pub(crate) type NamespaceLookup = IdMap<NamespaceId, String>;
