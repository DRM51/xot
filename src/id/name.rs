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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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

pub(crate) type NameLookup = IdMap<NameId, Name>;
