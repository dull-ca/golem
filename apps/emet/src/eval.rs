//! Evaluation: a type-checked `Module` -> `Vec<Scroll>` (the value of `main`).
//!
//! Inference has already ruled out every runtime type error, so the many
//! `unreachable!`s here are genuinely unreachable on a module that type-checks —
//! each rests on an inference guarantee (an applied value is a function; a
//! `case` scrutinee matches some arm; a glyph field is a `String`; …).
//! `run_module` assumes its `Module` came through `check_module`.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ast::*;
use crate::ir::{Entry, Glyph, Perms, Scroll};
use crate::prelude;

pub type BuiltinFn = fn(Vec<Value>) -> Value;

pub const RECURSION_LIMIT: u64 = 20_000;

const EVAL_STACK_SIZE: usize = 512 * 1024 * 1024;

pub struct EvalError {
    pub msg: String,
}

/// A runtime value. Beyond the obvious literals and containers:
/// `Data` is a saturated sum-type constructor (`Just 3`, `True`); `Closure` is
/// a user lambda over a captured env; `Builtin` is a prelude function
/// collecting `args` until it reaches `arity`, then calling `run`. An
/// unsaturated constructor is either a `Builtin` (prelude constructors, see
/// `prelude`) or a `Ctor` (user constructors) — both build a `Data` once
/// saturated.
#[derive(Clone)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Glyph(Glyph),
    Scroll(Scroll),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
    Data { ctor: String, args: Vec<Value> },
    Closure { rec: Option<Rc<RecGroup>>, param: String, body: Rc<Spanned<Expr>>, env: Env },
    Builtin { name: String, arity: usize, args: Vec<Value>, run: BuiltinFn },
    /// A user sum-type value constructor collecting its `arity` arguments,
    /// yielding `Data { ctor, args }` once saturated (`apply`). A prelude
    /// constructor instead rides on `Builtin`, whose `run` is a `fn` pointer
    /// baked in at compile time — fine for a fixed constructor set, but user
    /// constructors are only known at runtime, so this variant carries the name
    /// and arity as data, letting one representation serve any user constructor.
    Ctor { ctor: String, arity: usize, args: Vec<Value> },
}

/// A group of mutually recursive declarations sharing one captured environment.
/// A closure built for a group carries the group so that, when it is applied,
/// every member is reconstructed and bound into the environment before the body
/// runs — tying the recursive knot lazily. A singleton group whose sole member
/// references itself is ordinary self-recursion.
pub struct RecGroup {
    members: Vec<RecMember>,
}

struct RecMember {
    name: String,
    param: String,
    body: Rc<Spanned<Expr>>,
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Str(s) => f.debug_tuple("Str").field(s).finish(),
            Value::Int(n) => f.debug_tuple("Int").field(n).finish(),
            Value::Float(x) => f.debug_tuple("Float").field(x).finish(),
            Value::Glyph(g) => f.debug_tuple("Glyph").field(g).finish(),
            Value::Scroll(s) => f.debug_tuple("Scroll").field(s).finish(),
            Value::List(vs) => f.debug_tuple("List").field(vs).finish(),
            Value::Record(m) => f.debug_tuple("Record").field(m).finish(),
            Value::Data { ctor, args } => {
                f.debug_struct("Data").field("ctor", ctor).field("args", args).finish()
            }
            Value::Closure { param, .. } => {
                f.debug_struct("Closure").field("param", param).finish()
            }
            Value::Builtin { name, arity, args, .. } => f
                .debug_struct("Builtin")
                .field("name", name)
                .field("arity", arity)
                .field("args", args)
                .finish(),
            Value::Ctor { ctor, arity, args } => f
                .debug_struct("Ctor")
                .field("ctor", ctor)
                .field("arity", arity)
                .field("args", args)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env(Rc<BTreeMap<String, Value>>);
impl Env {
    fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k)
    }
    pub fn lookup(&self, k: &str) -> Option<Value> {
        self.0.get(k).cloned()
    }
    pub fn insert(&self, k: String, v: Value) -> Env {
        let mut m = (*self.0).clone();
        m.insert(k, v);
        Env(Rc::new(m))
    }
}

fn eval(env: &Env, e: &Spanned<Expr>, depth: &mut u64) -> Result<Value, EvalError> {
    Ok(match &e.0 {
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Int(n) => Value::Int(*n),
        Expr::Float(x) => Value::Float(*x),
        Expr::Var(name) => env.get(name).cloned().unwrap_or_else(|| unreachable!("unbound {name}")),
        Expr::AptPackage(name) => Value::Glyph(Glyph::AptPackage {
            name: as_str(eval(env, name, depth)?),
        }),
        Expr::SystemdService(unit) => Value::Glyph(Glyph::SystemdService {
            unit: as_str(eval(env, unit, depth)?),
        }),
        Expr::Filesystem { path, entry } => {
            let path = as_str(eval(env, path, depth)?);
            let entry = match entry {
                EntryExpr::File { contents, mode } => Entry::File {
                    contents: as_str(eval(env, contents, depth)?),
                    perms: perms_from_mode(as_str(eval(env, mode, depth)?))?,
                },
                EntryExpr::Directory { mode } => Entry::Directory {
                    perms: perms_from_mode(as_str(eval(env, mode, depth)?))?,
                },
                EntryExpr::Symlink { target } => Entry::Symlink {
                    target: as_str(eval(env, target, depth)?),
                },
            };
            Value::Glyph(Glyph::Filesystem { path, entry })
        }
        Expr::LineInFile { path, line } => Value::Glyph(Glyph::LineInFile {
            path: as_str(eval(env, path, depth)?),
            line: as_str(eval(env, line, depth)?),
        }),
        Expr::Scroll { name, glyphs } => Value::Scroll(Scroll {
            name: as_str(eval(env, name, depth)?),
            glyphs: as_glyphs(eval(env, glyphs, depth)?),
        }),
        Expr::Ctor(name) => env.get(name).cloned().unwrap_or_else(|| unreachable!("unbound ctor {name}")),
        Expr::List(items) => {
            let mut vs = Vec::with_capacity(items.len());
            for it in items {
                vs.push(eval(env, it, depth)?);
            }
            Value::List(vs)
        }
        Expr::Lam { param, body } => Value::Closure {
            rec: None,
            param: param.clone(),
            body: Rc::new((**body).clone()),
            env: env.clone(),
        },
        Expr::App(f, x) => {
            let fv = eval(env, f, depth)?;
            let xv = eval(env, x, depth)?;
            apply(fv, xv, depth)?
        }
        Expr::Let { decls, body } => {
            let mut e = env.clone();
            for group in crate::depgraph::scc_order(decls) {
                e = eval_group(&e, decls, &group, depth)?;
            }
            eval(&e, body, depth)?
        }
        Expr::Record(fields) => {
            let mut m = BTreeMap::new();
            for (k, v) in fields {
                m.insert(k.clone(), eval(env, v, depth)?);
            }
            Value::Record(m)
        }
        Expr::Field(base, field) => match eval(env, base, depth)? {
            Value::Record(mut m) => m.remove(field).unwrap_or_else(|| unreachable!()),
            _ => unreachable!("field on non-record"),
        },
        Expr::If { cond, then_, else_ } => match eval(env, cond, depth)? {
            Value::Data { ctor, .. } if ctor == "True" => eval(env, then_, depth)?,
            Value::Data { ctor, .. } if ctor == "False" => eval(env, else_, depth)?,
            _ => unreachable!("if condition is not a Bool"),
        },
        Expr::Case { scrutinee, arms } => {
            let value = eval(env, scrutinee, depth)?;
            for arm in arms {
                let mut bindings = Vec::new();
                if match_pattern(&arm.pat.0, &value, &mut bindings) {
                    let mut arm_env = env.clone();
                    for (name, v) in bindings {
                        arm_env = arm_env.insert(name, v);
                    }
                    return eval(&arm_env, &arm.body, depth);
                }
            }
            unreachable!("non-exhaustive case")
        }
    })
}

fn match_pattern(pat: &Pattern, value: &Value, bindings: &mut Vec<(String, Value)>) -> bool {
    match pat {
        Pattern::Wildcard => true,
        Pattern::Var(name) => {
            bindings.push((name.clone(), value.clone()));
            true
        }
        Pattern::Str(s) => matches!(value, Value::Str(v) if v == s),
        Pattern::Nil => matches!(value, Value::List(items) if items.is_empty()),
        Pattern::Cons(head, tail) => match value {
            Value::List(items) => match items.split_first() {
                Some((first, rest)) => {
                    match_pattern(&head.0, first, bindings)
                        && match_pattern(&tail.0, &Value::List(rest.to_vec()), bindings)
                }
                None => false,
            },
            _ => false,
        },
        Pattern::Ctor(name, subpats) => match value {
            Value::Data { ctor, args } if ctor == name && args.len() == subpats.len() => subpats
                .iter()
                .zip(args.iter())
                .all(|(p, a)| match_pattern(&p.0, a, bindings)),
            Value::Glyph(g) => match_reified(name, subpats, glyph_reified(g), bindings),
            _ => false,
        },
    }
}

/// Project a built [`Glyph`] into the `(tag, record)` a constructor pattern
/// matches against (ADR 0017). A `Glyph` is not itself a `Value::Data`, so
/// matching one reconstructs, on demand and read-only, the same shape
/// `glyph_ctors` types the pattern at: the PascalCase tag plus a record of the
/// glyph's fields. Nothing here mutates or rebuilds the glyph — it is a view
/// used only to answer "which variant, and what fields?" during a match.
fn glyph_reified(g: &Glyph) -> (&'static str, Value) {
    match g {
        Glyph::AptPackage { name } => {
            ("AptPackage", record_value(&[("name", Value::Str(name.clone()))]))
        }
        Glyph::SystemdService { unit } => {
            ("SystemdService", record_value(&[("unit", Value::Str(unit.clone()))]))
        }
        Glyph::Filesystem { path, entry } => (
            "Filesystem",
            record_value(&[("path", Value::Str(path.clone())), ("entry", entry_value(entry))]),
        ),
        Glyph::LineInFile { path, line } => (
            "LineInFile",
            record_value(&[("path", Value::Str(path.clone())), ("line", Value::Str(line.clone()))]),
        ),
    }
}

/// A `Filesystem` glyph's `entry` projects as a `Value::Data`, not a bare
/// record, so it re-enters `match_pattern`'s sum-match arm exactly like a
/// user constructor: a `File`/`Directory`/`Symlink` pattern discriminates the
/// tag and binds the arm's field record. This is what lets a `case` nest a
/// match on the entry inside a match on the glyph.
fn entry_value(entry: &Entry) -> Value {
    let (ctor, fields): (&str, Value) = match entry {
        Entry::File { contents, perms } => (
            "File",
            record_value(&[
                ("contents", Value::Str(contents.clone())),
                ("perms", perms_value(perms)),
            ]),
        ),
        Entry::Directory { perms } => ("Directory", record_value(&[("perms", perms_value(perms))])),
        Entry::Symlink { target } => {
            ("Symlink", record_value(&[("target", Value::Str(target.clone()))]))
        }
    };
    Value::Data { ctor: ctor.to_string(), args: vec![fields] }
}

/// Reify typed [`Perms`] back to the surface shape a pattern binds: `mode` as
/// an `Int` (the `u16` widened), `owner`/`group` as `Maybe String` (`Some` ->
/// `Just`, `None` -> `Nothing`, via `maybe_str_value`). The inverse of the
/// `perms_from_mode` lowering, projected for matching rather than storage.
fn perms_value(perms: &Perms) -> Value {
    record_value(&[
        ("mode", Value::Int(perms.mode as i64)),
        ("owner", maybe_str_value(&perms.owner)),
        ("group", maybe_str_value(&perms.group)),
    ])
}

fn maybe_str_value(s: &Option<String>) -> Value {
    match s {
        Some(v) => Value::Data { ctor: "Just".to_string(), args: vec![Value::Str(v.clone())] },
        None => Value::Data { ctor: "Nothing".to_string(), args: Vec::new() },
    }
}

fn record_value(fields: &[(&str, Value)]) -> Value {
    Value::Record(fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

fn match_reified(
    name: &str,
    subpats: &[Spanned<Pattern>],
    reified: (&str, Value),
    bindings: &mut Vec<(String, Value)>,
) -> bool {
    let (tag, fields) = reified;
    tag == name && subpats.len() == 1 && match_pattern(&subpats[0].0, &fields, bindings)
}

/// Apply a function value to one argument. A closure substitutes and evaluates
/// its body; a builtin accumulates the argument and, once saturated (arg count
/// reaches its arity), runs — otherwise it stays partially applied. This is the
/// single apply path both user lambdas and higher-order builtins go through.
pub fn apply_top(func: Value, arg: Value) -> Value {
    let mut depth = 0;
    match apply(func, arg, &mut depth) {
        Ok(v) => v,
        Err(e) => panic!("{}", e.msg),
    }
}

pub fn apply(func: Value, arg: Value, depth: &mut u64) -> Result<Value, EvalError> {
    match func {
        Value::Closure { rec, param, body, env: captured } => {
            *depth += 1;
            if *depth > RECURSION_LIMIT {
                return Err(EvalError {
                    msg: "evaluation exceeded recursion limit (possible infinite recursion)"
                        .to_string(),
                });
            }
            let self_bound = match &rec {
                Some(group) => bind_group(&captured, group),
                None => captured,
            };
            let result = eval(&self_bound.insert(param, arg), &body, depth);
            *depth -= 1;
            result
        }
        Value::Builtin { name, arity, mut args, run } => {
            args.push(arg);
            Ok(if args.len() == arity {
                run(args)
            } else {
                Value::Builtin { name, arity, args, run }
            })
        }
        Value::Ctor { ctor, arity, mut args } => {
            args.push(arg);
            Ok(if args.len() == arity {
                Value::Data { ctor, args }
            } else {
                Value::Ctor { ctor, arity, args }
            })
        }
        _ => unreachable!("applied non-function"),
    }
}

/// Reconstruct every member of a recursive group as a closure carrying the same
/// group and the shared captured environment, then bind each under its name.
/// Injecting the whole group before a member's body runs is what lets the
/// members call one another, generalizing self-recursion to mutual recursion.
fn bind_group(captured: &Env, group: &Rc<RecGroup>) -> Env {
    let mut env = captured.clone();
    for member in &group.members {
        env = env.insert(
            member.name.clone(),
            Value::Closure {
                rec: Some(group.clone()),
                param: member.param.clone(),
                body: member.body.clone(),
                env: captured.clone(),
            },
        );
    }
    env
}

fn decl_as_curried(d: &Decl) -> Spanned<Expr> {
    let mut e = d.body.clone();
    for p in d.params.iter().rev() {
        e = Spanned(Expr::Lam { param: p.clone(), body: Box::new(e) }, d.span.clone());
    }
    e
}

/// Evaluate one dependency group into `env`. A recursive group (a mutually
/// recursive set, or a self-referential singleton) whose members are all
/// functions builds a shared `RecGroup` so each member's closure can see the
/// others when applied; any other group is evaluated member by member against
/// an environment that already carries the groups it depends on.
fn eval_group(
    env: &Env,
    decls: &[Decl],
    group: &[usize],
    depth: &mut u64,
) -> Result<Env, EvalError> {
    if crate::depgraph::group_is_recursive(decls, group) {
        if let Some(rec_env) = eval_recursive_group(env, decls, group) {
            return Ok(rec_env);
        }
    }
    let mut cur = env.clone();
    for &idx in group {
        let value = eval(&cur, &decl_as_curried(&decls[idx]), depth)?;
        cur = cur.insert(decls[idx].name.clone(), value);
    }
    Ok(cur)
}

fn eval_recursive_group(env: &Env, decls: &[Decl], group: &[usize]) -> Option<Env> {
    let mut members = Vec::with_capacity(group.len());
    for &idx in group {
        let curried = decl_as_curried(&decls[idx]);
        match curried.0 {
            Expr::Lam { param, body } => members.push(RecMember {
                name: decls[idx].name.clone(),
                param,
                body: Rc::new(*body),
            }),
            _ => return None,
        }
    }
    let group = Rc::new(RecGroup { members });
    Some(bind_group(env, &group))
}

fn as_str(v: Value) -> String {
    match v {
        Value::Str(s) => s,
        _ => unreachable!("expected Str"),
    }
}

/// Lower a surface `mode` string to typed [`Perms`]. Accepts an optional `0o`
/// prefix and parses the rest as octal into the 12 permission bits; a
/// non-octal or out-of-range (`> 0o7777`) mode is an eval error — this is where
/// a malformed mode becomes a compile-time failure rather than a reconcile-time
/// one (ADR 0019 §1). `owner`/`group` default to `None`; the surface
/// constructors do not expose them yet, so every authored entry leaves ownership
/// unmanaged.
fn perms_from_mode(mode: String) -> Result<Perms, EvalError> {
    let digits = mode.strip_prefix("0o").unwrap_or(&mode);
    let bits = u16::from_str_radix(digits, 8).map_err(|e| EvalError {
        msg: format!("invalid mode `{mode}`: {e}"),
    })?;
    if bits > 0o7777 {
        return Err(EvalError {
            msg: format!("invalid mode `{mode}`: out of range"),
        });
    }
    Ok(Perms {
        mode: bits,
        owner: None,
        group: None,
    })
}

/// Evaluate the module's `main` to a scroll list.
pub fn run_module(m: &Module) -> Result<Vec<Scroll>, EvalError> {
    let m = m.clone();
    std::thread::Builder::new()
        .stack_size(EVAL_STACK_SIZE)
        .spawn(move || {
            let env = eval_module_env(&m, prelude::env())?;
            Ok(main_scrolls(&env))
        })
        .expect("spawn eval thread")
        .join()
        .expect("eval thread panicked")
}

/// Run `work` on a fresh thread with the evaluation stack size, so a
/// multi-module resolution (which evaluates several modules and keeps their
/// non-`Send` value envs across the pass) gets the same deep-recursion headroom
/// as `run_module` without moving those envs across a thread boundary.
pub fn on_eval_thread<T, F>(work: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(EVAL_STACK_SIZE)
        .spawn(work)
        .expect("spawn eval thread")
        .join()
        .expect("eval thread panicked")
}

/// Evaluate a library module against a base env carrying the values of its
/// imports, returning the full resulting env so the resolver can harvest the
/// values of its exposed decls. Assumes it already runs on an eval thread.
pub fn eval_library(m: &Module, base: Env) -> Result<Env, EvalError> {
    eval_module_env(m, base)
}

/// Evaluate an entry module against a base env carrying its imports' values,
/// returning `main` as a scroll list. Assumes it already runs on an eval thread.
pub fn eval_entry(m: &Module, base: Env) -> Result<Vec<Scroll>, EvalError> {
    let env = eval_module_env(m, base)?;
    Ok(main_scrolls(&env))
}

pub fn prelude_env() -> Env {
    prelude::env()
}

fn main_scrolls(env: &Env) -> Vec<Scroll> {
    match env.get("main") {
        Some(Value::List(items)) => items.iter().map(as_scroll).collect(),
        Some(Value::Scroll(s)) => vec![s.clone()],
        _ => vec![],
    }
}

fn eval_module_env(m: &Module, base: Env) -> Result<Env, EvalError> {
    let mut env = base;
    // Seed every user constructor into the env before value decls, so a decl
    // body can refer to one. A nullary variant (`Leaf`) is already a complete
    // value, so it binds directly to `Data`; a variant with fields binds to a
    // `Ctor` that becomes `Data` once applied to all of them (`apply`).
    for td in &m.type_decls {
        for variant in &td.variants {
            let value = if variant.fields.is_empty() {
                Value::Data { ctor: variant.name.clone(), args: Vec::new() }
            } else {
                Value::Ctor {
                    ctor: variant.name.clone(),
                    arity: variant.fields.len(),
                    args: Vec::new(),
                }
            };
            env = env.insert(variant.name.clone(), value);
        }
    }
    let mut depth = 0;
    for group in crate::depgraph::scc_order(&m.decls) {
        env = eval_group(&env, &m.decls, &group, &mut depth)?;
    }
    Ok(env)
}

fn as_glyphs(v: Value) -> Vec<Glyph> {
    match v {
        Value::List(items) => items.iter().map(as_glyph).collect(),
        _ => unreachable!("expected List of Glyph"),
    }
}

fn as_glyph(v: &Value) -> Glyph {
    match v {
        Value::Glyph(g) => g.clone(),
        _ => unreachable!("expected Glyph in glyph list"),
    }
}

fn as_scroll(v: &Value) -> Scroll {
    match v {
        Value::Scroll(s) => s.clone(),
        _ => unreachable!("expected Scroll in main list"),
    }
}
