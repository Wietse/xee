use xee_xpath_ast::{ast::Span, span::Spanned};

use crate::{ir, Variables};

/// A binding consists of a unique variable name and an expression.
#[derive(Debug, Clone)]
pub struct Binding {
    name: ir::Name,
    expr: ir::Expr,
    span: Span,
}

impl Binding {
    #[inline]
    pub fn new(name: ir::Name, expr: ir::Expr, span: Span) -> Self {
        Self { name, expr, span }
    }
}

#[derive(Debug, Clone)]
pub struct Bindings {
    bindings: Vec<Binding>,
}

impl Bindings {
    pub fn new(binding: Binding) -> Self {
        Self {
            bindings: vec![binding],
        }
    }

    pub fn empty() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Create an atom
    /// Takes the last added binding
    /// If it's already an atom, return it, and pops it from the bindings.
    /// If it's not atom, create a variable based on its name and
    /// return that as an atom.
    pub fn atom(&mut self) -> ir::AtomS {
        let last = self.bindings.last().unwrap();
        let (want_pop, atom) = match &last.expr {
            ir::Expr::Atom(atom) => (true, atom.clone()),
            _ => (
                false,
                Spanned::new(ir::Atom::Variable(last.name.clone()), last.span),
            ),
        };
        if want_pop {
            self.bindings.pop();
        }
        atom
    }

    /// Given bindings, return a let expression.
    /// This takes all the bindings and wraps it in a let expression.
    ///
    /// Consumes the bindings: their expressions move into the result.
    /// Cloning them copied every binding's whole expression each time a
    /// sub-expression was lowered, quadratic work in the expression's size.
    pub fn expr(mut self) -> ir::ExprS {
        let last_binding = self.bindings.pop().unwrap();
        let span = last_binding.span;
        Spanned::new(
            self.bindings
                .into_iter()
                .rev()
                .fold(last_binding.expr, |expr, binding| {
                    ir::Expr::Let(ir::Let {
                        name: binding.name,
                        var_expr: Box::new(Spanned::new(binding.expr, binding.span)),
                        return_expr: Box::new(Spanned::new(expr, span)),
                    })
                }),
            span,
        )
    }

    pub fn atom_bindings(mut self) -> (ir::AtomS, Self) {
        let atom = self.atom();
        (atom, self)
    }

    pub fn bind_expr(self, variables: &mut Variables, expr: ir::ExprS) -> Self {
        let binding = variables.new_binding(expr.value, expr.span);
        self.bind(binding)
    }

    pub fn bind_expr_no_span(self, variables: &mut Variables, expr: ir::Expr) -> Self {
        let binding = variables.new_binding(expr, (0..0).into());
        self.bind(binding)
    }

    /// Add a binding. Consumes the bindings, like [`Self::concat`]:
    /// copying the accumulated bindings on every addition made lowering a
    /// sequence of n items quadratic.
    pub fn bind(mut self, binding: Binding) -> Self {
        self.bindings.push(binding);
        self
    }

    /// Concatenate one bindings object with another.
    pub fn concat(mut self, bindings: Bindings) -> Self {
        self.bindings.extend(bindings.bindings);
        self
    }
}
