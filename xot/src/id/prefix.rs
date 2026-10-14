use super::idmap::{IdIndex, IdMap};

/// Id uniquely identifying a prefix
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct PrefixId(u32);

impl IdIndex<PrefixId> for PrefixId {
    fn to_id(index: usize) -> PrefixId {
        // u32, not u16: 65,536 distinct prefixes are within reach of one
        // document, and a wrapped id would alias another
        PrefixId(u32::try_from(index).expect("more than 2^32 distinct prefixes"))
    }

    fn from_id(id: PrefixId) -> usize {
        id.0 as usize
    }
}

pub(crate) type PrefixLookup = IdMap<PrefixId, String>;
