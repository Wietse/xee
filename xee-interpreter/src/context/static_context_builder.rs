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
    /// Descriptions are typically produced via [`crate::wrap_xpath_fn!`]
    /// from a Rust function annotated with `#[xpath_fn]`. The signature
    /// is parsed at [`Self::build`] time against the builder's namespace
    /// map, so user prefixes (e.g. `xfi:`) work as long as the prefix
    /// has been registered via [`Self::namespaces`] /
    /// [`Self::add_namespace`].
    ///
    /// Built-in functions defined by the XPath/XSLT specs take
    /// precedence; an extension that uses the same `(name, arity)` is
    /// never reachable.
    pub fn add_function(
        &mut self,
        description: function::StaticFunctionDescription,
    ) -> &mut Self {
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
    /// [`Self::add_function`] has a signature that fails to parse
    /// against the builder's namespace map (e.g. uses an undeclared
    /// prefix).
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
                static_context.builtin_function_count(),
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
}
