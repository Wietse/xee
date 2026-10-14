use super::idmap::{IdIndex, IdMap};
use super::namespace::NamespaceId;

/// Id uniquely identifying a name and namespace.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct NameId(pub(crate) u32);

impl IdIndex<NameId> for NameId {
    fn to_id(index: usize) -> NameId {
        // u32, not u16: 65,536 distinct names are within reach of one
        // document, and a wrapped id would alias another
        NameId(u32::try_from(index).expect("more than 2^32 distinct names"))
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
