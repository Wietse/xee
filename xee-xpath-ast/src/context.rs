use xee_name::VariableNames;

use crate::ast;
use crate::{Namespaces, ParserError};

/// Which dialect of the XPath grammar to parse.
///
/// Xee parses standard XPath 3.1 by default. [`XPathDialect::XbrlFormula`]
/// additionally recognises the XBRL Formula expression dialect: a bare `INF`
/// and `NaN` are `xs:double` numeric literals. Standard XPath has no such
/// literals — there `INF` and `NaN` are ordinary names — so the extension is
/// opt-in and standard parsing stays spec-conformant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XPathDialect {
    /// Standard XPath 3.1.
    #[default]
    Standard,
    /// The XBRL Formula expression dialect. Currently its only grammar
    /// extension over [`XPathDialect::Standard`] is that a bare `INF` / `NaN`
    /// lexes as an `xs:double` literal.
    XbrlFormula,
}

#[derive(Debug, Default)]
pub struct XPathParserContext {
    pub namespaces: Namespaces,
    pub variable_names: VariableNames,
    /// The grammar dialect expressions are parsed with.
    pub dialect: XPathDialect,
}

impl XPathParserContext {
    /// Construct a new XPath parser context.
    ///
    /// This consists of information about namespaces and variable names
    /// available. The grammar dialect defaults to [`XPathDialect::Standard`];
    /// set the [`XPathParserContext::dialect`] field to change it.
    pub fn new(namespaces: Namespaces, variable_names: VariableNames) -> Self {
        Self {
            namespaces,
            variable_names,
            dialect: XPathDialect::Standard,
        }
    }

    /// Given an XPath string, parse into an XPath AST
    ///
    /// This uses the namespaces and variable names with which
    /// this static context has been initialized.
    pub fn parse_xpath(&self, s: &str) -> Result<ast::XPath, ParserError> {
        ast::XPath::parse_with_dialect(s, &self.namespaces, &self.variable_names, self.dialect)
    }

    /// Given an XSLT pattern, parse into an AST
    pub fn parse_pattern(&self, s: &str) -> Result<crate::Pattern<ast::ExprS>, ParserError> {
        crate::Pattern::parse(s, &self.namespaces, &self.variable_names)
    }

    /// Parse an XPath string as it would appear in an XSLT value template.
    /// This means it should have a closing `}` following the xpath expression.
    pub fn parse_value_template_xpath(&self, s: &str) -> Result<ast::XPath, ParserError> {
        ast::XPath::parse_value_template(s, &self.namespaces, &self.variable_names)
    }
}
