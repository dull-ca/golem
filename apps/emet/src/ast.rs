//! The AST and the type language shared by every stage after parsing.
//!
//! The parser builds `Expr`/`Decl`/`Module`; `infer.rs` reads them and
//! manipulates `Type`/`Scheme`; `eval.rs` reads them again to produce the IR.

use std::collections::BTreeMap;
use std::ops::Range;

/// Byte range into the original source, carried on every node for diagnostics.
pub type Span = Range<usize>;

/// A value paired with the source span it came from.
#[derive(Debug, Clone)]
pub struct Spanned<T>(pub T, pub Span);

// ---------------------------------------------------------------------------
// Types (with unification variables for Algorithm W)
// ---------------------------------------------------------------------------

/// A type, in the uniform applied-constructor representation of ADR 0003.
/// Every concrete type is a `Con`: `String` is `Con("String", [])`, `List a`
/// is `Con("List", [a])`. There are no fixed type heads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// A unification variable, minted fresh during inference and resolved via
    /// the substitution. Carries a `Constraint` bound (Elm's `number` /
    /// `comparable`) so, e.g., a generalized `number` re-instantiates bounded.
    Var(u32, Constraint),
    /// A signature type variable (`a`, `b`) while checking one declaration's
    /// annotation. Rigid because it must not unify with anything but itself;
    /// instantiated to a fresh `Var` at the signature boundary and never seen
    /// by general inference.
    Rigid(String),
    /// A fully-applied type constructor: name plus its arguments, arity =
    /// `args.len()`. Nullary (`Con("Int", [])`) covers every ground type;
    /// applied (`Con("Maybe", [a])`) covers the generic ones.
    Con(String, Vec<Type>),
    Fun(Box<Type>, Box<Type>),
    /// A record: a set of known field types plus a `Row` saying whether that
    /// set is complete. A record literal produces a `Closed` record (exactly
    /// these fields); field access `.f` produces an `Open` record (at least
    /// `f`, and whatever else the row-tail variable stands for). See ADR 0010.
    Record(BTreeMap<String, Type>, Row),
    /// A tuple type `(A, B)` / `(A, B, C)`, or unit `()` when empty. The
    /// positional, fixed-arity product mirroring `Record` — a `Vec` (order is
    /// the identity, not field names) with no `Row` tail, since a tuple's arity
    /// is never open (ADR 0027).
    Tuple(Vec<Type>),
}

/// The tail of a record type — what, if anything, sits beyond its known
/// fields (ADR 0010, the Elm row model).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// Exactly the known fields, no more. Record literals are closed.
    Closed,
    /// At least the known fields; the row variable stands for the unknown
    /// rest. This is what lets `\h -> h.name` accept any record that has a
    /// `name`. Row variables live in their own id space, resolved through
    /// `infer.rs`'s `row_subst`, never the ordinary type substitution.
    Open(u32),
}

/// A bound on a unification variable — the one place emet leaves pure
/// Hindley-Milner (ADR 0007). A closed, built-in set, not user-extensible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// Ordinary unconstrained variable.
    None,
    /// Elm's `number`: admits `Int` and `Float`. Integer literals get this.
    Number,
    /// Elm's `comparable`: admits `Int`, `Float`, `String`. Comparison and
    /// equality operators require it.
    Comparable,
    /// Elm's `appendable`: admits `String` and `List a`. The `++` operator
    /// requires it.
    Appendable,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Var(n, _) => write!(f, "t{n}"),
            Type::Rigid(name) => write!(f, "{name}"),
            Type::Con(name, args) if args.is_empty() => write!(f, "{name}"),
            Type::Con(name, args) => {
                write!(f, "{name}")?;
                for arg in args {
                    match arg {
                        Type::Con(_, inner) if !inner.is_empty() => write!(f, " ({arg})")?,
                        Type::Fun(_, _) => write!(f, " ({arg})")?,
                        _ => write!(f, " {arg}")?,
                    }
                }
                Ok(())
            }
            Type::Fun(a, b) => match **a {
                Type::Fun(_, _) => write!(f, "({a}) -> {b}"),
                _ => write!(f, "{a} -> {b}"),
            },
            Type::Record(fields, row) => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} : {v}")?;
                }
                if let Row::Open(r) = row {
                    write!(f, " | r{r}")?;
                }
                write!(f, " }}")
            }
            // `(A, B)` / `(A, B, C)`, and `()` for the empty tuple (unit).
            Type::Tuple(elems) => {
                write!(f, "(")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// A type scheme: `∀ vars . ty` — the result of generalization. Each
/// quantified variable carries its `Constraint` so re-instantiation preserves
/// the bound (a `number` stays a `number`).
#[derive(Debug, Clone)]
pub struct Scheme {
    pub vars: Vec<(u32, Constraint)>,
    /// Quantified row variables, kept separate from `vars` because they range
    /// over record tails, not types, and refresh through a distinct mapping at
    /// instantiation. Quantifying them is what makes a polymorphic
    /// `\h -> h.name` reusable at several record shapes (ADR 0010).
    pub row_vars: Vec<u32>,
    pub ty: Type,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

/// The surface expression language.
///
/// Two things that look like they might be here are not: string interpolation
/// desugars to `App(Var("String.concat"), List[...])` in the parser, and infix
/// operators desugar to builtin applications (`a + b` → `add a b`). So
/// inference and evaluation never see interpolation or operator nodes — only
/// `App`/`List`/`Str` (ADR 0004, ADR 0007).
#[derive(Debug, Clone)]
pub enum Expr {
    Str(String),
    /// Integer literal. Typed as a fresh `number` variable, defaulting to
    /// `Int` if never resolved otherwise (ADR 0007).
    Int(i64),
    /// Float literal. Always `Float`.
    Float(f64),
    /// Char literal `'c'` — exactly one Unicode scalar. Typed `Con("Char")`,
    /// mirroring `Str`'s `Con("String")` (ADR 0025). Char *patterns* (`'c'` in a
    /// `case`) are the `Pattern::Char` arm (ADR 0026).
    Char(char),
    Var(String),
    /// `aptPackage { name = e }` — the apt-package primitive constructor.
    /// Reserved lowercase word; parsed as this variant, not a record. One of
    /// the four glyph constructors (ADR 0002).
    AptPackage(Box<Spanned<Expr>>),
    /// `systemdService { unit = e }` — the systemd-unit primitive constructor.
    SystemdService(Box<Spanned<Expr>>),
    /// `file { path, contents, mode }` / `directory { path, mode }` /
    /// `symlink { path, target }` — the three surface spellings of the one
    /// filesystem glyph, differing in the `entry` arm they build. Contents are a
    /// concrete evaluated `String`, never a template (ADR 0004).
    Filesystem {
        path: Box<Spanned<Expr>>,
        entry: EntryExpr,
    },
    /// `lineInFile { path = …, line = … }` — a single-line glyph.
    LineInFile {
        path: Box<Spanned<Expr>>,
        line: Box<Spanned<Expr>>,
    },
    /// `scroll { name = …, policy = …, notifies = …, glyphs | groups = … }` — a
    /// node in the recursive scroll tree (ADR 0031 §7). A leaf carries `glyphs`,
    /// a branch carries named sub-`groups`; `contents` holds exactly one, never
    /// both. `policy` is optional and cascades to the leaves beneath; `notifies`
    /// is optional, a `List String` of systemd units, and unions downward
    /// instead of cascading (ADR 0036). Not a glyph; the program's output bottom
    /// is a `List Scroll` of per-host roots (ADR 0009).
    Scroll {
        name: Box<Spanned<Expr>>,
        policy: Option<Box<Spanned<Expr>>>,
        notifies: Option<Box<Spanned<Expr>>>,
        contents: ContentsExpr,
    },
    /// The braceless policy shorthand `rollback` / `keep` — an `on_exhaust`
    /// choice with the other retry knobs left to default (ADR 0031 §3).
    PolicyExhaust(OnExhaustTag),
    /// `retry { maxAttempts = …, onExhaust = keep, … }` — the full policy record
    /// carrying the ADR 0029 §3 retry knobs. Fields are validated in `infer`.
    PolicyRetry(BTreeMap<String, Spanned<Expr>>),
    List(Vec<Spanned<Expr>>),
    /// A reference to a sum-type value constructor (`Just`, `Nothing`, `True`,
    /// `LT`, …). Distinct from `Var`: constructors live in the prelude and,
    /// when saturated, evaluate to `Value::Data` (ADR 0005).
    Ctor(String),
    Lam {
        param: Spanned<Pattern>,
        body: Box<Spanned<Expr>>,
    },
    App(Box<Spanned<Expr>>, Box<Spanned<Expr>>),
    /// `let <decls> in <body>`. Decls are not strictly left-to-right: like
    /// top-level bindings they are grouped into dependency SCCs (`depgraph`) and
    /// each group inferred/evaluated together, so a `let` may bind mutually
    /// recursive helpers and reference them in any order (ADR 0011).
    Let {
        decls: Vec<Decl>,
        body: Box<Spanned<Expr>>,
    },
    /// `{ a = e1, b = e2 }` — a record literal. Infers to a `Closed` record
    /// (ADR 0010).
    Record(BTreeMap<String, Spanned<Expr>>),
    RecordUpdate {
        base: Box<Spanned<Expr>>,
        fields: Vec<(Spanned<String>, Spanned<Expr>)>,
    },
    /// A tuple literal `(a, b)` / `(a, b, c)`, or unit `()` when empty
    /// (ADR 0027).
    Tuple(Vec<Spanned<Expr>>),
    /// `e.field` — record field access. Infers to an `Open` record demanding
    /// just `field`, so it accepts any record carrying that field, not only a
    /// concrete one (ADR 0010).
    Field(Box<Spanned<Expr>>, String),
    /// `case scrut of` — the sole elimination form. Exhaustiveness and
    /// redundancy are checked at compile time, so no arm can fall through at
    /// runtime (ADR 0005).
    Case {
        scrutinee: Box<Spanned<Expr>>,
        arms: Vec<Arm>,
    },
    /// `if c then t else e`, where `c` is `Bool`. Modeled on a two-arm
    /// `True`/`False` `case` (ADR 0005) but kept as its own node; inference and
    /// evaluation handle it directly.
    If {
        cond: Box<Spanned<Expr>>,
        then_: Box<Spanned<Expr>>,
        else_: Box<Spanned<Expr>>,
    },
}

/// The `entry` arm of a [`Expr::Filesystem`], mirroring `scroll_format::Entry`:
/// each spelling carries only the fields its arm gives meaning. `mode` on the
/// surface is an octal `String` lowered to a `u16` in `eval`.
#[derive(Debug, Clone)]
pub enum EntryExpr {
    File {
        contents: Box<Spanned<Expr>>,
        mode: Box<Spanned<Expr>>,
    },
    Directory {
        mode: Box<Spanned<Expr>>,
    },
    Symlink {
        target: Box<Spanned<Expr>>,
    },
}

/// The `glyphs`-xor-`groups` arm of a [`Expr::Scroll`], mirroring
/// `scroll_format::Contents` at the surface: a scroll is a leaf (a `List Glyph`)
/// or a branch (a `List Scroll`), never a mix. `build_constructor` (`parser.rs`)
/// enforces the exclusion (ADR 0031 §7).
#[derive(Debug, Clone)]
pub enum ContentsExpr {
    Glyphs(Box<Spanned<Expr>>),
    Groups(Box<Spanned<Expr>>),
}

/// The `on_exhaust` choice a [`Expr::PolicyExhaust`] carries — the two braceless
/// policy words, lowered to `scroll_format::OnExhaust` in `eval` (ADR 0031 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnExhaustTag {
    Rollback,
    Keep,
}

/// One `pattern -> body` arm of a `case`.
#[derive(Debug, Clone)]
pub struct Arm {
    pub pat: Spanned<Pattern>,
    pub body: Spanned<Expr>,
}

/// A `case` pattern. The small pattern language the exhaustiveness checker in
/// `infer.rs` reasons over.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,
    /// A lowercase name — matches anything and binds it.
    Var(String),
    /// A string literal — matches an equal `String`.
    Str(String),
    // NOTE: there is deliberately no `Pattern::Float`. Float literals in pattern
    // position are rejected at parse time (ADR 0026 §3), so no stage past the
    // parser ever reasons about float equality.
    /// An integer literal — matches an equal integer. Typed as a `number`
    /// variable defaulting to `Int`, mirroring integer literal *expressions*
    /// rather than hard-`Int` (ADR 0026 §4, ADR 0007), so `case x of 0 -> …`
    /// still typechecks against a `Float` scrutinee. A leading `-` is folded at
    /// parse time, so `-1` arrives here as `Int(-1)`.
    Int(i64),
    /// A char literal `'c'` — matches an equal `Char`; its scrutinee unifies
    /// with `Con("Char")`, exactly as `Str` does with `String` (ADR 0026,
    /// ADR 0025).
    Char(char),
    /// `Upper p1 p2 …` — a constructor applied to sub-patterns.
    Ctor(String, Vec<Spanned<Pattern>>),
    /// `[]` — matches the empty list.
    Nil,
    /// `(head :: tail)` — matches a non-empty list, binding its head element
    /// and its tail list. A `[a, b, c]` literal desugars to nested `Cons`
    /// ending in `Nil`.
    Cons(Box<Spanned<Pattern>>, Box<Spanned<Pattern>>),
    /// `(a, b)` / `(a, b, c)`, or unit `()` when empty — the single-shape
    /// product. Unlike a sum, a tuple has exactly one constructor, so a tuple
    /// `case` is exhaustive iff its element patterns are (ADR 0027).
    Tuple(Vec<Spanned<Pattern>>),
}

/// A top-level or `let` binding, optionally preceded by a matching signature:
///
/// ```text
/// name : Type          -- optional; must immediately precede its binding
/// name p1 p2 = body
/// ```
///
/// `params` are desugared into nested lambdas before inference/eval, so a
/// declaration is just a name bound to an expression.
#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    pub sig: Option<Spanned<Type>>,
    pub params: Vec<Spanned<Pattern>>,
    pub body: Spanned<Expr>,
    pub span: Span,
}

/// One variant of a user `type` declaration: a constructor name and the type
/// atoms it carries. `Node (Tree a) a (Tree a)` has three fields; a nullary
/// variant like `Leaf` has none.
#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Spanned<Type>>,
    pub span: Span,
}

/// A user sum-type declaration: `type Name p1 p2 = V1 f1 f2 | V2 | V3 f`. The
/// type constructor `Name` has arity `params.len()`; each `Variant` becomes a
/// first-class value constructor whose result type is `Name p1 p2` (ADR 0005,
/// design 0001 §6).
#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Exposing {
    All,
    Explicit(Vec<Exposed>),
}

#[derive(Debug, Clone)]
pub enum Exposed {
    Value {
        name: String,
        span: Span,
    },
    Type {
        name: String,
        open: bool,
        span: Span,
    },
}

impl Exposed {
    pub fn name(&self) -> &str {
        match self {
            Exposed::Value { name, .. } | Exposed::Type { name, .. } => name,
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            Exposed::Value { span, .. } | Exposed::Type { span, .. } => span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImportExposing {
    None,
    Explicit(Vec<Exposed>),
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub alias: Option<String>,
    pub exposing: ImportExposing,
    pub span: Span,
}

/// A whole module: its user type declarations and its top-level value
/// declarations. The decl named `main` is the program's output and must have
/// type `List Scroll` (ADR 0009).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Option<String>,
    pub exposing: Exposing,
    pub imports: Vec<Import>,
    pub type_decls: Vec<TypeDecl>,
    pub decls: Vec<Decl>,
}
