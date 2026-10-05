use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};

use crate::Name;
use crate::{ast, span::WithSpan, VariableNames};

use super::visitor::AstVisitor;

struct UniqueNameGenerator {
    names: HashSet<Name>,
    // For each base name, how many `*` suffixes the next candidate starts
    // with.
    suffixes: HashMap<Name, usize>,
}

impl UniqueNameGenerator {
    fn new() -> Self {
        UniqueNameGenerator {
            names: HashSet::new(),
            suffixes: HashMap::new(),
        }
    }

    fn generate(&mut self, name: &Name) -> Name {
        // The names generated for a base are the base followed by `*` zero,
        // one, two... times, taken in order, so all the shorter ones are
        // already taken: start after them. Testing each in turn hashed
        // names of growing length, cubic in the number of times one name is
        // shadowed (8,192 nested `let $x` took 22 s in release). The loop
        // still checks, so the name chosen is the same first free one as
        // before. The k-th name is k characters longer than its base, and
        // building and hashing it reads them all, so n shadowings still
        // take time quadratic in n: the size of the names produced.
        let mut suffixes = self.suffixes.get(name).copied().unwrap_or(0);
        let mut candidate = name.clone();
        for _ in 0..suffixes {
            candidate = candidate.with_suffix();
        }
        while self.names.contains(&candidate) {
            candidate = candidate.with_suffix();
            suffixes += 1;
        }
        self.suffixes.insert(name.clone(), suffixes + 1);
        self.names.insert(candidate.clone());
        candidate
    }
}

struct Names {
    names: Vec<(Name, Name)>,
    generator: UniqueNameGenerator,
}

impl Names {
    fn new() -> Self {
        Names {
            names: Vec::new(),
            generator: UniqueNameGenerator::new(),
        }
    }

    fn get(&mut self, name: &Name) -> Name {
        // this always returns a name, even if the
        // name is unknown, in which case a unique bogus
        // name is generated
        self.names
            .iter()
            .rev()
            .find(|(old_name, _)| old_name == name)
            .map(|(_, new_name)| new_name.clone())
            .unwrap_or_else(|| self.generator.generate(name))
    }

    fn push_name(&mut self, name: &Name) -> Name {
        let new_name = self.generator.generate(name);
        self.names.push((name.clone(), new_name.clone()));
        new_name
    }

    fn pop_name(&mut self) {
        self.names.pop();
    }
}

struct Renamer {
    names: Names,
}

impl Renamer {
    fn new() -> Self {
        Renamer {
            names: Names::new(),
        }
    }

    fn push_name(&mut self, name: &Name) -> Name {
        self.names.push_name(name)
    }

    fn pop_name(&mut self) {
        self.names.pop_name();
    }
}

impl AstVisitor for Renamer {
    fn visit_let_expr(&mut self, expr: &mut ast::LetExpr) {
        self.visit_expr_single(&mut expr.var_expr);
        let old_span = expr.var_name.span;
        expr.var_name = self.push_name(&expr.var_name.value).with_span(old_span);
        self.visit_expr_single(&mut expr.return_expr);
        self.pop_name();
    }

    fn visit_for_expr(&mut self, expr: &mut ast::ForExpr) {
        self.visit_expr_single(&mut expr.var_expr);
        let old_span = expr.var_name.span;
        expr.var_name = self.push_name(&expr.var_name.value).with_span(old_span);
        self.visit_expr_single(&mut expr.return_expr);
        self.pop_name();
    }

    fn visit_quantified_expr(&mut self, expr: &mut ast::QuantifiedExpr) {
        self.visit_expr_single(&mut expr.var_expr);
        let old_span = expr.var_name.span;
        expr.var_name = self.push_name(&expr.var_name.value).with_span(old_span);
        self.visit_expr_single(&mut expr.satisfies_expr);
        self.pop_name();
    }

    fn visit_inline_function(&mut self, expr: &mut ast::InlineFunction) {
        for param in &mut expr.params {
            param.name = self.push_name(&param.name);
        }
        self.visit_expr_or_empty(&mut expr.body);
        for _ in &expr.params {
            self.pop_name();
        }
    }

    fn visit_var_ref(&mut self, name: &mut Name) {
        let new_name = self.names.get(name);
        *name = new_name;
    }
}

pub(crate) fn unique_names(expr: &mut ast::XPath, variable_names: &VariableNames) {
    let mut renamer = Renamer::new();
    // ensure we know of the outer variable names too;
    // these are never going to be changed as there isn't
    // any other shadowing yet at this point
    for name in variable_names {
        renamer.push_name(name);
    }
    renamer.visit_xpath(expr);
}
