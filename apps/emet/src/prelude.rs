//! The prelude: the built-in constructors and functions every module starts
//! with. `ty_env` seeds inference and `env` seeds evaluation, both from the
//! same `ctors()` + `builtins()` tables (kept in lockstep — a name in one must
//! be in the other).
//!
//! Why these are builtins, not library source: emet has no user recursion (a
//! totality invariant), so the user *cannot* write iteration over a list. The
//! combinators that would be recursive library functions in Elm (`List.map`,
//! `List.foldr`, …) must therefore be primitive Rust `Value` -> `Value`
//! functions (ADR 0006). Their schemes are Elm-accurate.
//!
//! `List.` / `Maybe.` / `String.` are a naming convention, not a module system
//! — a qualified name like `List.map` is just a dotted `Var` resolved by env
//! lookup (ADR 0006). Numeric/comparison names Elm exposes bare (`round`,
//! `abs`, `min`, `compare`, the operator targets `add`/`lt`/`eq`/…) are bound
//! unqualified (ADR 0007). The sum-type constructors `Just`/`Nothing`,
//! `True`/`False`, `LT`/`EQ`/`GT` live here too (ADR 0005); `Maybe`/`Bool`/
//! `Order` are ordinary types defined by their constructors, not hardcoded into
//! `unify`.

use std::collections::BTreeMap;

use crate::ast::{Constraint, Row, Scheme, Type};
use crate::eval::{apply_top, BuiltinFn, Env, Value};
use crate::infer::TyEnv;

// Sentinel type-variable ids for the polymorphic schemes below. Chosen at the
// top of the u32 space to never collide with the fresh ids inference mints from
// 0 upward. `N`/`C` also carry `number`/`comparable` bounds (see `constraint_for`).
const A: u32 = u32::MAX;
const B: u32 = u32::MAX - 1;

const N: u32 = u32::MAX - 2;
const C: u32 = u32::MAX - 3;
const P: u32 = u32::MAX - 4;
// `X`/`Y` are the extra unconstrained result vars the `Tuple.map*` schemes need
// beyond `A`/`B` — `mapFirst : (a -> x) -> (a, b) -> (x, b)`, and `mapBoth`
// which needs both (ADR 0027 §6).
const X: u32 = u32::MAX - 5;
const Y: u32 = u32::MAX - 6;

fn var(n: u32) -> Type {
    Type::Var(n, Constraint::None)
}

fn number(n: u32) -> Type {
    Type::Var(n, Constraint::Number)
}

fn comparable(n: u32) -> Type {
    Type::Var(n, Constraint::Comparable)
}

fn appendable(n: u32) -> Type {
    Type::Var(n, Constraint::Appendable)
}

fn list(elem: Type) -> Type {
    Type::Con("List".to_string(), vec![elem])
}

fn maybe(elem: Type) -> Type {
    Type::Con("Maybe".to_string(), vec![elem])
}

fn bool_ty() -> Type {
    Type::Con("Bool".to_string(), vec![])
}

fn int() -> Type {
    Type::Con("Int".to_string(), vec![])
}

fn float() -> Type {
    Type::Con("Float".to_string(), vec![])
}

fn string() -> Type {
    Type::Con("String".to_string(), vec![])
}

// The `Char` base type, beside `string()`/`int()` (ADR 0025).
fn char() -> Type {
    Type::Con("Char".to_string(), vec![])
}

fn order() -> Type {
    Type::Con("Order".to_string(), vec![])
}

fn fun(from: Type, to: Type) -> Type {
    Type::Fun(Box::new(from), Box::new(to))
}

// The 2-tuple type `(a, b)`, the shape every `Tuple.*` scheme is written over —
// Elm's `Tuple` module operates only on pairs (ADR 0027 §6).
fn pair(a: Type, b: Type) -> Type {
    Type::Tuple(vec![a, b])
}

fn glyph() -> Type {
    Type::Con("Glyph".to_string(), vec![])
}

fn entry() -> Type {
    Type::Con("Entry".to_string(), vec![])
}

fn policy() -> Type {
    Type::Con("Policy".to_string(), vec![])
}

fn record(fields: &[(&str, Type)]) -> Type {
    Type::Record(
        fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<BTreeMap<_, _>>(),
        Row::Closed,
    )
}

fn perms() -> Type {
    record(&[("mode", int()), ("owner", maybe(string())), ("group", maybe(string()))])
}

fn scheme(vars: &[u32], ty: Type) -> Scheme {
    Scheme {
        vars: vars.iter().map(|v| (*v, constraint_for(*v))).collect(),
        row_vars: vec![],
        ty,
    }
}

/// The bound carried by a sentinel var id: `N` is `number`, `C` is
/// `comparable`, everything else unconstrained. So `scheme(&[N], …)` quantifies
/// over a `number` variable.
fn constraint_for(v: u32) -> Constraint {
    match v {
        N => Constraint::Number,
        C => Constraint::Comparable,
        P => Constraint::Appendable,
        _ => Constraint::None,
    }
}

fn as_list(v: &Value) -> &[Value] {
    match v {
        Value::List(items) => items,
        _ => unreachable!("expected List"),
    }
}

fn as_tuple(v: &Value) -> &[Value] {
    match v {
        Value::Tuple(items) => items,
        _ => unreachable!("expected Tuple"),
    }
}

// The `Tuple` module runtime (ADR 0027 §6): `pair`/`first`/`second`/`mapFirst`/
// `mapSecond`/`mapBoth`, exactly `elm/core`'s `Tuple`. All pair-based — Elm's
// `Tuple` has no 3-tuple accessors, so a triple is destructured by pattern, not
// by these. Inference has already fixed each argument to a 2-tuple, so the
// `as_tuple` indexing is total.

fn tuple_pair(mut args: Vec<Value>) -> Value {
    let b = args.pop().unwrap();
    let a = args.pop().unwrap();
    Value::Tuple(vec![a, b])
}

fn tuple_first(mut args: Vec<Value>) -> Value {
    as_tuple(&args.pop().unwrap())[0].clone()
}

fn tuple_second(mut args: Vec<Value>) -> Value {
    as_tuple(&args.pop().unwrap())[1].clone()
}

fn tuple_map_first(mut args: Vec<Value>) -> Value {
    let t = args.pop().unwrap();
    let f = args.pop().unwrap();
    let elems = as_tuple(&t);
    Value::Tuple(vec![apply_top(f, elems[0].clone()), elems[1].clone()])
}

fn tuple_map_second(mut args: Vec<Value>) -> Value {
    let t = args.pop().unwrap();
    let f = args.pop().unwrap();
    let elems = as_tuple(&t);
    Value::Tuple(vec![elems[0].clone(), apply_top(f, elems[1].clone())])
}

fn tuple_map_both(mut args: Vec<Value>) -> Value {
    let t = args.pop().unwrap();
    let g = args.pop().unwrap();
    let f = args.pop().unwrap();
    let elems = as_tuple(&t);
    Value::Tuple(vec![apply_top(f, elems[0].clone()), apply_top(g, elems[1].clone())])
}

// `String.uncons : String -> Maybe (Char, String)` — the tuple-returning
// function ADR 0025 §4 deferred for want of a pair, and the concrete driver for
// the whole tuple type (ADR 0027 §6). `Nothing` on the empty string; otherwise
// `Just (firstScalar, rest)`, split by Unicode scalar (`chars()`) to stay
// consistent with the rest of the scalar-indexed `String` surface (ADR 0025 §5).
fn string_uncons(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let s = as_string(&s);
    match s.chars().next() {
        Some(first) => {
            let rest: String = s.chars().skip(1).collect();
            data("Just", vec![Value::Tuple(vec![Value::Char(first), Value::Str(rest)])])
        }
        None => data("Nothing", vec![]),
    }
}

fn list_map(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let f = args.pop().unwrap();
    Value::List(as_list(&xs).iter().map(|x| apply_top(f.clone(), x.clone())).collect())
}

fn list_concat(mut args: Vec<Value>) -> Value {
    let xss = args.pop().unwrap();
    let mut out = Vec::new();
    for xs in as_list(&xss) {
        out.extend(as_list(xs).iter().cloned());
    }
    Value::List(out)
}

fn list_concat_map(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mut out = Vec::new();
    for x in as_list(&xs) {
        out.extend(as_list(&apply_top(f.clone(), x.clone())).iter().cloned());
    }
    Value::List(out)
}

fn list_cons(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let head = args.pop().unwrap();
    let mut out = Vec::with_capacity(as_list(&xs).len() + 1);
    out.push(head);
    out.extend(as_list(&xs).iter().cloned());
    Value::List(out)
}

fn list_append(mut args: Vec<Value>) -> Value {
    let ys = args.pop().unwrap();
    let xs = args.pop().unwrap();
    let mut out = as_list(&xs).to_vec();
    out.extend(as_list(&ys).iter().cloned());
    Value::List(out)
}

fn list_foldr(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let init = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mut acc = init;
    for x in as_list(&xs).iter().rev() {
        acc = apply_top(apply_top(f.clone(), x.clone()), acc);
    }
    acc
}

fn list_foldl(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let init = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mut acc = init;
    for x in as_list(&xs) {
        acc = apply_top(apply_top(f.clone(), x.clone()), acc);
    }
    acc
}

fn data(ctor: &str, args: Vec<Value>) -> Value {
    Value::Data { ctor: ctor.to_string(), args }
}

fn as_data(v: &Value) -> (&str, &[Value]) {
    match v {
        Value::Data { ctor, args } => (ctor.as_str(), args.as_slice()),
        _ => unreachable!("expected Data"),
    }
}

fn make_just(mut args: Vec<Value>) -> Value {
    data("Just", vec![args.pop().unwrap()])
}

fn maybe_map(mut args: Vec<Value>) -> Value {
    let m = args.pop().unwrap();
    let f = args.pop().unwrap();
    match as_data(&m) {
        ("Just", inner) => data("Just", vec![apply_top(f, inner[0].clone())]),
        _ => data("Nothing", vec![]),
    }
}

fn maybe_with_default(mut args: Vec<Value>) -> Value {
    let m = args.pop().unwrap();
    let default = args.pop().unwrap();
    match as_data(&m) {
        ("Just", inner) => inner[0].clone(),
        _ => default,
    }
}

fn maybe_and_then(mut args: Vec<Value>) -> Value {
    let m = args.pop().unwrap();
    let f = args.pop().unwrap();
    match as_data(&m) {
        ("Just", inner) => apply_top(f, inner[0].clone()),
        _ => data("Nothing", vec![]),
    }
}

fn list_filter(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let f = args.pop().unwrap();
    let kept = as_list(&xs)
        .iter()
        .filter(|x| matches!(as_data(&apply_top(f.clone(), (*x).clone())), ("True", _)))
        .cloned()
        .collect();
    Value::List(kept)
}

fn list_is_empty(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    data(if as_list(&xs).is_empty() { "True" } else { "False" }, vec![])
}

fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        _ => unreachable!("expected Int"),
    }
}

fn as_float(v: &Value) -> f64 {
    match v {
        Value::Float(x) => *x,
        _ => unreachable!("expected Float"),
    }
}

fn as_string(v: &Value) -> &str {
    match v {
        Value::Str(s) => s.as_str(),
        _ => unreachable!("expected String"),
    }
}

// Unwrap a `Value::Char`, beside `as_string` (ADR 0025).
fn as_char(v: &Value) -> char {
    match v {
        Value::Char(c) => *c,
        _ => unreachable!("expected Char"),
    }
}

fn boolean(b: bool) -> Value {
    data(if b { "True" } else { "False" }, vec![])
}

fn numeric_binop(args: &[Value], on_int: fn(i64, i64) -> i64, on_float: fn(f64, f64) -> f64) -> Value {
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(on_int(*a, *b)),
        (Value::Float(a), Value::Float(b)) => Value::Float(on_float(*a, *b)),
        _ => unreachable!("numeric operands share a type"),
    }
}

fn builtin_add(args: Vec<Value>) -> Value {
    numeric_binop(&args, |a, b| a + b, |a, b| a + b)
}

fn builtin_sub(args: Vec<Value>) -> Value {
    numeric_binop(&args, |a, b| a - b, |a, b| a - b)
}

fn builtin_mul(args: Vec<Value>) -> Value {
    numeric_binop(&args, |a, b| a * b, |a, b| a * b)
}

fn builtin_pow(args: Vec<Value>) -> Value {
    numeric_binop(&args, |a, b| a.pow(b.max(0) as u32), |a, b| a.powf(b))
}

// NOTE: division and modulo are total — dividing by zero returns 0 rather than
// trapping, matching Elm, so evaluation can never crash (ADR 0007).
fn builtin_fdiv(args: Vec<Value>) -> Value {
    let b = as_float(&args[1]);
    Value::Float(if b == 0.0 { 0.0 } else { as_float(&args[0]) / b })
}

fn builtin_idiv(args: Vec<Value>) -> Value {
    let b = as_int(&args[1]);
    Value::Int(if b == 0 { 0 } else { as_int(&args[0]).wrapping_div(b) })
}

fn builtin_mod_by(args: Vec<Value>) -> Value {
    let modulus = as_int(&args[0]);
    let x = as_int(&args[1]);
    Value::Int(if modulus == 0 { 0 } else { x.rem_euclid(modulus) })
}

fn builtin_remainder_by(args: Vec<Value>) -> Value {
    let divisor = as_int(&args[0]);
    let x = as_int(&args[1]);
    Value::Int(if divisor == 0 { 0 } else { x.wrapping_rem(divisor) })
}

fn builtin_negate(mut args: Vec<Value>) -> Value {
    match args.pop().unwrap() {
        Value::Int(n) => Value::Int(-n),
        Value::Float(x) => Value::Float(-x),
        _ => unreachable!("negate on non-number"),
    }
}

fn builtin_abs(mut args: Vec<Value>) -> Value {
    match args.pop().unwrap() {
        Value::Int(n) => Value::Int(n.abs()),
        Value::Float(x) => Value::Float(x.abs()),
        _ => unreachable!("abs on non-number"),
    }
}

fn builtin_to_float(mut args: Vec<Value>) -> Value {
    Value::Float(as_int(&args.pop().unwrap()) as f64)
}

fn builtin_round(mut args: Vec<Value>) -> Value {
    Value::Int(as_float(&args.pop().unwrap()).round() as i64)
}

fn builtin_floor(mut args: Vec<Value>) -> Value {
    Value::Int(as_float(&args.pop().unwrap()).floor() as i64)
}

fn builtin_ceiling(mut args: Vec<Value>) -> Value {
    Value::Int(as_float(&args.pop().unwrap()).ceil() as i64)
}

fn builtin_truncate(mut args: Vec<Value>) -> Value {
    Value::Int(as_float(&args.pop().unwrap()).trunc() as i64)
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        // Codepoint order — Elm's `Char` comparison (ADR 0025).
        (Value::Char(x), Value::Char(y)) => x.cmp(y),
        // Lexicographic: compare element-wise, returning at the first
        // non-`Equal`. Inference guarantees equal arity and comparable elements,
        // so no length or cross-type case arises; unit (empty loop) is `Equal`,
        // matching Elm (ADR 0027 §4).
        (Value::Tuple(xs), Value::Tuple(ys)) => {
            for (x, y) in xs.iter().zip(ys.iter()) {
                let ord = compare_values(x, y);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        }
        _ => unreachable!("comparable operands share a type"),
    }
}

fn builtin_lt(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_lt())
}

fn builtin_gt(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_gt())
}

fn builtin_le(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_le())
}

fn builtin_ge(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_ge())
}

fn builtin_eq(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_eq())
}

fn builtin_neq(args: Vec<Value>) -> Value {
    boolean(compare_values(&args[0], &args[1]).is_ne())
}

fn builtin_min(mut args: Vec<Value>) -> Value {
    let b = args.pop().unwrap();
    let a = args.pop().unwrap();
    if compare_values(&a, &b).is_le() { a } else { b }
}

fn builtin_max(mut args: Vec<Value>) -> Value {
    let b = args.pop().unwrap();
    let a = args.pop().unwrap();
    if compare_values(&a, &b).is_ge() { a } else { b }
}

fn builtin_clamp(mut args: Vec<Value>) -> Value {
    let x = args.pop().unwrap();
    let hi = args.pop().unwrap();
    let lo = args.pop().unwrap();
    if compare_values(&x, &lo).is_lt() {
        lo
    } else if compare_values(&x, &hi).is_gt() {
        hi
    } else {
        x
    }
}

fn builtin_compare(mut args: Vec<Value>) -> Value {
    let b = args.pop().unwrap();
    let a = args.pop().unwrap();
    let ctor = match compare_values(&a, &b) {
        std::cmp::Ordering::Less => "LT",
        std::cmp::Ordering::Equal => "EQ",
        std::cmp::Ordering::Greater => "GT",
    };
    data(ctor, vec![])
}

fn builtin_and(args: Vec<Value>) -> Value {
    let a = matches!(as_data(&args[0]), ("True", _));
    let b = matches!(as_data(&args[1]), ("True", _));
    boolean(a && b)
}

fn builtin_or(args: Vec<Value>) -> Value {
    let a = matches!(as_data(&args[0]), ("True", _));
    let b = matches!(as_data(&args[1]), ("True", _));
    boolean(a || b)
}

fn builtin_not(mut args: Vec<Value>) -> Value {
    boolean(!matches!(as_data(&args.pop().unwrap()), ("True", _)))
}

fn string_append(mut args: Vec<Value>) -> Value {
    let b = args.pop().unwrap();
    let a = args.pop().unwrap();
    Value::Str(format!("{}{}", as_string(&a), as_string(&b)))
}

fn append(args: Vec<Value>) -> Value {
    match &args[0] {
        Value::List(_) => list_append(args),
        _ => string_append(args),
    }
}

fn string_concat(mut args: Vec<Value>) -> Value {
    let parts = args.pop().unwrap();
    let mut out = String::new();
    for part in as_list(&parts) {
        out.push_str(as_string(part));
    }
    Value::Str(out)
}

fn string_join(mut args: Vec<Value>) -> Value {
    let parts = args.pop().unwrap();
    let sep = args.pop().unwrap();
    let joined = as_list(&parts)
        .iter()
        .map(|p| as_string(p))
        .collect::<Vec<_>>()
        .join(as_string(&sep));
    Value::Str(joined)
}

fn string_length(mut args: Vec<Value>) -> Value {
    Value::Int(as_string(&args.pop().unwrap()).chars().count() as i64)
}

fn string_from_int(mut args: Vec<Value>) -> Value {
    Value::Str(as_int(&args.pop().unwrap()).to_string())
}

fn string_from_float(mut args: Vec<Value>) -> Value {
    Value::Str(as_float(&args.pop().unwrap()).to_string())
}

fn string_to_int(mut args: Vec<Value>) -> Value {
    match as_string(&args.pop().unwrap()).parse::<i64>() {
        Ok(n) => data("Just", vec![Value::Int(n)]),
        Err(_) => data("Nothing", vec![]),
    }
}

fn string_to_float(mut args: Vec<Value>) -> Value {
    match as_string(&args.pop().unwrap()).parse::<f64>() {
        Ok(x) => data("Just", vec![Value::Float(x)]),
        Err(_) => data("Nothing", vec![]),
    }
}

fn list_length(mut args: Vec<Value>) -> Value {
    Value::Int(as_list(&args.pop().unwrap()).len() as i64)
}

fn list_range(mut args: Vec<Value>) -> Value {
    let hi = as_int(&args.pop().unwrap());
    let lo = as_int(&args.pop().unwrap());
    Value::List((lo..=hi).map(Value::Int).collect())
}

fn list_sum(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    let items = as_list(&xs);
    match items.first() {
        Some(Value::Float(_)) => {
            Value::Float(items.iter().map(as_float).sum())
        }
        _ => Value::Int(items.iter().map(as_int).sum()),
    }
}

// The Elm-faithful `Char` surface (ADR 0025 §2). Every function mirrors its
// `elm/core` `Char` counterpart in name, signature, and semantics; a `Char` is
// one Unicode scalar, so these agree with the scalar indexing the `String`
// functions below use (`String.length` counts `chars()`). The `is*` predicates
// are Elm's ASCII-oriented definitions. All total.

fn char_to_code(mut args: Vec<Value>) -> Value {
    Value::Int(as_char(&args.pop().unwrap()) as i64)
}

// Total, following Elm: an out-of-range code or a surrogate yields the Unicode
// replacement character `U+FFFD` rather than trapping or returning `Maybe`.
fn char_from_code(mut args: Vec<Value>) -> Value {
    let code = as_int(&args.pop().unwrap());
    let scalar = u32::try_from(code).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}');
    Value::Char(scalar)
}

fn char_to_upper(mut args: Vec<Value>) -> Value {
    let c = as_char(&args.pop().unwrap());
    Value::Char(c.to_uppercase().next().unwrap_or(c))
}

fn char_to_lower(mut args: Vec<Value>) -> Value {
    let c = as_char(&args.pop().unwrap());
    Value::Char(c.to_lowercase().next().unwrap_or(c))
}

fn char_is_upper(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_uppercase())
}

fn char_is_lower(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_lowercase())
}

fn char_is_alpha(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_alphabetic())
}

fn char_is_alpha_num(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_alphanumeric())
}

fn char_is_digit(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_digit())
}

fn char_is_oct_digit(mut args: Vec<Value>) -> Value {
    boolean(matches!(as_char(&args.pop().unwrap()), '0'..='7'))
}

fn char_is_hex_digit(mut args: Vec<Value>) -> Value {
    boolean(as_char(&args.pop().unwrap()).is_ascii_hexdigit())
}

fn char_is_space(mut args: Vec<Value>) -> Value {
    boolean(matches!(
        as_char(&args.pop().unwrap()),
        ' ' | '\t' | '\n' | '\r' | '\u{000B}' | '\u{000C}'
    ))
}

// The Elm-faithful `String` surface (ADR 0025 §3): `elm/core` names, argument
// order, and total/clamping semantics. Every length, index, and slice bound is
// a Unicode scalar index (`chars()`), never a byte or grapheme offset, so these
// agree with `String.length`'s `chars().count()` and with the `Char` functions
// above (ADR 0025 §5). Combining marks and modifier sequences therefore count
// as several scalars — Elm's own caveat, kept.

fn string_is_empty(mut args: Vec<Value>) -> Value {
    boolean(as_string(&args.pop().unwrap()).is_empty())
}

fn string_reverse(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).chars().rev().collect())
}

fn string_repeat(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let n = as_int(&args.pop().unwrap());
    if n <= 0 {
        Value::Str(String::new())
    } else {
        Value::Str(as_string(&s).repeat(n as usize))
    }
}

fn string_replace(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let after = args.pop().unwrap();
    let before = args.pop().unwrap();
    let before = as_string(&before);
    if before.is_empty() {
        return Value::Str(as_string(&s).to_string());
    }
    Value::Str(as_string(&s).replace(before, as_string(&after)))
}

// An empty separator splits into one single-scalar string per character, as in
// Elm; otherwise a plain substring split.
fn string_split(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let sep = args.pop().unwrap();
    let s = as_string(&s);
    let sep = as_string(&sep);
    let parts: Vec<Value> = if sep.is_empty() {
        s.chars().map(|c| Value::Str(c.to_string())).collect()
    } else {
        s.split(sep).map(|p| Value::Str(p.to_string())).collect()
    };
    Value::List(parts)
}

// Trim, then split on runs of whitespace. NOTE: `split_whitespace` keys on
// Rust's `char::is_whitespace` (Unicode), a negligible divergence from Elm's
// JS `\s` set — the two disagree only on exotic separators.
fn string_words(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let trimmed = as_string(&s).trim();
    let parts: Vec<Value> = if trimmed.is_empty() {
        vec![Value::Str(String::new())]
    } else {
        trimmed.split_whitespace().map(|w| Value::Str(w.to_string())).collect()
    };
    Value::List(parts)
}

// Break on line terminators, recognizing `\r\n`, a lone `\r`, and `\n`. Emet
// has no `\r` escape to author a lone `\r`, but it is handled for strings that
// acquire one at runtime.
fn string_lines(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let s = as_string(&s);
    let mut lines = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\r' && i + 1 < chars.len() && chars[i + 1] == '\n' {
            lines.push(Value::Str(std::mem::take(&mut current)));
            i += 2;
        } else if c == '\n' || c == '\r' {
            lines.push(Value::Str(std::mem::take(&mut current)));
            i += 1;
        } else {
            current.push(c);
            i += 1;
        }
    }
    lines.push(Value::Str(current));
    Value::List(lines)
}

// Elm's `slice` bound resolution, shared by `String.slice`: a negative index
// counts from the end (`idx + len`), then both ends clamp into `0..=len`. If
// the resolved bounds cross (`lo >= hi`) the result is empty.
fn scalar_slice(s: &str, start: i64, end: i64) -> String {
    let scalars: Vec<char> = s.chars().collect();
    let len = scalars.len() as i64;
    let resolve = |idx: i64| -> i64 {
        let shifted = if idx < 0 { idx + len } else { idx };
        shifted.clamp(0, len)
    };
    let lo = resolve(start);
    let hi = resolve(end);
    if lo >= hi {
        String::new()
    } else {
        scalars[lo as usize..hi as usize].iter().collect()
    }
}

fn string_slice(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let end = as_int(&args.pop().unwrap());
    let start = as_int(&args.pop().unwrap());
    Value::Str(scalar_slice(as_string(&s), start, end))
}

fn string_left(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let n = as_int(&args.pop().unwrap());
    if n < 1 {
        Value::Str(String::new())
    } else {
        Value::Str(as_string(&s).chars().take(n as usize).collect())
    }
}

fn string_right(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let n = as_int(&args.pop().unwrap());
    if n < 1 {
        return Value::Str(String::new());
    }
    let scalars: Vec<char> = as_string(&s).chars().collect();
    let take = (n as usize).min(scalars.len());
    Value::Str(scalars[scalars.len() - take..].iter().collect())
}

fn string_drop_left(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let n = as_int(&args.pop().unwrap());
    if n < 1 {
        Value::Str(as_string(&s).to_string())
    } else {
        Value::Str(as_string(&s).chars().skip(n as usize).collect())
    }
}

fn string_drop_right(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let n = as_int(&args.pop().unwrap());
    if n < 1 {
        return Value::Str(as_string(&s).to_string());
    }
    let scalars: Vec<char> = as_string(&s).chars().collect();
    let keep = scalars.len().saturating_sub(n as usize);
    Value::Str(scalars[..keep].iter().collect())
}

fn string_contains(mut args: Vec<Value>) -> Value {
    let haystack = args.pop().unwrap();
    let needle = args.pop().unwrap();
    boolean(as_string(&haystack).contains(as_string(&needle)))
}

fn string_starts_with(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let prefix = args.pop().unwrap();
    boolean(as_string(&s).starts_with(as_string(&prefix)))
}

fn string_ends_with(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let suffix = args.pop().unwrap();
    boolean(as_string(&s).ends_with(as_string(&suffix)))
}

// Scalar indices of every (possibly overlapping) match. An empty needle yields
// `[]`, matching Elm. Backs both `String.indexes` and its deliberate alias
// `String.indices`, which share this one function.
fn string_indexes(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let needle = args.pop().unwrap();
    let needle = as_string(&needle);
    if needle.is_empty() {
        return Value::List(Vec::new());
    }
    let haystack: Vec<char> = as_string(&s).chars().collect();
    let pat: Vec<char> = needle.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + pat.len() <= haystack.len() {
        if haystack[i..i + pat.len()] == pat[..] {
            out.push(Value::Int(i as i64));
        }
        i += 1;
    }
    Value::List(out)
}

fn string_to_list(mut args: Vec<Value>) -> Value {
    Value::List(as_string(&args.pop().unwrap()).chars().map(Value::Char).collect())
}

fn string_from_list(mut args: Vec<Value>) -> Value {
    let xs = args.pop().unwrap();
    Value::Str(as_list(&xs).iter().map(as_char).collect())
}

fn string_from_char(mut args: Vec<Value>) -> Value {
    Value::Str(as_char(&args.pop().unwrap()).to_string())
}

fn string_cons(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let c = as_char(&args.pop().unwrap());
    Value::Str(format!("{c}{}", as_string(&s)))
}

fn string_to_upper(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).to_uppercase())
}

fn string_to_lower(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).to_lowercase())
}

fn string_trim(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).trim().to_string())
}

fn string_trim_left(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).trim_start().to_string())
}

fn string_trim_right(mut args: Vec<Value>) -> Value {
    Value::Str(as_string(&args.pop().unwrap()).trim_end().to_string())
}

// Center-pad to width `n`. An odd deficit favors the left: ceil of the half
// goes left, floor goes right, as in Elm.
fn string_pad(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let pad = as_char(&args.pop().unwrap());
    let n = as_int(&args.pop().unwrap());
    let s = as_string(&s);
    let deficit = n - s.chars().count() as i64;
    let half = deficit as f64 / 2.0;
    let left = half.ceil().max(0.0) as usize;
    let right = half.floor().max(0.0) as usize;
    let mut out = String::new();
    out.extend(std::iter::repeat(pad).take(left));
    out.push_str(s);
    out.extend(std::iter::repeat(pad).take(right));
    Value::Str(out)
}

fn string_pad_left(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let pad = as_char(&args.pop().unwrap());
    let n = as_int(&args.pop().unwrap());
    let s = as_string(&s);
    let deficit = (n - s.chars().count() as i64).max(0) as usize;
    let mut out: String = std::iter::repeat(pad).take(deficit).collect();
    out.push_str(s);
    Value::Str(out)
}

fn string_pad_right(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let pad = as_char(&args.pop().unwrap());
    let n = as_int(&args.pop().unwrap());
    let s = as_string(&s);
    let deficit = (n - s.chars().count() as i64).max(0) as usize;
    let mut out = s.to_string();
    out.extend(std::iter::repeat(pad).take(deficit));
    Value::Str(out)
}

fn string_map(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mapped: String = as_string(&s)
        .chars()
        .map(|c| as_char(&apply_top(f.clone(), Value::Char(c))))
        .collect();
    Value::Str(mapped)
}

fn string_filter(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let f = args.pop().unwrap();
    let kept: String = as_string(&s)
        .chars()
        .filter(|c| matches!(as_data(&apply_top(f.clone(), Value::Char(*c))), ("True", _)))
        .collect();
    Value::Str(kept)
}

fn string_foldl(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let init = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mut acc = init;
    for c in as_string(&s).chars() {
        acc = apply_top(apply_top(f.clone(), Value::Char(c)), acc);
    }
    acc
}

fn string_foldr(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let init = args.pop().unwrap();
    let f = args.pop().unwrap();
    let mut acc = init;
    for c in as_string(&s).chars().rev() {
        acc = apply_top(apply_top(f.clone(), Value::Char(c)), acc);
    }
    acc
}

fn string_any(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let f = args.pop().unwrap();
    boolean(
        as_string(&s)
            .chars()
            .any(|c| matches!(as_data(&apply_top(f.clone(), Value::Char(c))), ("True", _))),
    )
}

fn string_all(mut args: Vec<Value>) -> Value {
    let s = args.pop().unwrap();
    let f = args.pop().unwrap();
    boolean(
        as_string(&s)
            .chars()
            .all(|c| matches!(as_data(&apply_top(f.clone(), Value::Char(c))), ("True", _))),
    )
}

/// A sum-type value constructor. A nullary one (`run: None`) evaluates directly
/// to a `Value::Data`; one taking arguments (`run: Some`) is a builtin that
/// collects its args and then builds the `Data` (so `List.map Just` works).
struct Ctor {
    name: &'static str,
    arity: usize,
    scheme: Scheme,
    run: Option<BuiltinFn>,
}

/// The complete set of data constructors. `sum_type_constructors` groups these
/// by result type to feed the exhaustiveness checker, so this table is the
/// single source of truth for which constructors a sum type has.
fn ctors() -> Vec<Ctor> {
    let a = || var(A);
    vec![
        Ctor {
            name: "Just",
            arity: 1,
            scheme: scheme(&[A], fun(a(), maybe(a()))),
            run: Some(make_just),
        },
        Ctor {
            name: "Nothing",
            arity: 0,
            scheme: scheme(&[A], maybe(a())),
            run: None,
        },
        Ctor {
            name: "True",
            arity: 0,
            scheme: scheme(&[], bool_ty()),
            run: None,
        },
        Ctor {
            name: "False",
            arity: 0,
            scheme: scheme(&[], bool_ty()),
            run: None,
        },
        Ctor {
            name: "LT",
            arity: 0,
            scheme: scheme(&[], order()),
            run: None,
        },
        Ctor {
            name: "EQ",
            arity: 0,
            scheme: scheme(&[], order()),
            run: None,
        },
        Ctor {
            name: "GT",
            arity: 0,
            scheme: scheme(&[], order()),
            run: None,
        },
    ]
}

/// The match-only glyph and `Entry` constructors (ADR 0017). Unlike `ctors()`,
/// these are *not* bound into `ty_env`/`env` — construction stays the reserved
/// lowercase words (`aptPackage`/`file`/…), so nothing here gives a way to
/// *build* a glyph. They are registered only in the pattern-resolution
/// registries `constructor_scheme` and `sum_type_constructors`, so their
/// PascalCase tags (`AptPackage`, `File`, …) are reachable solely from a
/// `case` pattern. The split spelling — lowercase to build, PascalCase to match
/// — keeps the two directions from colliding on one name.
///
/// Each scheme is `field-record -> Glyph` (or `-> Entry`), the projection a
/// match sees, mirroring what `eval::glyph_reified` reconstructs from a built
/// glyph: `Filesystem` carries an `entry : Entry`, and `File`/`Directory` carry
/// `perms`. `perms` is a plain closed record, not its own matchable sum, because
/// it has one shape (`{ mode, owner, group }`) with no variants to discriminate
/// — a pattern binds and reads its fields directly.
fn glyph_ctors() -> Vec<Ctor> {
    vec![
        Ctor {
            name: "AptPackage",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("name", string())]), glyph())),
            run: None,
        },
        Ctor {
            name: "SystemdService",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("unit", string())]), glyph())),
            run: None,
        },
        Ctor {
            name: "Filesystem",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("path", string()), ("entry", entry())]), glyph())),
            run: None,
        },
        Ctor {
            name: "LineInFile",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("path", string()), ("line", string())]), glyph())),
            run: None,
        },
        Ctor {
            name: "File",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("contents", string()), ("perms", perms())]), entry())),
            run: None,
        },
        Ctor {
            name: "Directory",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("perms", perms())]), entry())),
            run: None,
        },
        Ctor {
            name: "Symlink",
            arity: 1,
            scheme: scheme(&[], fun(record(&[("target", string())]), entry())),
            run: None,
        },
    ]
}

/// A primitive function: its Elm-accurate type scheme and the Rust
/// implementation invoked once `arity` arguments have arrived.
struct Builtin {
    name: &'static str,
    arity: usize,
    scheme: Scheme,
    run: BuiltinFn,
}

/// Every built-in function: the list combinators (the language's only way to
/// iterate), `Maybe.*`, `String.*`, the numeric/comparison functions, and the
/// operator desugar targets (`add`, `sub`, `lt`, `eq`, `and`, …).
fn builtins() -> Vec<Builtin> {
    let a = || var(A);
    let b = || var(B);
    vec![
        Builtin {
            name: "List.map",
            arity: 2,
            scheme: scheme(&[A, B], fun(fun(a(), b()), fun(list(a()), list(b())))),
            run: list_map,
        },
        Builtin {
            name: "List.foldr",
            arity: 3,
            scheme: scheme(&[A, B], fun(fun(a(), fun(b(), b())), fun(b(), fun(list(a()), b())))),
            run: list_foldr,
        },
        Builtin {
            name: "List.foldl",
            arity: 3,
            scheme: scheme(&[A, B], fun(fun(a(), fun(b(), b())), fun(b(), fun(list(a()), b())))),
            run: list_foldl,
        },
        Builtin {
            name: "List.concat",
            arity: 1,
            scheme: scheme(&[A], fun(list(list(a())), list(a()))),
            run: list_concat,
        },
        Builtin {
            name: "List.concatMap",
            arity: 2,
            scheme: scheme(&[A, B], fun(fun(a(), list(b())), fun(list(a()), list(b())))),
            run: list_concat_map,
        },
        Builtin {
            name: "List.append",
            arity: 2,
            scheme: scheme(&[A], fun(list(a()), fun(list(a()), list(a())))),
            run: list_append,
        },
        // The desugaring target of the `::` operator: prepend an element onto a
        // list. Has no surface spelling of its own (`::` is the only way to
        // reach it), unlike the `List.`-qualified builtins around it.
        Builtin {
            name: "cons",
            arity: 2,
            scheme: scheme(&[A], fun(a(), fun(list(a()), list(a())))),
            run: list_cons,
        },
        Builtin {
            name: "List.filter",
            arity: 2,
            scheme: scheme(&[A], fun(fun(a(), bool_ty()), fun(list(a()), list(a())))),
            run: list_filter,
        },
        Builtin {
            name: "List.isEmpty",
            arity: 1,
            scheme: scheme(&[A], fun(list(a()), bool_ty())),
            run: list_is_empty,
        },
        Builtin {
            name: "Maybe.map",
            arity: 2,
            scheme: scheme(&[A, B], fun(fun(a(), b()), fun(maybe(a()), maybe(b())))),
            run: maybe_map,
        },
        Builtin {
            name: "Maybe.withDefault",
            arity: 2,
            scheme: scheme(&[A], fun(a(), fun(maybe(a()), a()))),
            run: maybe_with_default,
        },
        Builtin {
            name: "Maybe.andThen",
            arity: 2,
            scheme: scheme(&[A, B], fun(fun(a(), maybe(b())), fun(maybe(a()), maybe(b())))),
            run: maybe_and_then,
        },
        Builtin {
            name: "List.length",
            arity: 1,
            scheme: scheme(&[A], fun(list(a()), int())),
            run: list_length,
        },
        Builtin {
            name: "List.range",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(int(), list(int())))),
            run: list_range,
        },
        Builtin {
            name: "List.sum",
            arity: 1,
            scheme: scheme(&[N], fun(list(number(N)), number(N))),
            run: list_sum,
        },
        Builtin {
            name: "Tuple.pair",
            arity: 2,
            scheme: scheme(&[A, B], fun(a(), fun(b(), pair(a(), b())))),
            run: tuple_pair,
        },
        Builtin {
            name: "Tuple.first",
            arity: 1,
            scheme: scheme(&[A, B], fun(pair(a(), b()), a())),
            run: tuple_first,
        },
        Builtin {
            name: "Tuple.second",
            arity: 1,
            scheme: scheme(&[A, B], fun(pair(a(), b()), b())),
            run: tuple_second,
        },
        Builtin {
            name: "Tuple.mapFirst",
            arity: 2,
            scheme: scheme(&[A, B, X], fun(fun(a(), var(X)), fun(pair(a(), b()), pair(var(X), b())))),
            run: tuple_map_first,
        },
        Builtin {
            name: "Tuple.mapSecond",
            arity: 2,
            scheme: scheme(&[A, B, Y], fun(fun(b(), var(Y)), fun(pair(a(), b()), pair(a(), var(Y))))),
            run: tuple_map_second,
        },
        Builtin {
            name: "Tuple.mapBoth",
            arity: 3,
            scheme: scheme(
                &[A, B, X, Y],
                fun(
                    fun(a(), var(X)),
                    fun(fun(b(), var(Y)), fun(pair(a(), b()), pair(var(X), var(Y)))),
                ),
            ),
            run: tuple_map_both,
        },
        Builtin {
            name: "String.uncons",
            arity: 1,
            scheme: scheme(&[], fun(string(), maybe(pair(char(), string())))),
            run: string_uncons,
        },
        Builtin {
            name: "String.append",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), string()))),
            run: string_append,
        },
        Builtin {
            name: "append",
            arity: 2,
            scheme: scheme(&[P], fun(appendable(P), fun(appendable(P), appendable(P)))),
            run: append,
        },
        Builtin {
            name: "String.concat",
            arity: 1,
            scheme: scheme(&[], fun(list(string()), string())),
            run: string_concat,
        },
        Builtin {
            name: "String.join",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(list(string()), string()))),
            run: string_join,
        },
        Builtin {
            name: "String.length",
            arity: 1,
            scheme: scheme(&[], fun(string(), int())),
            run: string_length,
        },
        Builtin {
            name: "String.fromInt",
            arity: 1,
            scheme: scheme(&[], fun(int(), string())),
            run: string_from_int,
        },
        Builtin {
            name: "String.fromFloat",
            arity: 1,
            scheme: scheme(&[], fun(float(), string())),
            run: string_from_float,
        },
        Builtin {
            name: "String.toInt",
            arity: 1,
            scheme: scheme(&[], fun(string(), maybe(int()))),
            run: string_to_int,
        },
        Builtin {
            name: "String.toFloat",
            arity: 1,
            scheme: scheme(&[], fun(string(), maybe(float()))),
            run: string_to_float,
        },
        Builtin {
            name: "Char.toCode",
            arity: 1,
            scheme: scheme(&[], fun(char(), int())),
            run: char_to_code,
        },
        Builtin {
            name: "Char.fromCode",
            arity: 1,
            scheme: scheme(&[], fun(int(), char())),
            run: char_from_code,
        },
        Builtin {
            name: "Char.toUpper",
            arity: 1,
            scheme: scheme(&[], fun(char(), char())),
            run: char_to_upper,
        },
        Builtin {
            name: "Char.toLower",
            arity: 1,
            scheme: scheme(&[], fun(char(), char())),
            run: char_to_lower,
        },
        Builtin {
            name: "Char.isUpper",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_upper,
        },
        Builtin {
            name: "Char.isLower",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_lower,
        },
        Builtin {
            name: "Char.isAlpha",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_alpha,
        },
        Builtin {
            name: "Char.isAlphaNum",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_alpha_num,
        },
        Builtin {
            name: "Char.isDigit",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_digit,
        },
        Builtin {
            name: "Char.isOctDigit",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_oct_digit,
        },
        Builtin {
            name: "Char.isHexDigit",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_hex_digit,
        },
        Builtin {
            name: "Char.isSpace",
            arity: 1,
            scheme: scheme(&[], fun(char(), bool_ty())),
            run: char_is_space,
        },
        Builtin {
            name: "String.isEmpty",
            arity: 1,
            scheme: scheme(&[], fun(string(), bool_ty())),
            run: string_is_empty,
        },
        Builtin {
            name: "String.reverse",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_reverse,
        },
        Builtin {
            name: "String.repeat",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(string(), string()))),
            run: string_repeat,
        },
        Builtin {
            name: "String.replace",
            arity: 3,
            scheme: scheme(&[], fun(string(), fun(string(), fun(string(), string())))),
            run: string_replace,
        },
        Builtin {
            name: "String.split",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), list(string())))),
            run: string_split,
        },
        Builtin {
            name: "String.words",
            arity: 1,
            scheme: scheme(&[], fun(string(), list(string()))),
            run: string_words,
        },
        Builtin {
            name: "String.lines",
            arity: 1,
            scheme: scheme(&[], fun(string(), list(string()))),
            run: string_lines,
        },
        Builtin {
            name: "String.slice",
            arity: 3,
            scheme: scheme(&[], fun(int(), fun(int(), fun(string(), string())))),
            run: string_slice,
        },
        Builtin {
            name: "String.left",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(string(), string()))),
            run: string_left,
        },
        Builtin {
            name: "String.right",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(string(), string()))),
            run: string_right,
        },
        Builtin {
            name: "String.dropLeft",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(string(), string()))),
            run: string_drop_left,
        },
        Builtin {
            name: "String.dropRight",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(string(), string()))),
            run: string_drop_right,
        },
        Builtin {
            name: "String.contains",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), bool_ty()))),
            run: string_contains,
        },
        Builtin {
            name: "String.startsWith",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), bool_ty()))),
            run: string_starts_with,
        },
        Builtin {
            name: "String.endsWith",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), bool_ty()))),
            run: string_ends_with,
        },
        Builtin {
            name: "String.indexes",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), list(int())))),
            run: string_indexes,
        },
        Builtin {
            name: "String.indices",
            arity: 2,
            scheme: scheme(&[], fun(string(), fun(string(), list(int())))),
            run: string_indexes,
        },
        Builtin {
            name: "String.toList",
            arity: 1,
            scheme: scheme(&[], fun(string(), list(char()))),
            run: string_to_list,
        },
        Builtin {
            name: "String.fromList",
            arity: 1,
            scheme: scheme(&[], fun(list(char()), string())),
            run: string_from_list,
        },
        Builtin {
            name: "String.fromChar",
            arity: 1,
            scheme: scheme(&[], fun(char(), string())),
            run: string_from_char,
        },
        Builtin {
            name: "String.cons",
            arity: 2,
            scheme: scheme(&[], fun(char(), fun(string(), string()))),
            run: string_cons,
        },
        Builtin {
            name: "String.toUpper",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_to_upper,
        },
        Builtin {
            name: "String.toLower",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_to_lower,
        },
        Builtin {
            name: "String.trim",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_trim,
        },
        Builtin {
            name: "String.trimLeft",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_trim_left,
        },
        Builtin {
            name: "String.trimRight",
            arity: 1,
            scheme: scheme(&[], fun(string(), string())),
            run: string_trim_right,
        },
        Builtin {
            name: "String.pad",
            arity: 3,
            scheme: scheme(&[], fun(int(), fun(char(), fun(string(), string())))),
            run: string_pad,
        },
        Builtin {
            name: "String.padLeft",
            arity: 3,
            scheme: scheme(&[], fun(int(), fun(char(), fun(string(), string())))),
            run: string_pad_left,
        },
        Builtin {
            name: "String.padRight",
            arity: 3,
            scheme: scheme(&[], fun(int(), fun(char(), fun(string(), string())))),
            run: string_pad_right,
        },
        Builtin {
            name: "String.map",
            arity: 2,
            scheme: scheme(&[], fun(fun(char(), char()), fun(string(), string()))),
            run: string_map,
        },
        Builtin {
            name: "String.filter",
            arity: 2,
            scheme: scheme(&[], fun(fun(char(), bool_ty()), fun(string(), string()))),
            run: string_filter,
        },
        Builtin {
            name: "String.foldl",
            arity: 3,
            scheme: scheme(&[B], fun(fun(char(), fun(b(), b())), fun(b(), fun(string(), b())))),
            run: string_foldl,
        },
        Builtin {
            name: "String.foldr",
            arity: 3,
            scheme: scheme(&[B], fun(fun(char(), fun(b(), b())), fun(b(), fun(string(), b())))),
            run: string_foldr,
        },
        Builtin {
            name: "String.any",
            arity: 2,
            scheme: scheme(&[], fun(fun(char(), bool_ty()), fun(string(), bool_ty()))),
            run: string_any,
        },
        Builtin {
            name: "String.all",
            arity: 2,
            scheme: scheme(&[], fun(fun(char(), bool_ty()), fun(string(), bool_ty()))),
            run: string_all,
        },
        Builtin {
            name: "toFloat",
            arity: 1,
            scheme: scheme(&[], fun(int(), float())),
            run: builtin_to_float,
        },
        Builtin {
            name: "round",
            arity: 1,
            scheme: scheme(&[], fun(float(), int())),
            run: builtin_round,
        },
        Builtin {
            name: "floor",
            arity: 1,
            scheme: scheme(&[], fun(float(), int())),
            run: builtin_floor,
        },
        Builtin {
            name: "ceiling",
            arity: 1,
            scheme: scheme(&[], fun(float(), int())),
            run: builtin_ceiling,
        },
        Builtin {
            name: "truncate",
            arity: 1,
            scheme: scheme(&[], fun(float(), int())),
            run: builtin_truncate,
        },
        Builtin {
            name: "negate",
            arity: 1,
            scheme: scheme(&[N], fun(number(N), number(N))),
            run: builtin_negate,
        },
        Builtin {
            name: "abs",
            arity: 1,
            scheme: scheme(&[N], fun(number(N), number(N))),
            run: builtin_abs,
        },
        Builtin {
            name: "clamp",
            arity: 3,
            scheme: scheme(&[N], fun(number(N), fun(number(N), fun(number(N), number(N))))),
            run: builtin_clamp,
        },
        Builtin {
            name: "modBy",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(int(), int()))),
            run: builtin_mod_by,
        },
        Builtin {
            name: "remainderBy",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(int(), int()))),
            run: builtin_remainder_by,
        },
        Builtin {
            name: "min",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), comparable(C)))),
            run: builtin_min,
        },
        Builtin {
            name: "max",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), comparable(C)))),
            run: builtin_max,
        },
        Builtin {
            name: "compare",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), order()))),
            run: builtin_compare,
        },
        Builtin {
            name: "not",
            arity: 1,
            scheme: scheme(&[], fun(bool_ty(), bool_ty())),
            run: builtin_not,
        },
        Builtin {
            name: "add",
            arity: 2,
            scheme: scheme(&[N], fun(number(N), fun(number(N), number(N)))),
            run: builtin_add,
        },
        Builtin {
            name: "sub",
            arity: 2,
            scheme: scheme(&[N], fun(number(N), fun(number(N), number(N)))),
            run: builtin_sub,
        },
        Builtin {
            name: "mul",
            arity: 2,
            scheme: scheme(&[N], fun(number(N), fun(number(N), number(N)))),
            run: builtin_mul,
        },
        Builtin {
            name: "pow",
            arity: 2,
            scheme: scheme(&[N], fun(number(N), fun(number(N), number(N)))),
            run: builtin_pow,
        },
        Builtin {
            name: "fdiv",
            arity: 2,
            scheme: scheme(&[], fun(float(), fun(float(), float()))),
            run: builtin_fdiv,
        },
        Builtin {
            name: "idiv",
            arity: 2,
            scheme: scheme(&[], fun(int(), fun(int(), int()))),
            run: builtin_idiv,
        },
        Builtin {
            name: "lt",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_lt,
        },
        Builtin {
            name: "gt",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_gt,
        },
        Builtin {
            name: "le",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_le,
        },
        Builtin {
            name: "ge",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_ge,
        },
        Builtin {
            name: "eq",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_eq,
        },
        Builtin {
            name: "neq",
            arity: 2,
            scheme: scheme(&[C], fun(comparable(C), fun(comparable(C), bool_ty()))),
            run: builtin_neq,
        },
        Builtin {
            name: "and",
            arity: 2,
            scheme: scheme(&[], fun(bool_ty(), fun(bool_ty(), bool_ty()))),
            run: builtin_and,
        },
        Builtin {
            name: "or",
            arity: 2,
            scheme: scheme(&[], fun(bool_ty(), fun(bool_ty(), bool_ty()))),
            run: builtin_or,
        },
    ]
}

/// The synthetic name of the nil list constructor, matched by an `[]` pattern.
pub const NIL: &str = "[]";
/// The synthetic name of the cons list constructor, matched by a `head :: tail`
/// pattern. Its two argument types are the element type and the list type.
pub const CONS: &str = "::";

/// The type scheme of a data constructor, for pattern inference. Alongside the
/// user/prelude sum constructors, the two synthetic list constructors `[]` and
/// `::` have schemes `∀a. List a` and `∀a. a -> List a -> List a`, so list
/// patterns type-check and drive the exhaustiveness checker like any sum type.
pub fn constructor_scheme(name: &str) -> Option<Scheme> {
    match name {
        NIL => return Some(scheme(&[A], list(var(A)))),
        CONS => return Some(scheme(&[A], fun(var(A), fun(list(var(A)), list(var(A)))))),
        _ => {}
    }
    ctors()
        .into_iter()
        .chain(glyph_ctors())
        .find(|c| c.name == name)
        .map(|c| c.scheme)
}

/// The constructors (name + arity) of a sum type, by result-type name — the
/// "complete signature" the exhaustiveness checker needs. `List` is treated as
/// a two-constructor sum (`[]`, `::`) so a `case` on a list is exhaustive
/// exactly when it covers both. `None` if no constructor produces this type
/// (e.g. `String`).
pub fn sum_type_constructors(type_name: &str) -> Option<Vec<(String, usize)>> {
    if type_name == "List" {
        return Some(vec![(NIL.to_string(), 0), (CONS.to_string(), 2)]);
    }
    let members: Vec<(String, usize)> = ctors()
        .into_iter()
        .chain(glyph_ctors())
        .filter(|c| result_type_name(&c.scheme.ty) == Some(type_name.to_string()))
        .map(|c| (c.name.to_string(), c.arity))
        .collect();
    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

fn result_type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Fun(_, to) => result_type_name(to),
        Type::Con(name, _) => Some(name.clone()),
        _ => None,
    }
}

/// The initial type environment for inference: every constructor and builtin
/// bound to its scheme.
pub fn ty_env() -> TyEnv {
    let mut env = TyEnv::default();
    for c in ctors() {
        env = env.bind(c.name.to_string(), c.scheme);
    }
    for b in builtins() {
        env = env.bind(b.name.to_string(), b.scheme);
    }
    env
}

/// The initial value environment for evaluation, mirroring `ty_env`: nullary
/// constructors as `Data`, everything else as a zero-arg `Builtin` that fills
/// up through `apply`.
pub fn env() -> Env {
    let mut env = Env::default();
    for c in ctors() {
        let value = match c.run {
            Some(run) => Value::Builtin {
                name: c.name.to_string(),
                arity: c.arity,
                args: Vec::new(),
                run,
            },
            None => Value::Data { ctor: c.name.to_string(), args: Vec::new() },
        };
        env = env.insert(c.name.to_string(), value);
    }
    for b in builtins() {
        env = env.insert(
            b.name.to_string(),
            Value::Builtin {
                name: b.name.to_string(),
                arity: b.arity,
                args: Vec::new(),
                run: b.run,
            },
        );
    }
    env
}
