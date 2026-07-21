//! Hindley-Milner type inference (Algorithm W). Runs to completion before
//! evaluation; a module that type-checks cannot fail at runtime with a type
//! error, and — crucially for a total language — a `case` that type-checks is
//! exhaustive, so evaluation never hits a no-match.
//!
//! The core (`prune`/`apply`/`unify`/`occurs`/`ftv`/`generalize`/`instantiate`)
//! is textbook Algorithm W over a substitution, with these additions:
//!
//!   * **Constrained variables** (ADR 0007). A `Var` carries a `Constraint`
//!     bound (`number`/`comparable`). `bind` enforces it: binding to an
//!     inadmissible concrete type is an error (`constraint_admits`), and two
//!     bounded vars merge to the stronger bound (`merge_constraints`). This is
//!     the one departure from pure HM.
//!   * **Glyph injection** (ADR 0002). Each concrete glyph type unifies with
//!     `Glyph` but not with the other glyph types (`glyph_injects`). The
//!     symmetric arm is sound only while glyphs have no elimination form — see
//!     the `NOTE` on that arm and ADR 0008.
//!   * **Signatures with type variables** (ADR 0003). A signature's `Rigid`
//!     vars are instantiated to fresh unification vars, then unified with the
//!     inferred body — the same machinery that already makes `id` polymorphic.
//!   * **Exhaustiveness + redundancy** (ADR 0005). `check_exhaustive` runs
//!     Maranget's usefulness algorithm over each `case`, guaranteeing totality
//!     of the elimination form.
//!   * **`Int` defaulting** (ADR 0007). An unresolved `number` at a top-level
//!     decl defaults to `Int` (`default_number_vars`).
//!   * **Row-polymorphic records** (ADR 0010). Records carry a `Row` tail, and
//!     `.field` unifies against an open record instead of demanding a concrete
//!     one, so `\h -> h.name` type-checks. Row variables live in their own id
//!     space with a separate substitution (`row_subst`) and their own occurs
//!     check — see `unify_records`, `bind_row`, `row_occurs`, and the
//!     `Expr::Field` rule.
//!   * **SCC-grouped recursive binding** (ADR 0011). Declarations are not
//!     inferred left-to-right: `infer_decls` partitions them into dependency
//!     strongly-connected components (`depgraph`) and infers each group
//!     together — every member bound to a fresh monomorphic var before any
//!     body, generalized only once the group is solved. This is what makes both
//!     self- and mutual recursion type-check; source order no longer matters for
//!     forward references. See `infer_group`.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ast::*;
use crate::query::{DefSite, QueryIndex, ScopeId};

pub struct TypeError {
    pub msg: String,
    pub span: Span,
    pub note: Option<String>,
}
impl TypeError {
    fn new(msg: impl Into<String>, span: Span) -> Self {
        TypeError { msg: msg.into(), span, note: None }
    }
    fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }
}

#[derive(Default)]
struct Infer {
    next: u32,
    subst: HashMap<u32, Type>,
    // Resolves row variables, parallel to `subst` for ordinary type variables
    // (ADR 0010). A bound row var maps to the fields it was found to carry plus
    // its own remaining tail, so a chain of bindings can be followed to a
    // record's full field set — see `prune_record`. Kept separate from `subst`
    // because row vars and type vars share neither a domain nor an occurs check.
    row_subst: HashMap<u32, (BTreeMap<String, Type>, Row)>,
    // The user `type` decls, indexed for lookup during inference and populated
    // once by `register_type_decls` before any value decl is inferred.
    // `user_ctor_schemes`: each variant's value-constructor scheme, keyed by
    // constructor name (`Just` → `∀a. a -> Maybe a`). `user_sum_ctors`: each
    // type's full variant set (name + arity), keyed by type-constructor name,
    // for the exhaustiveness checker. Both are consulted ahead of the prelude —
    // see `constructor_scheme` / `sum_type_constructors`.
    user_ctor_schemes: HashMap<String, Scheme>,
    user_sum_ctors: HashMap<String, Vec<(String, usize)>>,
    recorder: Option<Recorder>,
}

#[derive(Default)]
struct Recorder {
    index: QueryIndex,
    next_scope: ScopeId,
    def_frames: Vec<HashMap<String, DefSite>>,
    type_spans: Vec<(Span, Type)>,
}

impl Recorder {
    fn open_scope(&mut self, region: Span, env: &TyEnv, added: HashMap<String, DefSite>) -> ScopeId {
        let id = self.next_scope;
        self.next_scope += 1;
        let names: Vec<(String, Scheme)> =
            env.entries().map(|(n, s)| (n.clone(), s.clone())).collect();
        self.index.scope_table.insert(id, names);
        self.index.scopes.push((region, id));
        self.def_frames.push(added);
        id
    }

    fn close_scope(&mut self) {
        self.def_frames.pop();
    }

    fn resolve_def(&self, name: &str) -> Option<DefSite> {
        for frame in self.def_frames.iter().rev() {
            if let Some(site) = frame.get(name) {
                return Some(site.clone());
            }
        }
        None
    }

    fn record_use(&mut self, span: Span, name: &str) {
        if let Some(site) = self.resolve_def(name) {
            self.index.defs.push((span, site));
        }
    }

    fn record_type(&mut self, span: Span, ty: Type) {
        self.type_spans.push((span, ty));
    }
}

impl Infer {
    /// The scheme of a value constructor. User-declared constructors (populated
    /// by `register_type_decls`) shadow the prelude's built-ins, though
    /// `register_type_decls` also rejects a user constructor that collides with
    /// a built-in, so in practice the two sets are disjoint.
    fn constructor_scheme(&self, name: &str) -> Option<Scheme> {
        self.user_ctor_schemes
            .get(name)
            .cloned()
            .or_else(|| crate::prelude::constructor_scheme(name))
    }

    /// The complete constructor set of a sum type — every variant's name and
    /// arity, which the exhaustiveness checker needs. User `type` decls first,
    /// then the prelude's built-in sums (`Maybe`/`Bool`/`Order`).
    fn sum_type_constructors(&self, type_name: &str) -> Option<Vec<(String, usize)>> {
        self.user_sum_ctors
            .get(type_name)
            .cloned()
            .or_else(|| crate::prelude::sum_type_constructors(type_name))
    }

    /// Seed the constructor scope with the open-exposed constructors this module
    /// imports, before its own `type` decls register. They land in the same
    /// `user_ctor_schemes` / `user_sum_ctors` tables the local decls use, so
    /// `constructor_scheme` and `sum_type_constructors` resolve imported and
    /// local constructors uniformly.
    fn seed_imported_constructors(&mut self, imported: &ImportedConstructors) {
        for (name, scheme) in &imported.ctor_schemes {
            self.user_ctor_schemes.insert(name.clone(), scheme.clone());
        }
        for (name, members) in &imported.sum_ctors {
            self.user_sum_ctors.insert(name.clone(), members.clone());
        }
    }
}

impl Infer {
    fn recording(&self) -> bool {
        self.recorder.is_some()
    }

    fn rec_type(&mut self, span: &Span, ty: &Type) {
        if let Some(r) = &mut self.recorder {
            r.record_type(span.clone(), ty.clone());
        }
    }

    fn rec_use(&mut self, span: &Span, name: &str) {
        if let Some(r) = &mut self.recorder {
            r.record_use(span.clone(), name);
        }
    }

    fn rec_open_scope(&mut self, region: Span, env: &TyEnv, added: HashMap<String, DefSite>) {
        if let Some(r) = &mut self.recorder {
            r.open_scope(region, env, added);
        }
    }

    fn rec_close_scope(&mut self) {
        if let Some(r) = &mut self.recorder {
            r.close_scope();
        }
    }

    fn fresh(&mut self) -> Type {
        self.fresh_constrained(Constraint::None)
    }

    fn fresh_constrained(&mut self, c: Constraint) -> Type {
        let v = self.next;
        self.next += 1;
        Type::Var(v, c)
    }

    /// A fresh open row tail. Draws from the same `next` counter as `fresh`,
    /// so a row-var id can never collide with a type-var id even though the two
    /// resolve through different substitutions.
    fn fresh_row(&mut self) -> Row {
        let r = self.next;
        self.next += 1;
        Row::Open(r)
    }

    /// Resolve a record's tail: follow bound row variables through `row_subst`,
    /// splicing in the fields each one carries, until the tail is closed or an
    /// unbound row var. Known fields win over spliced ones (`or_insert`), so a
    /// field already unified on this record is never overwritten. This is the
    /// row-level analogue of `prune` for records.
    fn prune_record(&self, fields: &BTreeMap<String, Type>, row: &Row) -> (BTreeMap<String, Type>, Row) {
        let mut all = fields.clone();
        let mut tail = row.clone();
        while let Row::Open(r) = tail {
            match self.row_subst.get(&r) {
                Some((more, next_tail)) => {
                    for (k, v) in more {
                        all.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                    tail = next_tail.clone();
                }
                None => break,
            }
        }
        (all, tail)
    }

    /// Follow the substitution to a representative type (shallow).
    fn prune(&self, t: &Type) -> Type {
        match t {
            Type::Var(v, _) => {
                if let Some(inner) = self.subst.get(v) {
                    self.prune(inner)
                } else {
                    t.clone()
                }
            }
            Type::Record(fields, row) => {
                let (fields, row) = self.prune_record(fields, row);
                Type::Record(fields, row)
            }
            _ => t.clone(),
        }
    }

    /// Fully apply the substitution (deep).
    fn apply(&self, t: &Type) -> Type {
        match self.prune(t) {
            Type::Con(name, args) => {
                Type::Con(name, args.iter().map(|a| self.apply(a)).collect())
            }
            Type::Fun(a, b) => Type::Fun(Box::new(self.apply(&a)), Box::new(self.apply(&b))),
            Type::Record(fs, row) => {
                Type::Record(fs.iter().map(|(k, v)| (k.clone(), self.apply(v))).collect(), row)
            }
            other => other,
        }
    }

    fn occurs(&self, v: u32, t: &Type) -> bool {
        match self.prune(t) {
            Type::Var(w, _) => v == w,
            Type::Con(_, args) => args.iter().any(|a| self.occurs(v, a)),
            Type::Fun(a, b) => self.occurs(v, &a) || self.occurs(v, &b),
            Type::Record(fs, _) => fs.values().any(|ty| self.occurs(v, ty)),
            _ => false,
        }
    }

    /// Occurs check for row variables: does row var `r` appear anywhere in `t`,
    /// whether inside a field's type or as a record's own tail? Prevents
    /// `bind_row` from tying a row var into a record it already tails, which
    /// would be an infinite record. The row-level counterpart of `occurs`.
    fn row_occurs(&self, r: u32, t: &Type) -> bool {
        match self.prune(t) {
            Type::Con(_, args) => args.iter().any(|a| self.row_occurs(r, a)),
            Type::Fun(a, b) => self.row_occurs(r, &a) || self.row_occurs(r, &b),
            Type::Record(fs, row) => {
                fs.values().any(|ty| self.row_occurs(r, ty)) || row == Row::Open(r)
            }
            _ => false,
        }
    }

    /// Bind unification variable `v` (carrying bound `c`) to `t`, enforcing the
    /// bound. Binding to another var merges their bounds to the stronger one;
    /// binding to a concrete type rejects it unless the bound admits it, and
    /// rejects an infinite type via the occurs check.
    fn bind(&mut self, v: u32, c: Constraint, t: &Type, span: &Span) -> Result<(), TypeError> {
        let t = self.prune(t);
        if let Type::Var(w, cw) = &t {
            if *w == v {
                return Ok(());
            }
            let merged = merge_constraints(c, *cw).ok_or_else(|| {
                TypeError::new(
                    format!(
                        "no type satisfies both `{}` and `{}`",
                        constraint_name(c),
                        constraint_name(*cw)
                    ),
                    span.clone(),
                )
            })?;
            if merged == *cw {
                self.subst.insert(v, Type::Var(*w, merged));
            } else {
                let rep = self.fresh_constrained(merged);
                self.subst.insert(v, rep.clone());
                self.subst.insert(*w, rep);
            }
            return Ok(());
        }
        if self.occurs(v, &t) {
            return Err(TypeError::new(
                format!("infinite type: t{v} occurs in `{}`", self.apply(&t)),
                span.clone(),
            ));
        }
        if !constraint_admits(c, &t) {
            return Err(TypeError::new(
                format!("type `{}` does not satisfy `{}`", self.apply(&t), constraint_name(c)),
                span.clone(),
            ));
        }
        self.subst.insert(v, t);
        Ok(())
    }

    fn unify(&mut self, a: &Type, b: &Type, span: &Span) -> Result<(), TypeError> {
        let a = self.prune(a);
        let b = self.prune(b);
        match (&a, &b) {
            (Type::Var(v, c), _) => self.bind(*v, *c, &b, span),
            (_, Type::Var(v, c)) => self.bind(*v, *c, &a, span),
            (Type::Rigid(x), Type::Rigid(y)) if x == y => Ok(()),
            (Type::Con(n1, a1), Type::Con(n2, a2))
                if glyph_widens_into(n2, a2, n1, a1) =>
            {
                Ok(())
            }
            (Type::Con(n1, a1), Type::Con(n2, a2)) => {
                if n1 != n2 || a1.len() != a2.len() {
                    return Err(TypeError::new(
                        format!(
                            "type mismatch: expected `{}`, found `{}`",
                            self.apply(&a),
                            self.apply(&b)
                        ),
                        span.clone(),
                    ));
                }
                for (x, y) in a1.iter().zip(a2.iter()) {
                    self.unify(x, y, span)?;
                }
                Ok(())
            }
            (Type::Fun(a1, a2), Type::Fun(b1, b2)) => {
                self.unify(a1, b1, span)?;
                self.unify(a2, b2, span)
            }
            (Type::Record(fa, ra), Type::Record(fb, rb)) => {
                self.unify_records(fa, ra, fb, rb, &a, &b, span)
            }
            _ => Err(TypeError::new(
                format!("type mismatch: expected `{}`, found `{}`", self.apply(&a), self.apply(&b)),
                span.clone(),
            )),
        }
    }

    /// Bind row var `r` to the record fragment "`fields`, then `tail`", the row
    /// analogue of `bind`. Rejects the infinite cases: binding `r` to a tail
    /// that is still `r` with extra fields, or to fields that themselves mention
    /// `r` (`row_occurs`). `r` tailing exactly `r` with no extra fields is the
    /// identity binding, so it is simply dropped.
    fn bind_row(
        &mut self,
        r: u32,
        fields: BTreeMap<String, Type>,
        tail: Row,
        span: &Span,
    ) -> Result<(), TypeError> {
        if tail == Row::Open(r) {
            if fields.is_empty() {
                return Ok(());
            }
            return Err(TypeError::new("infinite record row", span.clone()));
        }
        for ty in fields.values() {
            if self.row_occurs(r, ty) {
                return Err(TypeError::new("infinite record row", span.clone()));
            }
        }
        self.row_subst.insert(r, (fields, tail));
        Ok(())
    }

    /// Unify two records (ADR 0010). Shared fields must unify pointwise; then
    /// the two `Row` tails decide what to do with the fields each side has and
    /// the other lacks (`only_a`, `only_b`):
    ///
    ///   * closed ~ closed — the field sets must match exactly.
    ///   * open ~ closed — the open side must be a subset; its row var binds to
    ///     the closed side's surplus fields, closing it off.
    ///   * open ~ open (same var) — the extra fields must be empty (the two are
    ///     already the same record).
    ///   * open ~ open (distinct vars) — each side is missing what the other
    ///     has, so both row vars bind through one shared fresh tail, unifying
    ///     the records while leaving room for still-unknown fields.
    #[allow(clippy::too_many_arguments)]
    fn unify_records(
        &mut self,
        fa: &BTreeMap<String, Type>,
        ra: &Row,
        fb: &BTreeMap<String, Type>,
        rb: &Row,
        a: &Type,
        b: &Type,
        span: &Span,
    ) -> Result<(), TypeError> {
        for (k, va) in fa {
            if let Some(vb) = fb.get(k) {
                self.unify(va, vb, span)?;
            }
        }
        let only_a: BTreeMap<String, Type> =
            fa.iter().filter(|(k, _)| !fb.contains_key(*k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        let only_b: BTreeMap<String, Type> =
            fb.iter().filter(|(k, _)| !fa.contains_key(*k)).map(|(k, v)| (k.clone(), v.clone())).collect();

        let mismatch = || {
            TypeError::new(
                format!("record types differ: `{}` vs `{}`", self.apply(a), self.apply(b)),
                span.clone(),
            )
        };

        match (ra, rb) {
            (Row::Closed, Row::Closed) => {
                if only_a.is_empty() && only_b.is_empty() {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }
            (Row::Open(r), Row::Closed) => {
                if !only_a.is_empty() {
                    return Err(mismatch());
                }
                self.bind_row(*r, only_b, Row::Closed, span)
            }
            (Row::Closed, Row::Open(r)) => {
                if !only_b.is_empty() {
                    return Err(mismatch());
                }
                self.bind_row(*r, only_a, Row::Closed, span)
            }
            (Row::Open(r1), Row::Open(r2)) if r1 == r2 => {
                if only_a.is_empty() && only_b.is_empty() {
                    Ok(())
                } else {
                    Err(mismatch())
                }
            }
            (Row::Open(r1), Row::Open(r2)) => {
                let rest = self.fresh_row();
                self.bind_row(*r1, only_b, rest.clone(), span)?;
                self.bind_row(*r2, only_a, rest, span)
            }
        }
    }

    /// Free type variables of a type under the current substitution.
    fn ftv(&self, t: &Type, acc: &mut HashMap<u32, Constraint>) {
        match self.prune(t) {
            Type::Var(v, c) => {
                acc.insert(v, c);
            }
            Type::Con(_, args) => {
                for a in &args {
                    self.ftv(a, acc);
                }
            }
            Type::Fun(a, b) => {
                self.ftv(&a, acc);
                self.ftv(&b, acc);
            }
            Type::Record(fs, _) => {
                for v in fs.values() {
                    self.ftv(v, acc);
                }
            }
            _ => {}
        }
    }

    /// Free row variables of a type — the row analogue of `ftv`, collecting
    /// open record tails rather than type vars. Drives which row vars
    /// `generalize` may quantify.
    fn frv(&self, t: &Type, acc: &mut HashSet<u32>) {
        match self.prune(t) {
            Type::Con(_, args) => {
                for a in &args {
                    self.frv(a, acc);
                }
            }
            Type::Fun(a, b) => {
                self.frv(&a, acc);
                self.frv(&b, acc);
            }
            Type::Record(fs, row) => {
                for v in fs.values() {
                    self.frv(v, acc);
                }
                if let Row::Open(r) = row {
                    acc.insert(r);
                }
            }
            _ => {}
        }
    }

    fn ftv_env(&self, env: &TyEnv, acc: &mut HashSet<u32>) {
        for scheme in env.0.values() {
            let mut inner = HashMap::new();
            self.ftv(&scheme.ty, &mut inner);
            for (q, _) in &scheme.vars {
                inner.remove(q);
            }
            acc.extend(inner.keys().copied());
        }
    }

    /// Row variables free in the environment: a scheme's own quantified
    /// `row_vars` are bound, so they are excluded. `generalize` must not
    /// quantify a row var still free here, exactly as with `ftv_env` for type
    /// vars.
    fn frv_env(&self, env: &TyEnv, acc: &mut HashSet<u32>) {
        for scheme in env.0.values() {
            let mut inner = HashSet::new();
            self.frv(&scheme.ty, &mut inner);
            for q in &scheme.row_vars {
                inner.remove(q);
            }
            acc.extend(inner.iter().copied());
        }
    }

    /// Generalize a type into a scheme, quantifying over both type variables
    /// and row variables that are free in `t` but not in `env`. Quantifying the
    /// row vars is what makes a record-polymorphic binding reusable at several
    /// record shapes (ADR 0010).
    fn generalize(&self, env: &TyEnv, t: &Type) -> Scheme {
        let mut tvs = HashMap::new();
        self.ftv(t, &mut tvs);
        let mut env_tvs = HashSet::new();
        self.ftv_env(env, &mut env_tvs);
        let vars: Vec<(u32, Constraint)> = tvs
            .into_iter()
            .filter(|(v, _)| !env_tvs.contains(v))
            .collect();
        let mut rvs = HashSet::new();
        self.frv(t, &mut rvs);
        let mut env_rvs = HashSet::new();
        self.frv_env(env, &mut env_rvs);
        let row_vars: Vec<u32> = rvs.into_iter().filter(|r| !env_rvs.contains(r)).collect();
        Scheme { vars, row_vars, ty: self.apply(t) }
    }

    /// Replace a signature's `Rigid` type variables with fresh unification
    /// vars (all occurrences of one name share a var), so the annotation can be
    /// unified against the inferred body and then generalized (ADR 0003).
    fn instantiate_signature(&mut self, sig: &Type) -> Type {
        let mut mapping: HashMap<String, Type> = HashMap::new();
        self.instantiate_rigids(sig, &mut mapping)
    }

    fn instantiate_rigids(&mut self, t: &Type, mapping: &mut HashMap<String, Type>) -> Type {
        match t {
            Type::Rigid(name) => mapping
                .entry(name.clone())
                .or_insert_with(|| {
                    let v = self.next;
                    self.next += 1;
                    Type::Var(v, Constraint::None)
                })
                .clone(),
            Type::Con(name, args) => Type::Con(
                name.clone(),
                args.iter().map(|a| self.instantiate_rigids(a, mapping)).collect(),
            ),
            Type::Fun(a, b) => Type::Fun(
                Box::new(self.instantiate_rigids(a, mapping)),
                Box::new(self.instantiate_rigids(b, mapping)),
            ),
            Type::Record(fs, row) => Type::Record(
                fs.iter()
                    .map(|(k, v)| (k.clone(), self.instantiate_rigids(v, mapping)))
                    .collect(),
                row.clone(),
            ),
            Type::Var(v, c) => Type::Var(*v, *c),
        }
    }

    /// Elm's number defaulting: any still-unresolved `number` variable in `t`
    /// is pinned to `Int` (ADR 0007). Applied at top-level generalization so a
    /// literal like `3` used at no other type becomes `Int`.
    fn default_number_vars(&mut self, t: &Type) {
        let mut acc = HashMap::new();
        self.ftv(t, &mut acc);
        for (v, c) in acc {
            if c == Constraint::Number {
                self.subst.insert(v, con("Int"));
            }
        }
    }

    /// Instantiate a scheme with fresh variables. Type vars and row vars are
    /// refreshed through separate mappings, so each use of a record-polymorphic
    /// binding gets its own row tail and can unify at a different record shape
    /// (ADR 0010).
    fn instantiate(&mut self, s: &Scheme) -> Type {
        let mut mapping = HashMap::new();
        for (v, c) in &s.vars {
            let f = self.fresh_constrained(*c);
            mapping.insert(*v, f);
        }
        let mut row_mapping = HashMap::new();
        for r in &s.row_vars {
            let f = self.fresh_row();
            row_mapping.insert(*r, f);
        }
        fn go(t: &Type, m: &HashMap<u32, Type>, rm: &HashMap<u32, Row>) -> Type {
            match t {
                Type::Var(v, c) => m.get(v).cloned().unwrap_or(Type::Var(*v, *c)),
                Type::Con(name, args) => {
                    Type::Con(name.clone(), args.iter().map(|a| go(a, m, rm)).collect())
                }
                Type::Fun(a, b) => Type::Fun(Box::new(go(a, m, rm)), Box::new(go(b, m, rm))),
                Type::Record(fs, row) => {
                    let fields = fs.iter().map(|(k, v)| (k.clone(), go(v, m, rm))).collect();
                    let row = match row {
                        Row::Open(r) => rm.get(r).cloned().unwrap_or(Row::Open(*r)),
                        Row::Closed => Row::Closed,
                    };
                    Type::Record(fields, row)
                }
                other => other.clone(),
            }
        }
        go(&s.ty, &mapping, &row_mapping)
    }
}

/// The stronger of two bounds when two constrained vars unify, or `None` when
/// no type can satisfy both. `Constraint::None` is the identity; `Number ⊂
/// Comparable`, so `Number ∧ Comparable = Number`. `Appendable` (`String`/`List
/// a`) shares no admissible type with `Number` or `Comparable`, so merging it
/// with either is unsatisfiable and rejected.
fn merge_constraints(a: Constraint, b: Constraint) -> Option<Constraint> {
    match (a, b) {
        (Constraint::None, other) | (other, Constraint::None) => Some(other),
        (Constraint::Number, Constraint::Number) => Some(Constraint::Number),
        (Constraint::Comparable, Constraint::Comparable) => Some(Constraint::Comparable),
        (Constraint::Appendable, Constraint::Appendable) => Some(Constraint::Appendable),
        (Constraint::Number, Constraint::Comparable) | (Constraint::Comparable, Constraint::Number) => {
            Some(Constraint::Number)
        }
        _ => None,
    }
}

/// Whether a concrete type satisfies a bound: `number` admits `Int`/`Float`,
/// `comparable` also admits `String`, `appendable` admits `String`/`List a`. A
/// non-`Con` type (var, function, record) is only admissible under `None`.
fn constraint_admits(c: Constraint, t: &Type) -> bool {
    let head = match t {
        Type::Con(name, _) => name.as_str(),
        _ => return c == Constraint::None,
    };
    match c {
        Constraint::None => true,
        Constraint::Number => matches!(head, "Int" | "Float"),
        Constraint::Comparable => matches!(head, "Int" | "Float" | "String"),
        Constraint::Appendable => matches!(head, "String" | "List"),
    }
}

fn constraint_name(c: Constraint) -> &'static str {
    match c {
        Constraint::None => "unconstrained",
        Constraint::Number => "number",
        Constraint::Comparable => "comparable",
        Constraint::Appendable => "appendable",
    }
}

/// Promote a concrete glyph subtype (`AptPackage`, …) to `Glyph` before it is
/// unified against a list element or a sibling branch, so a mixed list of
/// glyphs infers as `List Glyph` rather than failing to unify the two concrete
/// subtypes with each other (they inject into `Glyph`, not into each other).
fn widen_glyph_subtype(inf: &Infer, t: &Type) -> Type {
    match inf.prune(t) {
        Type::Con(name, args) if args.is_empty() && is_glyph_subtype(&name) => con("Glyph"),
        Type::Con(name, args) if name == "List" && args.len() == 1 => {
            Type::Con("List".to_string(), vec![widen_glyph_subtype(inf, &args[0])])
        }
        pruned => pruned,
    }
}

fn is_glyph_subtype(name: &str) -> bool {
    matches!(name, "AptPackage" | "SystemdService" | "Filesystem" | "LineInFile")
}

fn glyph_widens_into(from: &str, from_args: &[Type], to: &str, to_args: &[Type]) -> bool {
    from_args.is_empty() && to_args.is_empty() && to == "Glyph" && is_glyph_subtype(from)
}

fn con(name: &str) -> Type {
    Type::Con(name.to_string(), vec![])
}

/// The constructor-scope contribution of a module's open-exposed (`Type(..)`)
/// imports: each imported constructor's value scheme and each imported type's
/// full variant set. The resolver harvests this from the interfaces a module
/// imports and hands it to `check_entry`/`check_library`, which seed it into the
/// `Infer` so `infer_pattern` can resolve imported constructors and the
/// exhaustiveness checker can see their type's complete signature — the pattern
/// counterpart to `imported_types` on the annotation side.
#[derive(Default)]
pub struct ImportedConstructors {
    pub ctor_schemes: HashMap<String, Scheme>,
    pub sum_ctors: HashMap<String, Vec<(String, usize)>>,
}

#[derive(Clone, Default)]
pub struct TyEnv(BTreeMap<String, Scheme>);
impl TyEnv {
    fn get(&self, k: &str) -> Option<&Scheme> {
        self.0.get(k)
    }
    pub fn scheme(&self, k: &str) -> Option<Scheme> {
        self.0.get(k).cloned()
    }
    pub fn has(&self, k: &str) -> bool {
        self.0.contains_key(k)
    }
    fn insert(&self, k: String, s: Scheme) -> TyEnv {
        let mut m = self.0.clone();
        m.insert(k, s);
        TyEnv(m)
    }
    pub fn bind(self, k: String, s: Scheme) -> TyEnv {
        self.insert(k, s)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&String, &Scheme)> {
        self.0.iter()
    }
}

fn mono(t: Type) -> Scheme {
    Scheme { vars: vec![], row_vars: vec![], ty: t }
}

fn infer_expr(inf: &mut Infer, env: &TyEnv, e: &Spanned<Expr>) -> Result<Type, TypeError> {
    let ty = infer_expr_inner(inf, env, e)?;
    inf.rec_type(&e.1, &ty);
    Ok(ty)
}

fn infer_expr_inner(inf: &mut Infer, env: &TyEnv, e: &Spanned<Expr>) -> Result<Type, TypeError> {
    let span = &e.1;
    match &e.0 {
        Expr::Str(_) => Ok(con("String")),

        Expr::Int(_) => Ok(inf.fresh_constrained(Constraint::Number)),

        Expr::Float(_) => Ok(con("Float")),

        Expr::Var(name) => match env.get(name) {
            Some(s) => {
                inf.rec_use(span, name);
                Ok(inf.instantiate(s))
            }
            None => Err(TypeError::new(format!("unknown name `{name}`"), span.clone())
                .note("not bound by any declaration, `let`, or lambda parameter")),
        },

        Expr::Ctor(name) => match env.get(name) {
            Some(s) => {
                inf.rec_use(span, name);
                Ok(inf.instantiate(s))
            }
            None => Err(TypeError::new(format!("unknown constructor `{name}`"), span.clone())),
        },

        Expr::AptPackage(name) => {
            let nt = infer_expr(inf, env, name)?;
            inf.unify(&nt, &con("String"), &name.1)?;
            Ok(con("AptPackage"))
        }

        Expr::SystemdService(unit) => {
            let ut = infer_expr(inf, env, unit)?;
            inf.unify(&ut, &con("String"), &unit.1)?;
            Ok(con("SystemdService"))
        }

        // All three surface spellings (`file`/`directory`/`symlink`) share one
        // glyph type, `Filesystem` (ADR 0019): the `Entry` distinction lives in
        // the IR, not the type surface. Every field is `String` here — including
        // `mode`, which stays an octal string until `eval` lowers it to a `u16`.
        Expr::Filesystem { path, entry } => {
            let pt = infer_expr(inf, env, path)?;
            inf.unify(&pt, &con("String"), &path.1)?;
            let fields: Vec<&Spanned<Expr>> = match entry {
                EntryExpr::File { contents, mode } => vec![contents, mode],
                EntryExpr::Directory { mode } => vec![mode],
                EntryExpr::Symlink { target } => vec![target],
            };
            for field in fields {
                let ft = infer_expr(inf, env, field)?;
                inf.unify(&ft, &con("String"), &field.1)?;
            }
            Ok(con("Filesystem"))
        }

        Expr::LineInFile { path, line } => {
            for field in [path, line] {
                let ft = infer_expr(inf, env, field)?;
                inf.unify(&ft, &con("String"), &field.1)?;
            }
            Ok(con("LineInFile"))
        }

        Expr::Scroll { name, glyphs } => {
            let nt = infer_expr(inf, env, name)?;
            inf.unify(&nt, &con("String"), &name.1)?;
            let gt = infer_expr(inf, env, glyphs)?;
            let gt = widen_glyph_subtype(inf, &gt);
            let glyph_list = Type::Con("List".to_string(), vec![con("Glyph")]);
            inf.unify(&gt, &glyph_list, &glyphs.1)?;
            Ok(con("Scroll"))
        }

        Expr::List(items) => {
            let elem = inf.fresh();
            for it in items {
                let t = infer_expr(inf, env, it)?;
                let t = widen_glyph_subtype(inf, &t);
                inf.unify(&elem, &t, &it.1)?;
            }
            Ok(Type::Con("List".to_string(), vec![elem]))
        }

        Expr::Lam { param, body } => {
            let tv = inf.fresh();
            let env2 = env.insert(param.clone(), mono(tv.clone()));
            let mut added = HashMap::new();
            added.insert(param.clone(), DefSite { span: span.clone(), module: None });
            inf.rec_open_scope(body.1.clone(), &env2, added);
            let bt = infer_expr(inf, &env2, body)?;
            inf.rec_close_scope();
            Ok(Type::Fun(Box::new(tv), Box::new(bt)))
        }

        Expr::App(f, x) => {
            let ft = infer_expr(inf, env, f)?;
            let xt = infer_expr(inf, env, x)?;
            let ret = inf.fresh();
            let expected = Type::Fun(Box::new(xt), Box::new(ret.clone()));
            inf.unify(&ft, &expected, span)?;
            Ok(ret)
        }

        Expr::Let { decls, body } => {
            let env2 = infer_decls(inf, env, decls, false)?;
            let added = decl_def_sites(decls);
            inf.rec_open_scope(span.clone(), &env2, added);
            let bt = infer_expr(inf, &env2, body)?;
            inf.rec_close_scope();
            Ok(bt)
        }

        Expr::Record(fields) => {
            // A literal names every field it has, so it is a closed record —
            // its type carries no row tail (ADR 0010).
            let mut tys = BTreeMap::new();
            for (k, v) in fields {
                tys.insert(k.clone(), infer_expr(inf, env, v)?);
            }
            Ok(Type::Record(tys, Row::Closed))
        }

        Expr::If { cond, then_, else_ } => {
            let ct = infer_expr(inf, env, cond)?;
            inf.unify(&ct, &con("Bool"), &cond.1)?;
            let tt = infer_expr(inf, env, then_)?;
            let tt = widen_glyph_subtype(inf, &tt);
            let et = infer_expr(inf, env, else_)?;
            let et = widen_glyph_subtype(inf, &et);
            inf.unify(&tt, &et, &else_.1)?;
            Ok(inf.apply(&tt))
        }

        Expr::Case { scrutinee, arms } => {
            let st = infer_expr(inf, env, scrutinee)?;
            let result = inf.fresh();
            for arm in arms {
                let mut arm_env = env.clone();
                infer_pattern(inf, &mut arm_env, &arm.pat, &st)?;
                let added = pattern_def_sites(&arm.pat);
                inf.rec_open_scope(arm.body.1.clone(), &arm_env, added);
                let bt = infer_expr(inf, &arm_env, &arm.body)?;
                let bt = widen_glyph_subtype(inf, &bt);
                inf.unify(&result, &bt, &arm.body.1)?;
                inf.rec_close_scope();
            }
            check_exhaustive(inf, &st, arms, span)?;
            Ok(inf.apply(&result))
        }

        Expr::Field(base, field) => {
            // Row polymorphism (ADR 0010). Rather than require `base` to be a
            // concrete record, unify it with an OPEN record demanding just this
            // one field: `{ field : result | rest }`. A fresh row var `rest`
            // absorbs whatever other fields `base` has. Because `base` may still
            // be an unbound type var (a lambda parameter), this is what lets
            // `\h -> h.name` type-check without knowing `h`'s full shape.
            let bt = infer_expr(inf, env, base)?;
            let pruned = inf.prune(&bt);
            // A non-record constructor (e.g. `Int`) can never gain fields, so
            // reject it now with a clearer message than a raw unify failure.
            if let Type::Con(name, _) = &pruned {
                return Err(TypeError::new(
                    format!("field access `.{field}` on non-record type `{name}`"),
                    base.1.clone(),
                ));
            }
            let result = inf.fresh();
            let rest = inf.fresh_row();
            let mut demanded = BTreeMap::new();
            demanded.insert(field.clone(), result.clone());
            let open = Type::Record(demanded, rest);
            inf.unify(&bt, &open, span)?;
            Ok(inf.apply(&result))
        }
    }
}

/// Type a pattern against the scrutinee type, extending `env` with any bindings
/// it introduces. A constructor pattern instantiates its scheme, unifies the
/// result with the scrutinee, and recurses into sub-patterns; a var pattern
/// binds the scrutinee type; `_` binds nothing; a string pattern forces
/// `String` (ADR 0005).
fn infer_pattern(
    inf: &mut Infer,
    env: &mut TyEnv,
    pat: &Spanned<Pattern>,
    scrutinee: &Type,
) -> Result<(), TypeError> {
    if inf.recording() {
        let scrut = inf.apply(scrutinee);
        inf.rec_type(&pat.1, &scrut);
    }
    match &pat.0 {
        Pattern::Wildcard => Ok(()),
        Pattern::Var(name) => {
            *env = env.insert(name.clone(), mono(inf.apply(scrutinee)));
            Ok(())
        }
        Pattern::Str(_) => inf.unify(scrutinee, &con("String"), &pat.1),
        // `[]` and `head :: tail` constrain the scrutinee to some `List elem`.
        // `Cons` additionally binds its head at `elem` and its tail at the same
        // list type — the recursive shape that lets a `case` walk a list.
        Pattern::Nil => {
            let elem = inf.fresh();
            let list_ty = Type::Con("List".to_string(), vec![elem]);
            inf.unify(scrutinee, &list_ty, &pat.1)
        }
        Pattern::Cons(head, tail) => {
            let elem = inf.fresh();
            let list_ty = Type::Con("List".to_string(), vec![elem.clone()]);
            inf.unify(scrutinee, &list_ty, &pat.1)?;
            infer_pattern(inf, env, head, &elem)?;
            infer_pattern(inf, env, tail, &list_ty)
        }
        Pattern::Ctor(name, subpats) => {
            let scheme = inf.constructor_scheme(name).ok_or_else(|| {
                TypeError::new(format!("unknown constructor `{name}`"), pat.1.clone())
            })?;
            let instantiated = inf.instantiate(&scheme);
            let (arg_types, result_type) = uncurry(&instantiated);
            if arg_types.len() != subpats.len() {
                return Err(TypeError::new(
                    format!(
                        "constructor `{name}` expects {} argument(s), found {}",
                        arg_types.len(),
                        subpats.len()
                    ),
                    pat.1.clone(),
                ));
            }
            inf.unify(scrutinee, &result_type, &pat.1)?;
            for (subpat, arg_type) in subpats.iter().zip(arg_types.iter()) {
                infer_pattern(inf, env, subpat, arg_type)?;
            }
            Ok(())
        }
    }
}

fn uncurry(ty: &Type) -> (Vec<Type>, Type) {
    let mut args = Vec::new();
    let mut cur = ty.clone();
    while let Type::Fun(a, b) = cur {
        args.push(*a);
        cur = *b;
    }
    (args, cur)
}

// ---------------------------------------------------------------------------
// Exhaustiveness + redundancy checking (ADR 0005).
//
// Maranget's (2007) "usefulness" algorithm, restricted to emet's pattern
// language. `useful(matrix, q)` asks: is there a value matched by `q` but by no
// row already in `matrix`? Two consequences fall out and together preserve
// totality:
//   * an arm is REDUNDANT if it is not useful against the arms above it;
//   * a `case` is NON-EXHAUSTIVE if a bare wildcard is still useful after all
//     arms — i.e. some value escapes every arm.
// The checker guarantees a matching arm always exists, which is why `eval`'s
// no-match path is `unreachable!`.
// ---------------------------------------------------------------------------

/// A pattern lowered for the usefulness check: variable binds are erased to
/// `Wild` (they do not affect coverage).
#[derive(Clone)]
enum UPat {
    Wild,
    Ctor(String, Vec<UPat>),
    Str(String),
}

fn lower_pattern(pat: &Pattern) -> UPat {
    match pat {
        Pattern::Wildcard | Pattern::Var(_) => UPat::Wild,
        Pattern::Str(s) => UPat::Str(s.clone()),
        Pattern::Ctor(name, subs) => {
            UPat::Ctor(name.clone(), subs.iter().map(|s| lower_pattern(&s.0)).collect())
        }
        // List patterns lower to the synthetic `[]`/`::` constructors, so the
        // Maranget checker treats `List` as an ordinary two-constructor sum:
        // a `case` is exhaustive exactly when it covers both, and a second `[]`
        // (or an arm after a catch-all) is redundant. No list-specific code in
        // the algorithm itself — see `prelude::sum_type_constructors`.
        Pattern::Nil => UPat::Ctor(crate::prelude::NIL.to_string(), vec![]),
        Pattern::Cons(head, tail) => UPat::Ctor(
            crate::prelude::CONS.to_string(),
            vec![lower_pattern(&head.0), lower_pattern(&tail.0)],
        ),
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Head {
    Ctor(String, usize),
    Str(String),
}

fn head_of(pat: &UPat) -> Option<Head> {
    match pat {
        UPat::Wild => None,
        UPat::Ctor(name, subs) => Some(Head::Ctor(name.clone(), subs.len())),
        UPat::Str(s) => Some(Head::Str(s.clone())),
    }
}

/// Maranget's S(head, matrix): keep only rows whose first pattern can match
/// `head`, replacing that column with the constructor's sub-patterns (a
/// wildcard expands to `arity` wildcards). Narrows the problem to "given the
/// scrutinee is `head`, what remains?".
fn specialize(matrix: &[Vec<UPat>], head: &Head) -> Vec<Vec<UPat>> {
    let arity = match head {
        Head::Ctor(_, n) => *n,
        Head::Str(_) => 0,
    };
    let mut out = Vec::new();
    for row in matrix {
        let (first, rest) = row.split_first().expect("non-empty row");
        match first {
            UPat::Wild => {
                let mut new_row = vec![UPat::Wild; arity];
                new_row.extend_from_slice(rest);
                out.push(new_row);
            }
            UPat::Ctor(name, subs) => {
                if let Head::Ctor(hname, _) = head {
                    if name == hname {
                        let mut new_row = subs.clone();
                        new_row.extend_from_slice(rest);
                        out.push(new_row);
                    }
                }
            }
            UPat::Str(s) => {
                if let Head::Str(hs) = head {
                    if s == hs {
                        out.push(rest.to_vec());
                    }
                }
            }
        }
    }
    out
}

/// Maranget's D(matrix): the rows that match values whose first column is not
/// any listed constructor — i.e. rows starting with a wildcard, first column
/// dropped. Used when the columns's head constructors are not a complete set.
fn default_matrix(matrix: &[Vec<UPat>]) -> Vec<Vec<UPat>> {
    matrix
        .iter()
        .filter(|row| matches!(row[0], UPat::Wild))
        .map(|row| row[1..].to_vec())
        .collect()
}

/// The full set of constructors (name + arity) for a sum type, or `None` for
/// a type with no finite constructor set (e.g. `String` — an infinite domain,
/// so a wildcard column is never "complete" and a catch-all is mandatory).
fn complete_signature(inf: &Infer, ty: &Type) -> Option<Vec<(String, usize)>> {
    match inf.prune(ty) {
        Type::Con(name, _) => inf.sum_type_constructors(&name),
        _ => None,
    }
}

fn constructor_arg_types(inf: &mut Infer, ctor: &str, ty: &Type) -> Vec<Type> {
    let scheme = inf.constructor_scheme(ctor).expect("known constructor");
    let instantiated = inf.instantiate(&scheme);
    let (args, result) = uncurry(&instantiated);
    let _ = inf.unify(&result, ty, &(0..0));
    args.iter().map(|a| inf.apply(a)).collect()
}

/// Is `vector` useful against `matrix` — does some value match `vector` but no
/// row of `matrix`? Recurses column by column. For a wildcard column whose
/// matrix heads already cover the type's complete constructor set, it must
/// prove usefulness under *some* constructor (splitting the wildcard); an
/// incomplete set means a wildcard value escapes, so it recurses on the default
/// matrix. `col_types` tracks each column's type to look up constructor sets.
fn useful(inf: &mut Infer, matrix: &[Vec<UPat>], vector: &[UPat], col_types: &[Type]) -> bool {
    if vector.is_empty() {
        return matrix.is_empty();
    }
    let (first, rest) = vector.split_first().unwrap();
    let (col_ty, rest_types) = col_types.split_first().unwrap();
    match first {
        UPat::Ctor(name, subs) => {
            let head = Head::Ctor(name.clone(), subs.len());
            let arg_types = constructor_arg_types(inf, name, col_ty);
            let mut new_vector = subs.clone();
            new_vector.extend_from_slice(rest);
            let mut new_types = arg_types;
            new_types.extend_from_slice(rest_types);
            useful(inf, &specialize(matrix, &head), &new_vector, &new_types)
        }
        UPat::Str(s) => {
            let head = Head::Str(s.clone());
            useful(inf, &specialize(matrix, &head), rest, rest_types)
        }
        UPat::Wild => {
            let heads: Vec<Head> = matrix
                .iter()
                .filter_map(|row| head_of(&row[0]))
                .collect();
            let signature = complete_signature(inf, col_ty);
            let complete = match &signature {
                Some(ctors) => ctors.iter().all(|(name, arity)| {
                    heads.iter().any(|h| *h == Head::Ctor(name.clone(), *arity))
                }),
                None => false,
            };
            if complete {
                let ctors = signature.unwrap();
                for (name, arity) in ctors {
                    let head = Head::Ctor(name.clone(), arity);
                    let arg_types = constructor_arg_types(inf, &name, col_ty);
                    let mut new_vector = vec![UPat::Wild; arity];
                    new_vector.extend_from_slice(rest);
                    let mut new_types = arg_types;
                    new_types.extend_from_slice(rest_types);
                    if useful(inf, &specialize(matrix, &head), &new_vector, &new_types) {
                        return true;
                    }
                }
                false
            } else {
                useful(inf, &default_matrix(matrix), rest, rest_types)
            }
        }
    }
}

/// Reject a `case` that is redundant or non-exhaustive. Arms are added to the
/// matrix one at a time: an arm that is not useful against the arms above it is
/// redundant. After all arms, a still-useful wildcard means some value is
/// unmatched — non-exhaustive, reported with the missing constructors.
fn check_exhaustive(
    inf: &mut Infer,
    scrutinee: &Type,
    arms: &[Arm],
    span: &Span,
) -> Result<(), TypeError> {
    let col_types = vec![inf.apply(scrutinee)];
    let mut matrix: Vec<Vec<UPat>> = Vec::new();
    for arm in arms {
        let row = vec![lower_pattern(&arm.pat.0)];
        if !useful(inf, &matrix, &row, &col_types) {
            return Err(TypeError::new(
                "redundant pattern: this arm can never match",
                arm.pat.1.clone(),
            ));
        }
        matrix.push(row);
    }

    let wildcard = vec![UPat::Wild];
    if useful(inf, &matrix, &wildcard, &col_types) {
        let missing = missing_constructors(inf, scrutinee, &matrix);
        return Err(TypeError::new(
            format!("non-exhaustive `case`: {missing}"),
            span.clone(),
        ));
    }
    Ok(())
}

fn missing_constructors(inf: &Infer, scrutinee: &Type, matrix: &[Vec<UPat>]) -> String {
    match complete_signature(inf, scrutinee) {
        Some(ctors) => {
            let covered: Vec<Head> = matrix.iter().filter_map(|row| head_of(&row[0])).collect();
            let missing: Vec<String> = ctors
                .iter()
                .filter(|(name, arity)| {
                    !covered.iter().any(|h| *h == Head::Ctor(name.clone(), *arity))
                })
                .map(|(name, _)| name.clone())
                .collect();
            if missing.is_empty() {
                "some values are not matched".to_string()
            } else {
                format!("missing constructor(s): {}", missing.join(", "))
            }
        }
        None => "add a `_` catch-all arm to cover all remaining values".to_string(),
    }
}

fn decl_def_sites(decls: &[Decl]) -> HashMap<String, DefSite> {
    decls
        .iter()
        .map(|d| (d.name.clone(), DefSite { span: d.span.clone(), module: None }))
        .collect()
}

fn pattern_def_sites(pat: &Spanned<Pattern>) -> HashMap<String, DefSite> {
    let mut out = HashMap::new();
    collect_pattern_binders(pat, &mut out);
    out
}

fn collect_pattern_binders(pat: &Spanned<Pattern>, out: &mut HashMap<String, DefSite>) {
    match &pat.0 {
        Pattern::Var(name) => {
            out.insert(name.clone(), DefSite { span: pat.1.clone(), module: None });
        }
        Pattern::Ctor(_, subs) => {
            for sub in subs {
                collect_pattern_binders(sub, out);
            }
        }
        Pattern::Cons(head, tail) => {
            collect_pattern_binders(head, out);
            collect_pattern_binders(tail, out);
        }
        _ => {}
    }
}

/// Turn a decl (name, params, body) into an expression type: params become
/// nested lambdas.
fn decl_as_lambda(decl: &Decl) -> Spanned<Expr> {
    let mut e = decl.body.clone();
    for p in decl.params.iter().rev() {
        let span = decl.span.clone();
        e = Spanned(Expr::Lam { param: p.clone(), body: Box::new(e) }, span);
    }
    e
}

/// Infer the declarations by dependency analysis: group them into strongly
/// connected components and infer each group together (ADR 0011). Every member
/// of a group is bound to a fresh monomorphic variable before any body is
/// inferred, so a group's members may reference one another in any direction —
/// this is what makes mutual recursion type-check, and a singleton
/// self-referential group is the self-recursion case. The whole group is
/// generalized only once it is solved, so members are monomorphic *within* the
/// group and polymorphic outside it. Groups come out in dependency order, so a
/// group sees the generalized schemes of the groups it depends on and source
/// order no longer matters for forward references.
fn infer_decls(inf: &mut Infer, env: &TyEnv, decls: &[Decl], top_level: bool) -> Result<TyEnv, TypeError> {
    let mut cur = env.clone();
    for group in crate::depgraph::scc_order(decls) {
        cur = infer_group(inf, &cur, decls, &group, top_level)?;
    }
    Ok(cur)
}

/// Infer one dependency SCC. Every member is first bound to a fresh
/// monomorphic variable, so members may call one another (or themselves) while
/// their bodies are checked; a member's signature, if any, is unified against
/// its inferred body in the same pass. Generalization happens against the
/// *pre-group* `env`, not the body env that carries the members' monomorphic
/// vars — generalizing against the body env would wrongly treat a group-mate's
/// still-unresolved var as generalizable and hand a member a falsely
/// polymorphic type. So members are monomorphic within the group and
/// polymorphic only outside it.
fn infer_group(
    inf: &mut Infer,
    env: &TyEnv,
    decls: &[Decl],
    group: &[usize],
    top_level: bool,
) -> Result<TyEnv, TypeError> {
    let self_tys: Vec<Type> = group.iter().map(|_| inf.fresh()).collect();
    let mut body_env = env.clone();
    for (&idx, self_ty) in group.iter().zip(self_tys.iter()) {
        body_env = body_env.insert(decls[idx].name.clone(), mono(self_ty.clone()));
    }

    let mut inferred_tys: Vec<Type> = Vec::with_capacity(group.len());
    for (&idx, self_ty) in group.iter().zip(self_tys.iter()) {
        let decl = &decls[idx];
        let lam = decl_as_lambda(decl);
        let inferred = infer_expr(inf, &body_env, &lam)?;
        inf.unify(self_ty, &inferred, &decl.span)?;
        if inf.recording() {
            let name_span = decl.span.start..decl.span.start + decl.name.len();
            inf.rec_type(&name_span, &inferred);
        }
        if let Some(sig) = &decl.sig {
            let sig_inst = inf.instantiate_signature(&sig.0);
            inf.unify(&inferred, &sig_inst, &sig.1)?;
        }
        inferred_tys.push(inferred);
    }

    if top_level {
        for inferred in &inferred_tys {
            inf.default_number_vars(inferred);
        }
    }

    let mut cur = env.clone();
    for (&idx, inferred) in group.iter().zip(inferred_tys.iter()) {
        let scheme = inf.generalize(env, inferred);
        cur = cur.insert(decls[idx].name.clone(), scheme);
    }
    Ok(cur)
}

/// Arity of every built-in type constructor: the ground types (arity 0), the
/// glyph/scroll types, and the generic `List`/`Maybe` (arity 1). User `type`
/// decls extend this set; every type reference in a signature or variant field
/// must resolve against one or the other.
fn builtin_type_arity(name: &str) -> Option<usize> {
    match name {
        "String" | "AptPackage" | "SystemdService" | "Filesystem" | "LineInFile" | "Glyph"
        | "Entry" | "Scroll" | "Bool" | "Int" | "Float" | "Order" => Some(0),
        "List" | "Maybe" => Some(1),
        _ => None,
    }
}

/// Assign each type parameter a distinct scheme variable id, drawn from the top
/// of the `u32` space so it never collides with the fresh ids inference mints
/// upward from 0 (the same discipline the prelude's sentinel ids follow). All
/// occurrences of one param name in the declaration share an id.
fn param_var_ids(params: &[String]) -> HashMap<String, u32> {
    params
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), u32::MAX - i as u32))
        .collect()
}

/// Rewrite a signature/variant `Type`, replacing each `Rigid(param)` bound by
/// this declaration with its scheme variable, so the result can be quantified
/// into a `Scheme`. Rigids not among the params are left intact (they are just
/// nullary type references, validated separately).
fn type_with_param_vars(ty: &Type, param_vars: &HashMap<String, u32>) -> Type {
    match ty {
        Type::Rigid(name) => match param_vars.get(name) {
            Some(id) => Type::Var(*id, Constraint::None),
            None => Type::Rigid(name.clone()),
        },
        Type::Con(name, args) => Type::Con(
            name.clone(),
            args.iter().map(|a| type_with_param_vars(a, param_vars)).collect(),
        ),
        Type::Fun(a, b) => Type::Fun(
            Box::new(type_with_param_vars(a, param_vars)),
            Box::new(type_with_param_vars(b, param_vars)),
        ),
        Type::Record(fs, row) => Type::Record(
            fs.iter().map(|(k, v)| (k.clone(), type_with_param_vars(v, param_vars))).collect(),
            row.clone(),
        ),
        Type::Var(v, c) => Type::Var(*v, *c),
    }
}

/// Check that every type constructor named in `ty` is known (built-in or a
/// user type) with a matching arity, and that every `Rigid` is a declared type
/// parameter. `type_arities` is the full set after collecting all user types,
/// so decls may reference each other in any order. `bound` is the set of type
/// parameter names in scope (a variant field or a signature has none beyond its
/// own declaration's params).
fn validate_type_refs(
    ty: &Spanned<Type>,
    type_arities: &HashMap<String, usize>,
    bound: &HashSet<String>,
) -> Result<(), TypeError> {
    validate_type_refs_inner(&ty.0, &ty.1, type_arities, bound)
}

fn validate_type_refs_inner(
    ty: &Type,
    span: &Span,
    type_arities: &HashMap<String, usize>,
    bound: &HashSet<String>,
) -> Result<(), TypeError> {
    match ty {
        Type::Con(name, args) => match type_arities.get(name) {
            Some(arity) if *arity == args.len() => {
                for a in args {
                    validate_type_refs_inner(a, span, type_arities, bound)?;
                }
                Ok(())
            }
            Some(arity) => Err(TypeError::new(
                format!(
                    "type constructor `{name}` expects {arity} argument(s), found {}",
                    args.len()
                ),
                span.clone(),
            )),
            None => Err(TypeError::new(format!("unknown type constructor `{name}`"), span.clone())),
        },
        Type::Fun(a, b) => {
            validate_type_refs_inner(a, span, type_arities, bound)?;
            validate_type_refs_inner(b, span, type_arities, bound)
        }
        Type::Record(fs, _) => {
            for v in fs.values() {
                validate_type_refs_inner(v, span, type_arities, bound)?;
            }
            Ok(())
        }
        Type::Rigid(name) if !bound.contains(name) => Err(TypeError::new(
            format!("unbound type variable `{name}`"),
            span.clone(),
        )),
        _ => Ok(()),
    }
}

/// Register the module's user `type` declarations, before any value decl is
/// inferred. Returns the value type env extended with each variant's
/// value-constructor scheme.
///
/// Two passes, because a variant field may name any type in the module —
/// including the type being declared (`Tree`) or one declared later. The first
/// loop collects every user type's arity (and rejects a name that duplicates
/// another decl or a built-in), so the full arity set exists before anything is
/// validated. The second loop then, per variant: validates its field types
/// against that complete set (`validate_type_refs`), rejects a constructor name
/// that duplicates another or shadows a built-in constructor, and builds the
/// constructor scheme (`f1 -> … -> Name p1 p2`, quantified over the params).
/// The order of decls in the source therefore never matters.
///
/// `imported_types` folds the arities of types this module imported (their
/// names and parameter counts, harvested by the resolver) into the arity set,
/// so a signature may reference an imported `type` alongside the module's own
/// and the built-ins.
fn register_type_decls(
    inf: &mut Infer,
    env: &TyEnv,
    type_decls: &[TypeDecl],
    imported_types: &HashMap<String, usize>,
) -> Result<TyEnv, TypeError> {
    let mut type_arities: HashMap<String, usize> = HashMap::new();
    for td in type_decls {
        if builtin_type_arity(&td.name).is_some() {
            return Err(TypeError::new(
                format!("type `{}` redefines a built-in type", td.name),
                td.span.clone(),
            ));
        }
        if type_arities.insert(td.name.clone(), td.params.len()).is_some() {
            return Err(TypeError::new(
                format!("duplicate type declaration `{}`", td.name),
                td.span.clone(),
            ));
        }
    }
    let mut all_arities = type_arities.clone();
    for (name, arity) in imported_types {
        all_arities.entry(name.clone()).or_insert(*arity);
    }
    for (name, arity) in builtin_types() {
        all_arities.entry(name).or_insert(arity);
    }

    let mut out = env.clone();
    let mut ctor_names: HashSet<String> = HashSet::new();
    for td in type_decls {
        let param_vars = param_var_ids(&td.params);
        let bound: HashSet<String> = td.params.iter().cloned().collect();
        let result_ty = Type::Con(
            td.name.clone(),
            td.params.iter().map(|p| Type::Var(param_vars[p], Constraint::None)).collect(),
        );
        let quantified: Vec<(u32, Constraint)> =
            td.params.iter().map(|p| (param_vars[p], Constraint::None)).collect();
        let members: Vec<(String, usize)> =
            td.variants.iter().map(|v| (v.name.clone(), v.fields.len())).collect();
        inf.user_sum_ctors.insert(td.name.clone(), members);

        for variant in &td.variants {
            if !ctor_names.insert(variant.name.clone())
                || crate::prelude::constructor_scheme(&variant.name).is_some()
            {
                return Err(TypeError::new(
                    format!("duplicate constructor `{}`", variant.name),
                    variant.span.clone(),
                ));
            }
            for field in &variant.fields {
                validate_type_refs(field, &all_arities, &bound)?;
            }
            let mut ctor_ty = result_ty.clone();
            for field in variant.fields.iter().rev() {
                let field_ty = type_with_param_vars(&field.0, &param_vars);
                ctor_ty = Type::Fun(Box::new(field_ty), Box::new(ctor_ty));
            }
            let scheme = Scheme { vars: quantified.clone(), row_vars: vec![], ty: ctor_ty };
            inf.user_ctor_schemes.insert(variant.name.clone(), scheme.clone());
            out = out.insert(variant.name.clone(), scheme);
        }
    }
    Ok(out)
}

fn builtin_types() -> Vec<(String, usize)> {
    [
        "String",
        "AptPackage",
        "SystemdService",
        "Filesystem",
        "LineInFile",
        "Glyph",
        "Entry",
        "Scroll",
        "Bool",
        "Int",
        "Float",
        "Order",
    ]
    .iter()
    .map(|n| (n.to_string(), 0usize))
    .chain([("List".to_string(), 1usize), ("Maybe".to_string(), 1usize)])
    .collect()
}

/// Validate every type constructor referenced in a value decl's signature,
/// now that the full user-type arity set is known — the built-ins, this
/// module's own `type` decls, and `imported_types` (imported type names and
/// their arities), so a signature may name an imported type.
fn validate_signature_refs(
    type_decls: &[TypeDecl],
    decls: &[Decl],
    imported_types: &HashMap<String, usize>,
) -> Result<(), TypeError> {
    let mut arities: HashMap<String, usize> = builtin_types().into_iter().collect();
    for (name, arity) in imported_types {
        arities.insert(name.clone(), *arity);
    }
    for td in type_decls {
        arities.insert(td.name.clone(), td.params.len());
    }
    for decl in decls {
        if let Some(sig) = &decl.sig {
            validate_type_refs(sig, &arities, &signature_rigids(&sig.0))?;
        }
    }
    Ok(())
}

/// The `Rigid` names a signature is allowed to mention: any lowercase type
/// variable it uses. A signature introduces its own type variables implicitly
/// (`id : a -> a`), so they are all in scope for reference validation.
fn signature_rigids(ty: &Type) -> HashSet<String> {
    let mut acc = HashSet::new();
    collect_rigids(ty, &mut acc);
    acc
}

fn collect_rigids(ty: &Type, acc: &mut HashSet<String>) {
    match ty {
        Type::Rigid(name) => {
            acc.insert(name.clone());
        }
        Type::Con(_, args) => {
            for a in args {
                collect_rigids(a, acc);
            }
        }
        Type::Fun(a, b) => {
            collect_rigids(a, acc);
            collect_rigids(b, acc);
        }
        Type::Record(fs, _) => {
            for v in fs.values() {
                collect_rigids(v, acc);
            }
        }
        Type::Var(_, _) => {}
    }
}

/// Type-check a library module against a base env seeded with the interfaces
/// of the modules it imports. Returns the module's full final type env, from
/// which the resolver harvests the schemes of its exposed decls. No `main`
/// requirement: a library never has one (that is enforced elsewhere).
pub fn check_library(
    m: &Module,
    base: TyEnv,
    imported_types: &HashMap<String, usize>,
    imported_ctors: &ImportedConstructors,
) -> Result<TyEnv, TypeError> {
    let mut inf = Infer::default();
    inf.seed_imported_constructors(imported_ctors);
    let env = register_type_decls(&mut inf, &base, &m.type_decls, imported_types)?;
    validate_signature_refs(&m.type_decls, &m.decls, imported_types)?;
    infer_decls(&mut inf, &env, &m.decls, true)
}

/// Type-check an entry module against a base env seeded with its imports,
/// enforcing that `main : List Scroll` is present. Returns the final env plus
/// `main`'s normalized type.
pub fn check_entry(
    m: &Module,
    base: TyEnv,
    imported_types: &HashMap<String, usize>,
    imported_ctors: &ImportedConstructors,
) -> Result<(TyEnv, Type), TypeError> {
    let mut inf = Infer::default();
    inf.seed_imported_constructors(imported_ctors);
    let env = register_type_decls(&mut inf, &base, &m.type_decls, imported_types)?;
    validate_signature_refs(&m.type_decls, &m.decls, imported_types)?;
    let final_env = infer_decls(&mut inf, &env, &m.decls, true)?;
    finish_main(&mut inf, &final_env)
}

/// Public entry: type-check a single-module program, returning the type env (so
/// `main`'s type can be reported) or the first error.
pub fn check_module(m: &Module) -> Result<(TyEnv, Type), TypeError> {
    let mut inf = Infer::default();
    let env = crate::prelude::ty_env();
    let no_imports = HashMap::new();
    let env = register_type_decls(&mut inf, &env, &m.type_decls, &no_imports)?;
    validate_signature_refs(&m.type_decls, &m.decls, &no_imports)?;
    let final_env = infer_decls(&mut inf, &env, &m.decls, true)?;
    finish_main(&mut inf, &final_env)
}

pub fn analyze_module(
    m: &Module,
    base: TyEnv,
    imported_types: &HashMap<String, usize>,
    imported_ctors: &ImportedConstructors,
    imported_defs: HashMap<String, DefSite>,
    file_span: Span,
) -> (Option<TypeError>, QueryIndex) {
    let mut inf = Infer { recorder: Some(Recorder::default()), ..Infer::default() };
    inf.seed_imported_constructors(imported_ctors);

    let error = run_recorded(&mut inf, m, base, imported_types, imported_defs, file_span);

    let mut recorder = inf.recorder.take().expect("recorder present");
    for (span, ty) in std::mem::take(&mut recorder.type_spans) {
        recorder.index.types.push((span, inf.apply(&ty)));
    }
    (error, recorder.index)
}

fn run_recorded(
    inf: &mut Infer,
    m: &Module,
    base: TyEnv,
    imported_types: &HashMap<String, usize>,
    imported_defs: HashMap<String, DefSite>,
    file_span: Span,
) -> Option<TypeError> {
    let env = match register_type_decls(inf, &base, &m.type_decls, imported_types) {
        Ok(env) => env,
        Err(e) => return Some(e),
    };
    if let Err(e) = validate_signature_refs(&m.type_decls, &m.decls, imported_types) {
        return Some(e);
    }
    let mut top_defs = imported_defs;
    top_defs.extend(decl_def_sites(&m.decls));
    let top_scope = inf
        .recorder
        .as_mut()
        .map(|r| r.open_scope(file_span, &base, top_defs));
    let result = infer_decls(inf, &env, &m.decls, true);
    match result {
        Ok(final_env) => {
            if let (Some(r), Some(id)) = (&mut inf.recorder, top_scope) {
                let names: Vec<(String, Scheme)> =
                    final_env.entries().map(|(n, s)| (n.clone(), s.clone())).collect();
                r.index.scope_table.insert(id, names);
            }
            None
        }
        Err(e) => Some(e),
    }
}

fn finish_main(inf: &mut Infer, final_env: &TyEnv) -> Result<(TyEnv, Type), TypeError> {
    let main = final_env.get("main").ok_or_else(|| {
        TypeError::new(
            "module has no `main` declaration",
            0..0,
        )
        .note("add `main = [ ... ]` producing the scroll list")
    })?;
    let main_ty = inf.instantiate(main);
    let main_ty = inf.apply(&main_ty);
    let scroll_list = Type::Con("List".to_string(), vec![con("Scroll")]);
    // Accept `List Scroll`, and also `List t` with an unresolved element (an
    // empty `main = []` leaves the element a free var): both normalize to
    // `List Scroll` for display. Any other shape is rejected.
    let normalized = match &main_ty {
        Type::Con(n, args) if n == "List" && args.len() == 1 => match &args[0] {
            Type::Con(e, ea) if ea.is_empty() && e == "Scroll" => Some(scroll_list),
            Type::Var(_, _) => Some(scroll_list),
            _ => None,
        },
        _ => None,
    };
    match normalized {
        Some(ty) => Ok((final_env.clone(), ty)),
        None => Err(TypeError::new(
            format!("`main` must be `List Scroll` (a list of scrolls), but is `{main_ty}`"),
            0..0,
        )),
    }
}
