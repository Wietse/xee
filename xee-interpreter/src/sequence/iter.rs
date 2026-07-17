use xot::Xot;

use crate::context::NodeTypedValueProvider;
use crate::{atomic, error, function};

use super::item::Item;

/// An iterator over the nodes in a sequence.
pub struct NodeIter<I>
where
    I: Iterator<Item = Item>,
{
    iter: I,
}

impl<I> NodeIter<I>
where
    I: Iterator<Item = Item>,
{
    pub(crate) fn new(iter: I) -> Self {
        Self { iter }
    }
}

impl<I> Iterator for NodeIter<I>
where
    I: Iterator<Item = Item>,
{
    type Item = error::Result<xot::Node>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next();
        next.map(|v| v.to_node())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// An iterator atomizing a sequence.
pub struct AtomizedIter<'a, I>
where
    I: Iterator<Item = Item> + 'a,
{
    provider: Option<&'a dyn NodeTypedValueProvider>,
    xot: &'a Xot,
    iter: I,
    item_iter: Option<AtomizedItemIter<'a>>,
}

impl<'a, I> AtomizedIter<'a, I>
where
    I: Iterator<Item = Item>,
{
    pub(crate) fn new(
        provider: Option<&'a dyn NodeTypedValueProvider>,
        xot: &'a Xot,
        iter: I,
    ) -> AtomizedIter<'a, I> {
        AtomizedIter {
            provider,
            xot,
            iter,
            item_iter: None,
        }
    }
}

impl<'a, I> Iterator for AtomizedIter<'a, I>
where
    I: Iterator<Item = Item> + 'a,
{
    type Item = error::Result<atomic::Atomic>;

    fn next(&mut self) -> Option<error::Result<atomic::Atomic>> {
        loop {
            // if there there are any more atoms in this node,
            // supply those
            if let Some(item_iter) = &mut self.item_iter {
                if let Some(item) = item_iter.next() {
                    return Some(item);
                } else {
                    self.item_iter = None;
                }
            }
            // if not, move on to the next item; no more items means we're done
            let item = self.iter.next()?;
            self.item_iter = Some(AtomizedItemIter::new(item, self.provider, self.xot));
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // using iter as the lower bound is safe, as we will
        // go through each item at least once. it's harder to determine an upper
        // bound however
        let (lower, _) = self.iter.size_hint();
        (lower, None)
    }
}

/// Atomizing an individual item in a sequence.
pub(crate) enum AtomizedItemIter<'a> {
    Atomic(std::iter::Once<atomic::Atomic>),
    Node(AtomizedNodeIter),
    Array(AtomizedArrayIter<'a>),
    // TODO: properly handle functions; for now they error
    Erroring(std::iter::Once<error::Result<atomic::Atomic>>),
}

impl<'a> AtomizedItemIter<'a> {
    pub(crate) fn new(
        item: Item,
        provider: Option<&'a dyn NodeTypedValueProvider>,
        xot: &'a Xot,
    ) -> Self {
        match item {
            Item::Atomic(a) => Self::Atomic(std::iter::once(a)),
            Item::Node(n) => Self::Node(AtomizedNodeIter::new(n, provider, xot)),
            Item::Function(function) => match function {
                function::Function::Array(a) => {
                    Self::Array(AtomizedArrayIter::new(a, provider, xot))
                }
                _ => Self::Erroring(std::iter::once(Err(error::Error::FOTY0013))),
            },
        }
    }
}

impl Iterator for AtomizedItemIter<'_> {
    type Item = error::Result<atomic::Atomic>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Atomic(iter) => iter.next().map(Ok),
            Self::Node(iter) => iter.next(),
            Self::Array(iter) => iter.next(),
            Self::Erroring(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Atomic(iter) => iter.size_hint(),
            Self::Node(iter) => iter.size_hint(),
            Self::Array(iter) => iter.size_hint(),
            Self::Erroring(iter) => iter.size_hint(),
        }
    }
}

/// Atomizing a node.
///
/// A node's typed value is a sequence of atomic values. With no
/// typed-value provider installed — the schema-unaware default — that
/// sequence is the single `xs:untypedAtomic` of the node's string
/// value. A provider may instead supply the node's real typed value,
/// or reject atomization of the node with an error.
pub(crate) struct AtomizedNodeIter {
    iter: std::vec::IntoIter<error::Result<atomic::Atomic>>,
}

impl AtomizedNodeIter {
    fn new(node: xot::Node, provider: Option<&dyn NodeTypedValueProvider>, xot: &Xot) -> Self {
        let typed_value: Vec<error::Result<atomic::Atomic>> = match provider {
            Some(provider) => match provider.typed_value(xot, node) {
                // The host has an authoritative typed value for this node.
                Ok(Some(values)) => values.into_iter().map(Ok).collect(),
                // The provider has no opinion — schema-unaware fallback.
                Ok(None) => vec![Ok(untyped_value(xot, node))],
                // The provider rejects atomization of this node.
                Err(e) => vec![Err(e)],
            },
            // No provider installed: schema-unaware default.
            None => vec![Ok(untyped_value(xot, node))],
        };
        Self {
            iter: typed_value.into_iter(),
        }
    }
}

/// The schema-unaware typed value of a node: a single `xs:untypedAtomic`
/// holding the node's string value.
fn untyped_value(xot: &Xot, node: xot::Node) -> atomic::Atomic {
    atomic::Atomic::Untyped(xot.string_value(node).into())
}

impl Iterator for AtomizedNodeIter {
    type Item = error::Result<atomic::Atomic>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Atomizing a XPath array
pub(crate) struct AtomizedArrayIter<'a> {
    provider: Option<&'a dyn NodeTypedValueProvider>,
    xot: &'a Xot,
    array: function::Array,
    array_index: usize,
    iter: Option<std::vec::IntoIter<error::Result<atomic::Atomic>>>,
}

impl<'a> AtomizedArrayIter<'a> {
    fn new(
        array: function::Array,
        provider: Option<&'a dyn NodeTypedValueProvider>,
        xot: &'a Xot,
    ) -> Self {
        Self {
            provider,
            xot,
            array,
            array_index: 0,
            iter: None,
        }
    }
}

impl Iterator for AtomizedArrayIter<'_> {
    type Item = error::Result<atomic::Atomic>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // if there there are any more atoms in this array entry,
            // supply those
            if let Some(iter) = &mut self.iter {
                if let Some(item) = iter.next() {
                    return Some(item);
                } else {
                    self.iter = None;
                }
            }
            let array = &self.array.0;
            // if we're at the end of the array, we're done
            if self.array_index >= array.len() {
                return None;
            }
            let sequence = &array[self.array_index];
            self.array_index += 1;
            // TODO: we have lifetime whackamole issues here, because
            // an array reference cannot live long enough, but an owned
            // array also cannot live long enough. So we collect things
            // into a vector...
            let v = sequence
                .atomized(self.provider, self.xot)
                .collect::<Vec<_>>();
            self.iter = Some(v.into_iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // we will have at least as many entries as in the array, but
        // we don't really know the upper bound
        let remaining = self.array.0.len() - self.array_index;
        (remaining, None)
    }
}

pub(crate) fn one<'a, T>(mut iter: impl Iterator<Item = T> + 'a) -> error::Result<T> {
    if let Some(one) = iter.next() {
        if iter.next().is_none() {
            Ok(one)
        } else {
            Err(error::Error::XPTY0004)
        }
    } else {
        Err(error::Error::XPTY0004)
    }
}

pub(crate) fn option<'a, T>(mut iter: impl Iterator<Item = T> + 'a) -> error::Result<Option<T>> {
    if let Some(one) = iter.next() {
        if iter.next().is_none() {
            Ok(Some(one))
        } else {
            Err(error::Error::XPTY0004)
        }
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::core::Sequence;

    // Consolidating the atomization iterators removed the `One::atomized`
    // override, so a `One` sequence now atomizes through `AtomizedIter` like
    // every other variant. These pin that rerouted path directly, rather than
    // leaning only on the conformance suites.

    #[test]
    fn one_sequence_atomizes_node_to_untyped_string_value() {
        let mut xot = Xot::new();
        let root = xot.parse("<doc>hello</doc>").unwrap();
        let doc = xot.document_element(root).unwrap();

        let seq: Sequence = vec![Item::from(doc)].into();
        assert!(matches!(seq, Sequence::One(_)), "expected a One sequence");

        let atomized = seq
            .atomized(None, &xot)
            .collect::<error::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(atomized, vec![atomic::Atomic::Untyped("hello".into())]);
    }

    #[test]
    fn one_sequence_atomizes_atomic_to_itself() {
        let xot = Xot::new();
        let seq: Sequence = vec![Item::from(atomic::Atomic::from(42i64))].into();
        assert!(matches!(seq, Sequence::One(_)), "expected a One sequence");

        let atomized = seq
            .atomized(None, &xot)
            .collect::<error::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(atomized, vec![atomic::Atomic::from(42i64)]);
    }

    /// A `NodeTypedValueProvider` whose answer is fixed up front, for
    /// exercising each branch of `AtomizedNodeIter`'s provider handling.
    /// It cannot store an `Atomic` directly (`Atomic` holds `Rc`, so it is
    /// not `Send + Sync`); it carries the integer and builds the atom.
    enum Stub {
        /// `Ok(Some(_))` — an authoritative typed value.
        Typed(i64),
        /// `Ok(Some(vec![]))` — an authoritatively empty typed value
        /// (e.g. an `xsi:nil` element).
        Empty,
        /// `Ok(None)` — no opinion; the caller falls back to untyped.
        NoOpinion,
        /// `Err(_)` — the provider rejects atomization of the node.
        Failing,
    }

    impl NodeTypedValueProvider for Stub {
        fn typed_value(
            &self,
            _xot: &Xot,
            _node: xot::Node,
        ) -> error::Result<Option<Vec<atomic::Atomic>>> {
            match self {
                Stub::Typed(n) => Ok(Some(vec![atomic::Atomic::from(*n)])),
                Stub::Empty => Ok(Some(vec![])),
                Stub::NoOpinion => Ok(None),
                Stub::Failing => Err(error::Error::XPTY0004),
            }
        }
    }

    fn node_sequence() -> (Xot, Sequence) {
        let mut xot = Xot::new();
        let root = xot.parse("<doc>untyped text</doc>").unwrap();
        let doc = xot.document_element(root).unwrap();
        let seq: Sequence = vec![Item::from(doc)].into();
        (xot, seq)
    }

    #[test]
    fn provider_supplies_the_node_typed_value() {
        let (xot, seq) = node_sequence();
        let stub: &dyn NodeTypedValueProvider = &Stub::Typed(42);
        let atomized = seq
            .atomized(Some(stub), &xot)
            .collect::<error::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(atomized, vec![atomic::Atomic::from(42i64)]);
    }

    #[test]
    fn provider_no_opinion_falls_back_to_untyped() {
        let (xot, seq) = node_sequence();
        let stub: &dyn NodeTypedValueProvider = &Stub::NoOpinion;
        let atomized = seq
            .atomized(Some(stub), &xot)
            .collect::<error::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            atomized,
            vec![atomic::Atomic::Untyped("untyped text".into())]
        );
    }

    #[test]
    fn provider_error_propagates() {
        let (xot, seq) = node_sequence();
        let stub: &dyn NodeTypedValueProvider = &Stub::Failing;
        let result = seq
            .atomized(Some(stub), &xot)
            .collect::<error::Result<Vec<_>>>();
        assert_eq!(result, Err(error::Error::XPTY0004));
    }

    #[test]
    fn provider_empty_typed_value_yields_no_atomics() {
        // Ok(Some(vec![])) is an authoritatively empty typed value (e.g.
        // an xsi:nil element): the node contributes zero atomics. This
        // breaks the old "a node always atomizes to exactly one atomic".
        let (xot, seq) = node_sequence();
        let stub: &dyn NodeTypedValueProvider = &Stub::Empty;
        let atomized = seq
            .atomized(Some(stub), &xot)
            .collect::<error::Result<Vec<_>>>()
            .unwrap();
        assert!(atomized.is_empty());
    }

    /// A provider that rejects the node whose string value is `"boom"`
    /// and supplies a typed value for every other node — for checking
    /// that per-node results land in document order.
    struct ByContent;

    impl NodeTypedValueProvider for ByContent {
        fn typed_value(
            &self,
            xot: &Xot,
            node: xot::Node,
        ) -> error::Result<Option<Vec<atomic::Atomic>>> {
            if xot.string_value(node) == "boom" {
                Err(error::Error::XPTY0004)
            } else {
                Ok(Some(vec![atomic::Atomic::from(true)]))
            }
        }
    }

    #[test]
    fn provider_results_keep_node_order() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<doc><a>ok</a><b>boom</b><c>ok</c></doc>")
            .unwrap();
        let doc = xot.document_element(root).unwrap();
        let a = xot.first_child(doc).unwrap();
        let b = xot.next_sibling(a).unwrap();
        let c = xot.next_sibling(b).unwrap();
        let seq: Sequence = vec![Item::from(a), Item::from(b), Item::from(c)].into();

        let stub: &dyn NodeTypedValueProvider = &ByContent;
        let results: Vec<_> = seq.atomized(Some(stub), &xot).collect();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Ok(atomic::Atomic::from(true)));
        assert!(matches!(results[1], Err(error::Error::XPTY0004)));
        assert_eq!(results[2], Ok(atomic::Atomic::from(true)));
    }
}
