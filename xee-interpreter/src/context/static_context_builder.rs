use std::sync::Arc;

use ahash::HashMap;
use iri_string::types::IriAbsoluteString;
use xee_name::Namespaces;
use xot::xmlname::OwnedName;

use crate::context;
use crate::error;
use crate::function;

#[derive(Debug, Default)]
pub struct StaticContextBuilder<'a> {
    variable_names: Vec<OwnedName>,
    namespaces: HashMap<&'a str, &'a str>,
    default_element_namespace: &'a str,
    default_function_namespace: &'a str,
    static_base_uri: Option<IriAbsoluteString>,
    extension_descriptions: Vec<function::StaticFunctionDescription>,
}

impl<'a> StaticContextBuilder<'a> {
    /// Set the variable names that the XPath expression expects.
    ///
    /// They should be supplied in the order that they would be passed into if
    /// the XPath expression was a function.
    ///
    /// Calling this multiple times will override the variable names.
    pub fn variable_names(
        &mut self,
        variable_names: impl IntoIterator<Item = OwnedName>,
    ) -> &mut Self {
        self.variable_names = variable_names.into_iter().collect();
        self
    }

    /// Set the namespace prefixes that the XPath expression can use.
    ///
    /// This is an iterable of tuples where the first element is the prefix and
    /// the second element is the namespace URI.
    ///
    /// If a prefix is empty, it sets the default namespace.
    ///
    /// Calling this multiple times will override the namespaces.
    pub fn namespaces(
        &mut self,
        namespaces: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> &mut Self {
        for (prefix, uri) in namespaces {
            self.add_namespace(prefix, uri);
        }
        self
    }

    /// Add a namespace prefix that the XPath expression can use.
    pub fn add_namespace(&mut self, prefix: &'a str, uri: &'a str) -> &mut Self {
        if prefix.is_empty() {
            self.default_element_namespace = uri;
        } else {
            self.namespaces.insert(prefix, uri);
        }
        self
    }

    /// Set the default namespace for element references in the XPath expression.
    pub fn default_element_namespace(&mut self, default_element_namespace: &'a str) -> &mut Self {
        self.default_element_namespace = default_element_namespace;
        self
    }

    /// Set the default namespace for function references in the XPath expression.
    pub fn default_function_namespace(&mut self, default_function_namespace: &'a str) -> &mut Self {
        self.default_function_namespace = default_function_namespace;
        self
    }

    /// Set the static base URI
    pub fn static_base_uri(&mut self, static_base_uri: Option<IriAbsoluteString>) -> &mut Self {
        self.static_base_uri = static_base_uri;
        self
    }

    /// Register a host-provided XPath function.
    ///
    /// Descriptions are produced via [`crate::wrap_xpath_fn!`] from a
    /// Rust function annotated with `#[xpath_fn]`. In the `#[xpath_fn]`
    /// signature, name the function with a braced-URI EQName —
    /// `Q{http://example.com/ns}local` — for any namespace outside the
    /// XPath defaults. `#[xpath_fn]` parses the signature against the
    /// standard namespaces at macro-expansion time, so a *prefixed*
    /// QName (`myns:local`) is rejected there, before [`Self::build`]
    /// ever runs; a braced URI carries the namespace literally and has
    /// no such problem.
    ///
    /// [`Self::add_namespace`] governs the prefixes a *query author*
    /// may use in an XPath expression; it does not affect how
    /// `#[xpath_fn]` signatures are parsed.
    ///
    /// Built-in functions defined by the XPath/XSLT specs may not be
    /// shadowed: registering an extension whose `(name, arity)` matches
    /// a built-in — or matches another registered extension — makes
    /// [`Self::build`] return an error.
    pub fn add_function(&mut self, description: function::StaticFunctionDescription) -> &mut Self {
        self.extension_descriptions.push(description);
        self
    }

    /// Register multiple host-provided XPath functions. See
    /// [`Self::add_function`].
    pub fn add_functions(
        &mut self,
        descriptions: impl IntoIterator<Item = function::StaticFunctionDescription>,
    ) -> &mut Self {
        self.extension_descriptions.extend(descriptions);
        self
    }

    /// Build the static context.
    ///
    /// This will always include the default known namespaces for
    /// XPath, and the default function namespace will be the `fn` namespace
    /// if not set.
    ///
    /// Returns an error if an extension function registered via
    /// [`Self::add_function`] has a signature that fails to parse, or
    /// whose `(name, arity)` collides with a built-in function or with
    /// another registered extension.
    pub fn build(&self) -> error::Result<context::StaticContext> {
        let mut namespaces = Namespaces::default_namespaces();
        for (prefix, uri) in &self.namespaces {
            namespaces.insert(prefix.to_string(), uri.to_string());
        }
        let default_function_namespace = if !self.default_function_namespace.is_empty() {
            self.default_function_namespace
        } else {
            Namespaces::FN_NAMESPACE
        };
        let namespaces = xee_name::Namespaces::new(
            namespaces,
            self.default_element_namespace.to_string(),
            default_function_namespace.to_string(),
        );
        let variable_names = self.variable_names.clone().into_iter().collect();
        let mut static_context =
            context::StaticContext::new(namespaces, variable_names, self.static_base_uri.clone());
        if !self.extension_descriptions.is_empty() {
            let extensions = function::ExtensionFunctions::build(
                &self.extension_descriptions,
                static_context.namespaces(),
                static_context.builtin_functions(),
            )?;
            static_context.set_extension_functions(Arc::new(extensions));
        }
        Ok(static_context)
    }
}

#[cfg(test)]
mod tests {
    use ahash::HashSet;

    use super::*;

    #[test]
    fn test_variable_names() {
        let mut builder = StaticContextBuilder::default();
        let foo = OwnedName::new("foo".to_string(), "".to_string(), "".to_string());
        let bar = OwnedName::new("bar".to_string(), "".to_string(), "".to_string());
        builder.variable_names([foo.clone(), bar.clone()]);
        assert_eq!(builder.variable_names, vec![foo, bar]);
    }

    #[test]
    fn test_default_behavior() {
        let builder = StaticContextBuilder::default();
        let static_context = builder.build().unwrap();
        assert_eq!(static_context.namespaces().default_element_namespace(), "");
        assert_eq!(
            static_context.namespaces().default_function_namespace,
            Namespaces::FN_NAMESPACE
        );
        assert_eq!(static_context.variable_names(), &HashSet::default());
        assert_eq!(
            static_context.namespaces().by_prefix("xml"),
            Some("http://www.w3.org/XML/1998/namespace")
        );
    }

    mod extension_function_registry {
        use super::*;
        use crate::function::StaticFunctionDescription;
        use crate::wrap_xpath_fn;
        use ibig::IBig;
        use xee_xpath_macros::xpath_fn;

        const XFI_NS: &str = "http://www.xbrl.org/2008/function/instance";

        // Extension functions name themselves with `Q{uri}local`
        // notation. The braced URI is a literal, so it carries no
        // prefix to resolve — the `#[xpath_fn]` macro parses the
        // signature at expansion time against `DEFAULT_NAMESPACES`
        // without any extra namespace setup, and the parameter/return
        // types stay in the standard `xs:` / untyped namespaces.
        #[xpath_fn(
            "Q{http://www.xbrl.org/2008/function/instance}add($a as xs:integer, $b as xs:integer) as xs:integer"
        )]
        fn xfi_add(a: IBig, b: IBig) -> IBig {
            a + b
        }

        #[xpath_fn(
            "Q{http://www.xbrl.org/2008/function/instance}identity($n as xs:integer) as xs:integer"
        )]
        fn xfi_identity(n: IBig) -> IBig {
            n
        }

        fn xfi_name(local: &str) -> OwnedName {
            OwnedName::new(local.to_string(), XFI_NS.to_string(), "".to_string())
        }

        fn dummy_func(
            _: &context::DynamicContext,
            _: &mut crate::interpreter::Interpreter,
            _: &[crate::sequence::Sequence],
        ) -> crate::error::Result<crate::sequence::Sequence> {
            Ok(crate::sequence::Sequence::default())
        }

        #[test]
        fn macro_defined_extension_resolves_to_extension_id() {
            let mut builder = StaticContextBuilder::default();
            builder.add_function(wrap_xpath_fn!(xfi_add));
            // No `add_namespace` call: the `Q{uri}` signature resolves
            // without the host registering anything for the signature.
            let ctx = builder.build().expect("build should succeed");

            let id = ctx
                .function_id_by_name(&xfi_name("add"), 2)
                .expect("extension must resolve by name");
            assert!(
                (id.as_u16() as usize) >= ctx.builtin_function_count(),
                "extension id must live above the built-in count"
            );
            let f = ctx.function_by_id(id);
            assert_eq!(f.arity(), 2);
        }

        #[test]
        fn registers_multiple_functions_at_distinct_ids() {
            let mut builder = StaticContextBuilder::default();
            builder.add_functions([wrap_xpath_fn!(xfi_add), wrap_xpath_fn!(xfi_identity)]);
            let ctx = builder.build().unwrap();

            let id_add = ctx.function_id_by_name(&xfi_name("add"), 2).unwrap();
            let id_identity = ctx.function_id_by_name(&xfi_name("identity"), 1).unwrap();
            assert_ne!(id_add.as_u16(), id_identity.as_u16());
        }

        #[test]
        fn extension_colliding_with_builtin_is_a_build_error() {
            let mut builder = StaticContextBuilder::default();
            // fn:abs#1 is a built-in — registering it as an extension
            // could never be reachable, so build() must reject it.
            builder.add_function(StaticFunctionDescription::new(
                dummy_func,
                "Q{http://www.w3.org/2005/xpath-functions}abs($n as xs:integer) as xs:integer",
                None,
            ));
            assert!(builder.build().is_err());
        }

        #[test]
        fn duplicate_extension_registration_is_a_build_error() {
            // Two different functions claiming the same (name, arity)
            // would make dispatch order-dependent — build() rejects it.
            let mut builder = StaticContextBuilder::default();
            builder.add_functions([
                StaticFunctionDescription::new(
                    dummy_func,
                    "Q{http://example.com/ext}add($a as xs:integer) as xs:integer",
                    None,
                ),
                StaticFunctionDescription::new(
                    dummy_func,
                    "Q{http://example.com/ext}add($a as xs:integer) as xs:integer",
                    None,
                ),
            ]);
            assert!(builder.build().is_err());
        }

        #[test]
        fn malformed_signature_surfaces_as_build_error() {
            // A truncated signature can't be parsed; `build()` reports
            // it rather than panicking.
            let mut builder = StaticContextBuilder::default();
            builder.add_function(StaticFunctionDescription::new(
                dummy_func,
                "Q{http://example.com/}broken($a as xs:integer",
                None,
            ));
            assert!(builder.build().is_err());
        }
    }
}
