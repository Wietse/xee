use ahash::{AHashMap, HashMap};
use iri_string::types::{IriStr, IriString};
use std::any::Any;
use std::fmt::{self, Debug};
use std::sync::Arc;

use crate::atomic::Atomic;
use crate::function::{self, Function};
use crate::{error::Error, interpreter::Program};
use crate::{interpreter, sequence};

use super::{DocumentsRef, StaticContext};

/// Host-provided state attached to a [`DynamicContext`], stored
/// type-erased. Wraps `Arc<dyn Any + Send + Sync>` behind a newtype so
/// the context's `Debug` derive still works (a bare `dyn Any` is not
/// `Debug`).
#[derive(Clone)]
pub(crate) struct UserData(Arc<dyn Any + Send + Sync>);

impl UserData {
    pub(crate) fn new<T: Any + Send + Sync>(value: Arc<T>) -> Self {
        Self(value)
    }
}

impl Debug for UserData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UserData(..)")
    }
}

/// A host-provided source of typed values for nodes, consulted during
/// atomization.
///
/// xee is schema-unaware: by default it atomizes every node as
/// `xs:untypedAtomic(string-value(node))`. A host that knows a node's
/// real typed value — for example from an XBRL DTS — installs a
/// `NodeTypedValueProvider` on the [`DynamicContext`] to supply it.
///
/// The trait deliberately does not receive the [`DynamicContext`]: the
/// provider is owned by the context, so passing it back in is awkward.
// The `Send + Sync` bound buys no thread-safety today (`DynamicContext` is
// `!Send` — it holds `Rc`), but keep it, mirroring the `user_data` slot:
// relaxing a bound later is non-breaking, tightening it is not.
pub trait NodeTypedValueProvider: Send + Sync {
    /// The typed value of `node`, if the provider has one.
    ///
    /// - `Ok(Some(values))` — an authoritative typed value, as an XDM
    ///   atomic sequence. `Ok(Some(vec![]))` is an authoritatively empty
    ///   typed value (e.g. an `xsi:nil` element).
    /// - `Ok(None)` — the provider has no opinion; the caller falls back
    ///   to `xs:untypedAtomic(string-value(node))`.
    /// - `Err(e)` — atomization of `node` fails with an XPath error.
    fn typed_value(&self, xot: &xot::Xot, node: xot::Node) -> Result<Option<Vec<Atomic>>, Error>;
}

/// Wraps the host's [`NodeTypedValueProvider`] behind a newtype with a
/// hand-written `Debug`, so [`DynamicContext`]'s `Debug` derive still
/// holds (a bare `dyn` trait object is not `Debug`). Mirrors [`UserData`].
#[derive(Clone)]
pub(crate) struct TypedValueProviderSlot(Arc<dyn NodeTypedValueProvider>);

impl TypedValueProviderSlot {
    pub(crate) fn new(provider: Arc<dyn NodeTypedValueProvider>) -> Self {
        Self(provider)
    }

    pub(crate) fn get(&self) -> &dyn NodeTypedValueProvider {
        self.0.as_ref()
    }
}

impl Debug for TypedValueProviderSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TypedValueProviderSlot(..)")
    }
}

/// A host-provided source for an element node's `[nilled]` PSVI
/// property, consulted by `fn:nilled`.
///
/// xee is schema-unaware: by default every element is reported as
/// not nilled. A host that knows a node's PSVI nilled status — for
/// example an XBRL processor treating wire-format `xsi:nil="true"`
/// as authoritative — installs a `NodeNilledProvider` on the
/// [`DynamicContext`] to supply it.
///
/// Parallel in shape to [`NodeTypedValueProvider`]: a single
/// `Arc<H>` can implement both traits and be installed twice on the
/// same context. They are deliberately separate traits because the
/// XDM `[typed-value]` and `[nilled]` properties are independent
/// PSVI properties, and a host may know one without the other.
// The `Send + Sync` bound buys no thread-safety today (`DynamicContext`
// is `!Send` — it holds `Rc`), but keep it for symmetry with
// `NodeTypedValueProvider` and `user_data`: relaxing a bound later is
// non-breaking, tightening it is not.
pub trait NodeNilledProvider: Send + Sync {
    /// The `[nilled]` property of `node`, if the provider has one.
    ///
    /// - `Some(true)` — the node is authoritatively nilled.
    /// - `Some(false)` — the node is authoritatively not nilled.
    /// - `None` — the provider has no opinion; the caller falls back
    ///   to xee's schema-unaware default (not nilled).
    ///
    /// `fn:nilled` only calls this for element nodes; non-element
    /// arguments are handled before reaching the provider.
    fn nilled(&self, xot: &xot::Xot, node: xot::Node) -> Option<bool>;
}

/// Wraps the host's [`NodeNilledProvider`] behind a newtype with a
/// hand-written `Debug`, parallel to [`TypedValueProviderSlot`].
#[derive(Clone)]
pub(crate) struct NilledProviderSlot(Arc<dyn NodeNilledProvider>);

impl NilledProviderSlot {
    pub(crate) fn new(provider: Arc<dyn NodeNilledProvider>) -> Self {
        Self(provider)
    }

    pub(crate) fn get(&self) -> &dyn NodeNilledProvider {
        self.0.as_ref()
    }
}

impl Debug for NilledProviderSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NilledProviderSlot(..)")
    }
}

/// A map of variables
///
/// These are variables to be passed into an XPath evaluation.
///
/// The key is the name of a variable, and the value is an item.
pub type Variables = AHashMap<xot::xmlname::OwnedName, sequence::Sequence>;

// a dynamic context is created for each xpath evaluation
#[derive(Debug)]
pub struct DynamicContext<'a> {
    // we keep a reference to the program
    program: &'a Program,

    /// An optional context item
    context_item: Option<sequence::Item>,
    // we want to mutate documents during evaluation, and this happens in
    // multiple spots. We use RefCell to manage that during runtime so we don't
    // need to make the whole thing immutable.
    documents: DocumentsRef,
    variables: Variables,
    // TODO: we want to be able to control the creation of this outside,
    // as it needs to be the same for all evalutions of XSLT I believe
    current_datetime: chrono::DateTime<chrono::offset::FixedOffset>,
    // default collection
    default_collection: Option<sequence::Sequence>,
    // collections
    collections: HashMap<IriString, sequence::Sequence>,
    // default uri collection
    default_uri_collection: Option<sequence::Sequence>,
    // uri collections
    uri_collections: HashMap<IriString, sequence::Sequence>,
    // environment variables
    environment_variables: HashMap<String, String>,
    // a single typed slot of host-provided state, reachable from
    // extension functions via the typed `user_data` accessor
    user_data: Option<UserData>,
    // a host-provided source of typed node values, consulted during
    // atomization; absent means the default xs:untypedAtomic behavior
    typed_value_provider: Option<TypedValueProviderSlot>,
    // a host-provided source for an element's [nilled] PSVI property,
    // consulted by fn:nilled; absent means the default (not nilled)
    nilled_provider: Option<NilledProviderSlot>,
}

impl<'a> DynamicContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        program: &'a Program,
        context_item: Option<sequence::Item>,
        documents: DocumentsRef,
        variables: Variables,
        current_datetime: chrono::DateTime<chrono::offset::FixedOffset>,
        default_collection: Option<sequence::Sequence>,
        collections: HashMap<IriString, sequence::Sequence>,
        default_uri_collection: Option<sequence::Sequence>,
        uri_collections: HashMap<IriString, sequence::Sequence>,
        environment_variables: HashMap<String, String>,
        user_data: Option<UserData>,
        typed_value_provider: Option<TypedValueProviderSlot>,
        nilled_provider: Option<NilledProviderSlot>,
    ) -> Self {
        Self {
            program,
            context_item,
            documents,
            variables,
            current_datetime,
            default_collection,
            collections,
            default_uri_collection,
            uri_collections,
            environment_variables,
            user_data,
            typed_value_provider,
            nilled_provider,
        }
    }

    /// The static context of the program.
    pub fn static_context(&self) -> &StaticContext {
        self.program.static_context()
    }

    /// Access the context item, if any.
    pub fn context_item(&self) -> Option<&sequence::Item> {
        self.context_item.as_ref()
    }

    /// The documents in this context.
    pub fn documents(&self) -> DocumentsRef {
        self.documents.clone()
    }

    /// The variables in this context.
    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    /// Access the default collection
    pub fn default_collection(&self) -> Option<&sequence::Sequence> {
        self.default_collection.as_ref()
    }

    /// Access a collection by URI
    pub fn collection(&self, uri: &IriStr) -> Option<&sequence::Sequence> {
        self.collections.get(uri)
    }

    /// Access the default URI collection
    pub fn default_uri_collection(&self) -> Option<&sequence::Sequence> {
        self.default_uri_collection.as_ref()
    }

    /// Access a URI collection by URI
    ///
    /// Note that the URI does not have to be a proper URI as the specification
    /// defines it as an xs:string
    pub fn uri_collection(&self, uri: &IriStr) -> Option<&sequence::Sequence> {
        self.uri_collections.get(uri)
    }

    /// Access an environment variable by name
    pub fn environment_variable(&self, name: &str) -> Option<&str> {
        self.environment_variables.get(name).map(String::as_str)
    }

    /// Access the host-provided user data, downcast to `T`.
    ///
    /// Returns `None` if no user data was set on the
    /// [`super::DynamicContextBuilder`], or if it was set to a type
    /// other than `T`. Extension functions use this to reach host
    /// state (e.g. an XBRL DTS handle) through the `&DynamicContext`
    /// they are passed.
    pub fn user_data<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.user_data.as_ref()?.0.downcast_ref::<T>()
    }

    /// Access the host-provided typed-value provider, if one was set.
    ///
    /// Atomization consults this for a node's typed value before falling
    /// back to `xs:untypedAtomic(string-value(node))`. Returns `None`
    /// when no provider was installed on the
    /// [`super::DynamicContextBuilder`]. This is a separate slot from
    /// [`Self::user_data`]: a node's typed value is core evaluator
    /// semantics, not extension-function host state.
    pub fn typed_value_provider(&self) -> Option<&dyn NodeTypedValueProvider> {
        self.typed_value_provider.as_ref().map(|p| p.get())
    }

    /// Access the host-provided nilled provider, if one was set.
    ///
    /// `fn:nilled` consults this for an element node's `[nilled]` PSVI
    /// property before falling back to the schema-unaware default (not
    /// nilled). Returns `None` when no provider was installed on the
    /// [`super::DynamicContextBuilder`]. Parallel slot to
    /// [`Self::typed_value_provider`] — see [`NodeNilledProvider`] for
    /// why the two are separate.
    pub fn nilled_provider(&self) -> Option<&dyn NodeNilledProvider> {
        self.nilled_provider.as_ref().map(|p| p.get())
    }

    /// Access all environment variable names
    pub fn environment_variable_names(&self) -> impl Iterator<Item = &str> {
        self.environment_variables.keys().map(String::as_str)
    }

    pub(crate) fn arguments(&self) -> Result<Vec<sequence::Sequence>, Error> {
        let mut arguments = Vec::new();
        for variable_name in self.static_context().variable_names() {
            let items = self.variables.get(variable_name).ok_or(Error::XPDY0002)?;
            arguments.push(items.clone());
        }
        Ok(arguments)
    }

    fn create_current_datetime() -> chrono::DateTime<chrono::offset::FixedOffset> {
        chrono::offset::Local::now().into()
    }

    pub(crate) fn current_datetime(&self) -> chrono::DateTime<chrono::offset::FixedOffset> {
        self.current_datetime
    }

    pub fn implicit_timezone(&self) -> chrono::FixedOffset {
        self.current_datetime.timezone()
    }

    /// Access information about a Function.
    pub fn function_info<'b>(&self, function: &'b Function) -> interpreter::FunctionInfo<'a, 'b> {
        self.program.function_info(function)
    }

    pub(crate) fn static_function_by_id(
        &self,
        id: function::StaticFunctionId,
    ) -> &function::StaticFunction {
        self.program.static_context().function_by_id(id)
    }

    pub(crate) fn inline_function_by_id(
        &self,
        id: function::InlineFunctionId,
    ) -> &function::InlineFunction {
        self.program.inline_function(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::StaticContext;
    use crate::interpreter::Program;

    #[derive(Debug)]
    struct HostState {
        label: String,
    }

    fn empty_program() -> Program {
        Program::new(StaticContext::default(), (0..0).into())
    }

    #[test]
    fn user_data_round_trips_typed() {
        let program = empty_program();
        let mut builder = program.dynamic_context_builder();
        builder.user_data(Arc::new(HostState {
            label: "dts".to_string(),
        }));
        let context = builder.build();

        let state = context
            .user_data::<HostState>()
            .expect("user data should be present");
        assert_eq!(state.label, "dts");
    }

    #[test]
    fn user_data_wrong_type_returns_none() {
        let program = empty_program();
        let mut builder = program.dynamic_context_builder();
        builder.user_data(Arc::new(HostState {
            label: "dts".to_string(),
        }));
        let context = builder.build();

        assert!(context.user_data::<String>().is_none());
    }

    #[test]
    fn user_data_unset_returns_none() {
        let program = empty_program();
        let builder = program.dynamic_context_builder();
        let context = builder.build();

        assert!(context.user_data::<HostState>().is_none());
    }

    #[test]
    fn user_data_last_write_wins() {
        let program = empty_program();
        let mut builder = program.dynamic_context_builder();
        builder.user_data(Arc::new(HostState {
            label: "first".to_string(),
        }));
        builder.user_data(Arc::new(HostState {
            label: "second".to_string(),
        }));
        let context = builder.build();

        let state = context
            .user_data::<HostState>()
            .expect("user data should be present");
        assert_eq!(state.label, "second");
    }

    struct StubProvider {
        label: &'static str,
    }

    impl NodeTypedValueProvider for StubProvider {
        fn typed_value(
            &self,
            _xot: &xot::Xot,
            _node: xot::Node,
        ) -> Result<Option<Vec<Atomic>>, Error> {
            Ok(Some(vec![Atomic::from(self.label)]))
        }
    }

    fn stub_node() -> (xot::Xot, xot::Node) {
        let mut xot = xot::Xot::new();
        let root = xot.parse("<a/>").expect("valid XML");
        (xot, root)
    }

    #[test]
    fn typed_value_provider_unset_returns_none() {
        let program = empty_program();
        let builder = program.dynamic_context_builder();
        let context = builder.build();

        assert!(context.typed_value_provider().is_none());
    }

    #[test]
    fn typed_value_provider_round_trips() {
        let program = empty_program();
        let mut builder = program.dynamic_context_builder();
        builder.typed_value_provider(Arc::new(StubProvider { label: "dts" }));
        let context = builder.build();

        let (xot, node) = stub_node();
        let value = context
            .typed_value_provider()
            .expect("provider should be present")
            .typed_value(&xot, node)
            .expect("provider should not error");
        assert_eq!(value, Some(vec![Atomic::from("dts")]));
    }

    #[test]
    fn typed_value_provider_last_write_wins() {
        let program = empty_program();
        let mut builder = program.dynamic_context_builder();
        builder.typed_value_provider(Arc::new(StubProvider { label: "first" }));
        builder.typed_value_provider(Arc::new(StubProvider { label: "second" }));
        let context = builder.build();

        let (xot, node) = stub_node();
        let value = context
            .typed_value_provider()
            .unwrap()
            .typed_value(&xot, node)
            .unwrap();
        assert_eq!(value, Some(vec![Atomic::from("second")]));
    }
}
