//! Sets of Unicode scalar values as sorted, disjoint, non-adjacent
//! inclusive ranges.

pub(crate) const MAX_CP: u32 = 0x10FFFF;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CharSet {
    ranges: Vec<(u32, u32)>,
}

impl CharSet {
    pub(crate) fn empty() -> Self {
        Self { ranges: Vec::new() }
    }

    pub(crate) fn all() -> Self {
        Self {
            ranges: vec![(0, MAX_CP)],
        }
    }

    pub(crate) fn single(c: u32) -> Self {
        Self {
            ranges: vec![(c, c)],
        }
    }

    pub(crate) fn range(a: u32, b: u32) -> Self {
        Self::from_ranges(vec![(a, b)])
    }

    pub(crate) fn from_ranges(mut ranges: Vec<(u32, u32)>) -> Self {
        ranges.retain(|&(a, b)| a <= b);
        ranges.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (a, b) in ranges {
            if let Some(last) = out.last_mut() {
                if a <= last.1.saturating_add(1) {
                    if b > last.1 {
                        last.1 = b;
                    }
                    continue;
                }
            }
            out.push((a, b));
        }
        // Merging can leave far fewer ranges than it was given; the set
        // holds what it counts against the parser's ceiling.
        out.shrink_to_fit();
        Self { ranges: out }
    }

    pub(crate) fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    /// The bytes the set keeps on the heap, at what the allocator takes
    /// for them.
    pub(crate) fn heap_bytes(&self) -> usize {
        crate::chunk(self.ranges.capacity() * size_of::<(u32, u32)>())
    }

    pub(crate) fn union(&self, other: &CharSet) -> CharSet {
        let mut r = self.ranges.clone();
        r.extend_from_slice(&other.ranges);
        Self::from_ranges(r)
    }

    pub(crate) fn negate(&self) -> CharSet {
        let mut out = Vec::with_capacity(self.ranges.len() + 1);
        let mut next = 0u32;
        for &(a, b) in &self.ranges {
            if a > next {
                out.push((next, a - 1));
            }
            next = b + 1;
        }
        if next <= MAX_CP {
            out.push((next, MAX_CP));
        }
        Self { ranges: out }
    }

    pub(crate) fn intersect(&self, other: &CharSet) -> CharSet {
        let mut out = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.ranges.len() && j < other.ranges.len() {
            let (a1, b1) = self.ranges[i];
            let (a2, b2) = other.ranges[j];
            let lo = a1.max(a2);
            let hi = b1.min(b2);
            if lo <= hi {
                out.push((lo, hi));
            }
            if b1 < b2 {
                i += 1;
            } else {
                j += 1;
            }
        }
        out.shrink_to_fit();
        Self { ranges: out }
    }

    pub(crate) fn difference(&self, other: &CharSet) -> CharSet {
        self.intersect(&other.negate())
    }

    pub(crate) fn contains(&self, c: u32) -> bool {
        self.ranges
            .binary_search_by(|&(a, b)| {
                if c < a {
                    std::cmp::Ordering::Greater
                } else if c > b {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_algebra() {
        let a = CharSet::range(b'a' as u32, b'z' as u32);
        let v = CharSet::from_ranges(vec![(b'a' as u32, b'a' as u32), (b'e' as u32, b'e' as u32)]);
        let d = a.difference(&v);
        assert!(!d.contains(b'a' as u32));
        assert!(d.contains(b'b' as u32));
        assert!(!d.contains(b'e' as u32));
        assert_eq!(a.negate().negate(), a);
        assert_eq!(CharSet::all().negate(), CharSet::empty());
        assert_eq!(
            CharSet::from_ranges(vec![(1, 3), (4, 6), (10, 12), (11, 20)]).ranges(),
            &[(1, 6), (10, 20)]
        );
    }
}
