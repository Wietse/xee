use super::idmap::{IdIndex, IdMap};

/// Id uniquely identifying namespace.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct NamespaceId(u32);

impl IdIndex<NamespaceId> for NamespaceId {
    fn to_id(index: usize) -> NamespaceId {
        // u32, not u16: 65,536 distinct namespaces are within reach of one
        // document, and a wrapped id would alias another
        NamespaceId(u32::try_from(index).expect("more than 2^32 distinct namespaces"))
    }

    fn from_id(id: NamespaceId) -> usize {
        id.0 as usize
    }
}

pub(crate) type NamespaceLookup = IdMap<NamespaceId, String>;
