//! Dependency analysis over a group of sibling declarations, shared by
//! inference (`infer::infer_decls`) and evaluation (`eval::eval_module_env` /
//! the `let` rule). A declaration depends on a sibling when it references that
//! sibling's name free (not shadowed by a parameter, lambda, `let`, or pattern
//! binder). Tarjan's algorithm collapses those edges into strongly connected
//! components: each SCC is one mutually recursive clique, and the components
//! come out in reverse-finish order — dependencies before dependents. Feeding
//! declarations to inference and eval one SCC at a time is what lets a group
//! reference itself and its group-mates (mutual recursion, ADR 0011) while a
//! non-recursive decl still sees the finished schemes/values of everything it
//! depends on, regardless of source order.

use std::collections::{HashMap, HashSet};

use crate::ast::{Arm, Decl, EntryExpr, Expr, Pattern, Spanned};

/// Partition `decls` into strongly connected components in dependency order:
/// every component precedes the components that depend on it, and a component
/// with more than one member (or a self-referential singleton) is a recursive
/// clique to be processed together.
pub fn scc_order(decls: &[Decl]) -> Vec<Vec<usize>> {
    let names: HashMap<&str, usize> =
        decls.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();

    let edges: Vec<Vec<usize>> = decls
        .iter()
        .map(|d| dependency_indices(d, &names))
        .collect();

    Tarjan::new(&edges).run()
}

/// The indices of the siblings a declaration references free — its edges in the
/// dependency graph. The decl's own parameters start out bound, so a parameter
/// that shadows a sibling name is not counted as a dependency.
fn dependency_indices(decl: &Decl, names: &HashMap<&str, usize>) -> Vec<usize> {
    let mut bound: HashSet<String> = decl.params.iter().cloned().collect();
    let mut refs: HashSet<usize> = HashSet::new();
    free_vars_expr(&decl.body, &mut bound, names, &mut refs);
    let mut out: Vec<usize> = refs.into_iter().collect();
    out.sort_unstable();
    out
}

/// Walk an expression collecting references to sibling declarations (`names`)
/// into `refs`, skipping any name currently in `bound`. Each binding form —
/// lambda, `let`, `case` arm — adds its binders to `bound` for the duration of
/// its body and removes only the ones it actually introduced (a binder that
/// shadows an already-bound name is left in place on the way out), so the same
/// `bound` set threads through nested scopes correctly.
fn free_vars_expr(
    e: &Spanned<Expr>,
    bound: &mut HashSet<String>,
    names: &HashMap<&str, usize>,
    refs: &mut HashSet<usize>,
) {
    match &e.0 {
        Expr::Var(name) => {
            if !bound.contains(name) {
                if let Some(&i) = names.get(name.as_str()) {
                    refs.insert(i);
                }
            }
        }
        Expr::Str(_) | Expr::Int(_) | Expr::Float(_) | Expr::Char(_) | Expr::Ctor(_) => {}
        Expr::AptPackage(x) | Expr::SystemdService(x) => free_vars_expr(x, bound, names, refs),
        Expr::Filesystem { path, entry } => {
            free_vars_expr(path, bound, names, refs);
            match entry {
                EntryExpr::File { contents, mode } => {
                    free_vars_expr(contents, bound, names, refs);
                    free_vars_expr(mode, bound, names, refs);
                }
                EntryExpr::Directory { mode } => {
                    free_vars_expr(mode, bound, names, refs);
                }
                EntryExpr::Symlink { target } => {
                    free_vars_expr(target, bound, names, refs);
                }
            }
        }
        Expr::LineInFile { path, line } => {
            free_vars_expr(path, bound, names, refs);
            free_vars_expr(line, bound, names, refs);
        }
        Expr::Scroll { name, glyphs } => {
            free_vars_expr(name, bound, names, refs);
            free_vars_expr(glyphs, bound, names, refs);
        }
        Expr::List(items) => {
            for it in items {
                free_vars_expr(it, bound, names, refs);
            }
        }
        Expr::Lam { param, body } => {
            let shadowed = bound.insert(param.clone());
            free_vars_expr(body, bound, names, refs);
            if shadowed {
                bound.remove(param);
            }
        }
        Expr::App(f, x) => {
            free_vars_expr(f, bound, names, refs);
            free_vars_expr(x, bound, names, refs);
        }
        Expr::Let { decls, body } => {
            let previously: Vec<(String, bool)> =
                decls.iter().map(|d| (d.name.clone(), bound.contains(&d.name))).collect();
            for d in decls {
                bound.insert(d.name.clone());
            }
            for d in decls {
                let mut inner: Vec<(String, bool)> =
                    d.params.iter().map(|p| (p.clone(), bound.contains(p))).collect();
                for p in &d.params {
                    bound.insert(p.clone());
                }
                free_vars_expr(&d.body, bound, names, refs);
                for (p, was_bound) in inner.drain(..) {
                    if !was_bound {
                        bound.remove(&p);
                    }
                }
            }
            free_vars_expr(body, bound, names, refs);
            for (name, was_bound) in previously {
                if !was_bound {
                    bound.remove(&name);
                }
            }
        }
        Expr::Record(fields) => {
            for v in fields.values() {
                free_vars_expr(v, bound, names, refs);
            }
        }
        // Recurse into every tuple element, like `List` — a reference inside a
        // tuple expression is a real dependency edge (ADR 0027).
        Expr::Tuple(items) => {
            for it in items {
                free_vars_expr(it, bound, names, refs);
            }
        }
        Expr::Field(base, _) => free_vars_expr(base, bound, names, refs),
        Expr::If { cond, then_, else_ } => {
            free_vars_expr(cond, bound, names, refs);
            free_vars_expr(then_, bound, names, refs);
            free_vars_expr(else_, bound, names, refs);
        }
        Expr::Case { scrutinee, arms } => {
            free_vars_expr(scrutinee, bound, names, refs);
            for arm in arms {
                free_vars_arm(arm, bound, names, refs);
            }
        }
    }
}

fn free_vars_arm(
    arm: &Arm,
    bound: &mut HashSet<String>,
    names: &HashMap<&str, usize>,
    refs: &mut HashSet<usize>,
) {
    let mut introduced = Vec::new();
    collect_pattern_binders(&arm.pat.0, &mut introduced);
    let restore: Vec<(String, bool)> =
        introduced.iter().map(|n| (n.clone(), bound.contains(n))).collect();
    for n in &introduced {
        bound.insert(n.clone());
    }
    free_vars_expr(&arm.body, bound, names, refs);
    for (n, was_bound) in restore {
        if !was_bound {
            bound.remove(&n);
        }
    }
}

fn collect_pattern_binders(pat: &Pattern, out: &mut Vec<String>) {
    match pat {
        // Literal patterns (`Str`/`Int`/`Char`) and `Nil` bind no names.
        Pattern::Wildcard | Pattern::Str(_) | Pattern::Int(_) | Pattern::Char(_) | Pattern::Nil => {}
        Pattern::Var(name) => out.push(name.clone()),
        Pattern::Ctor(_, subs) => {
            for s in subs {
                collect_pattern_binders(&s.0, out);
            }
        }
        Pattern::Cons(head, tail) => {
            collect_pattern_binders(&head.0, out);
            collect_pattern_binders(&tail.0, out);
        }
        // A tuple pattern binds through each element, like `Ctor` — so a binder
        // inside `(x, y) -> …` is counted as bound, not as a free variable
        // (ADR 0027).
        Pattern::Tuple(subs) => {
            for s in subs {
                collect_pattern_binders(&s.0, out);
            }
        }
    }
}

struct Tarjan<'a> {
    edges: &'a [Vec<usize>],
    index: usize,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    indices: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    components: Vec<Vec<usize>>,
}

impl<'a> Tarjan<'a> {
    fn new(edges: &'a [Vec<usize>]) -> Self {
        let n = edges.len();
        Tarjan {
            edges,
            index: 0,
            stack: Vec::new(),
            on_stack: vec![false; n],
            indices: vec![None; n],
            lowlink: vec![0; n],
            components: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Vec<usize>> {
        for v in 0..self.edges.len() {
            if self.indices[v].is_none() {
                self.strongconnect(v);
            }
        }
        self.components
    }

    fn strongconnect(&mut self, v: usize) {
        self.indices[v] = Some(self.index);
        self.lowlink[v] = self.index;
        self.index += 1;
        self.stack.push(v);
        self.on_stack[v] = true;

        for &w in &self.edges[v].clone() {
            match self.indices[w] {
                None => {
                    self.strongconnect(w);
                    self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                }
                Some(idx) => {
                    if self.on_stack[w] {
                        self.lowlink[v] = self.lowlink[v].min(idx);
                    }
                }
            }
        }

        if self.indices[v] == Some(self.lowlink[v]) {
            let mut component = Vec::new();
            loop {
                let w = self.stack.pop().unwrap();
                self.on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            component.sort_unstable();
            self.components.push(component);
        }
    }
}

/// Whether a component needs the recursive-binding treatment: any multi-member
/// clique is recursive, and a singleton is recursive exactly when it references
/// its own name. A non-recursive singleton can be bound with a plain
/// left-to-right pass instead of tying a knot.
pub fn group_is_recursive(decls: &[Decl], group: &[usize]) -> bool {
    if group.len() > 1 {
        return true;
    }
    let idx = group[0];
    let decl = &decls[idx];
    let mut names = HashMap::new();
    names.insert(decl.name.as_str(), idx);
    dependency_indices(decl, &names).contains(&idx)
}
