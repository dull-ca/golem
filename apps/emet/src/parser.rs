//! A chumsky 0.10 parser over the laid-out token stream from `layout.rs`
//! (offside `{`/`;`/`}` already inserted; `parse-error(t)` already resolved —
//! see `layout.rs`'s module doc).
//!
//! Grammar, roughly:
//!
//! ```text
//! module  = { type_decl | decl }
//! type_decl = 'type' Upper param* '=' variant ('|' variant)*
//! variant = Upper type_atom*
//! decl    = name ':' type                     -- signature
//!         | name param* '=' expr              -- binding
//! expr    = '\' param+ '->' expr              -- lambda
//!         | 'let' decls 'in' expr
//!         | 'if' expr 'then' expr 'else' expr
//!         | 'case' expr 'of' { arm }
//!         | operators                         -- precedence-climbing layer
//! arm     = pattern '->' expr
//! operators = unary (op unary)*               -- see `fold_operators`
//! unary   = '-'* app
//! app     = postfix postfix*                  -- juxtaposition
//! postfix = atom ('.' field)*                 -- record field access
//! atom    = literal | var | qualified | Ctor | ctor-record | list | record
//!         | interpolation | '(' expr ')'
//! ```
//!
//! Two things desugar in the parser, so nothing downstream needs special
//! handling (ADR 0004, ADR 0007):
//!   * Operators desugar to prelude-builtin applications — `a + b` → `add a b`,
//!     `a ++ b` → `String.append a b` — via a precedence-climbing pass over the
//!     flat operator/operand list (`fold_operators` / `climb`), with
//!     Elm-accurate fixity (`operator_fixity`).
//!   * Interpolation `"a${e}b"` desugars to `String.concat ["a", e, "b"]`.
//!
//! `Upper '.' ident` written adjacently (`List.map`) parses as one qualified
//! `Expr::Var("List.map")` — resolved by ordinary env lookup, no new node
//! (ADR 0006). A bare `Upper` is a data constructor (`Expr::Ctor`).
//!
//! The reserved lowercase words `aptPackage`, `systemdService`, `file`,
//! `lineInFile`, and `scroll` are each parsed as a constructor atom rather than
//! as ordinary records — each requires exactly its own fields (see
//! `build_constructor`) and builds the matching `Expr` variant.
//!
//! Signatures attach to the binding that follows them, but that pairing
//! happens after parsing. `module_parser` collects `Vec<TopItem>` — each item
//! either a `type` declaration or a value `DeclItem` (a sig or a binding);
//! `decls_parser` (for `let`) collects a flat `Vec<DeclItem>`. In both cases
//! `fold_decls` walks the value items — sigs and bindings interleaved as
//! written — to produce `Vec<Decl>`, erroring on an orphaned or mismatched
//! signature.
//!
//! chumsky reports errors as `Rich<Tok, TokSpan>`; `parse` converts each into
//! a `ParseError` carrying a message and a source byte span, which
//! `main.rs` feeds to `ariadne` for rendering.

use std::collections::BTreeMap;

use chumsky::input::ValueInput;
use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::{Tok, Token};

/// A parse failure: a message plus the source byte span to underline.
pub struct ParseError {
    pub msg: String,
    pub span: Span,
}

type TokSpan = SimpleSpan<usize>;

enum SigOrBind {
    Sig(Spanned<Type>),
    Bind {
        params: Vec<String>,
        body: Spanned<Expr>,
    },
}

enum DeclItem {
    Sig {
        name: String,
        ty: Spanned<Type>,
        span: Span,
    },
    Bind {
        name: String,
        params: Vec<String>,
        body: Spanned<Expr>,
        span: Span,
    },
}

fn ident<'src, I>() -> impl Parser<'src, I, String, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    select! { Tok::Ident(name) => name }
}

fn field_name<'src, I>() -> impl Parser<'src, I, String, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    select! { Tok::Ident(name) => name }
}

fn type_parser<'src, I>() -> impl Parser<'src, I, Spanned<Type>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    recursive(|ty| {
        let con_head = select! { Tok::Upper(u) => u };

        let nullary = con_head
            .map_with(|u, e| Spanned(canonical_con(&u, vec![]), span_range(e.span())));

        let type_var = select! { Tok::Ident(name) => name }
            .map_with(|name, e| Spanned(Type::Rigid(name), span_range(e.span())));

        let list = ty
            .clone()
            .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
            .map_with(|inner: Spanned<Type>, e| {
                Spanned(Type::Con("List".to_string(), vec![inner.0]), span_range(e.span()))
            });

        let record_field = field_name()
            .then_ignore(just(Tok::Colon))
            .then(ty.clone());

        let record = record_field
            .separated_by(just(Tok::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Tok::LBrace), just(Tok::RBrace))
            .map_with(|pairs: Vec<(String, Spanned<Type>)>, e| {
                let mut fields = BTreeMap::new();
                for (k, v) in pairs {
                    fields.insert(k, v.0);
                }
                Spanned(Type::Record(fields, Row::Closed), span_range(e.span()))
            });

        let paren = ty
            .clone()
            .delimited_by(just(Tok::LParen), just(Tok::RParen));

        let atom = choice((nullary, type_var, list, record, paren)).labelled("a type");

        let application = con_head
            .then(atom.clone().repeated().at_least(1).collect::<Vec<_>>())
            .map_with(|(head, args), e| {
                Spanned(
                    canonical_con(&head, args.into_iter().map(|a| a.0).collect()),
                    span_range(e.span()),
                )
            });

        let btype = choice((application, atom)).labelled("a type");

        btype
            .clone()
            .then(just(Tok::Arrow).ignore_then(ty).or_not())
            .map(|(a, rest)| match rest {
                Some(b) => {
                    let span = a.1.start..b.1.end;
                    Spanned(Type::Fun(Box::new(a.0), Box::new(b.0)), span)
                }
                None => a,
            })
    })
}

/// A single type atom, as a variant field appears: a nullary constructor, a
/// type variable, a `[T]` list, a record type, or a parenthesized type (which
/// may itself be an applied constructor like `(Tree a)`). Unlike `type_parser`,
/// a bare applied head is NOT an atom — `Node Tree a` is three separate fields,
/// so an application must be parenthesized.
fn type_atom_parser<'src, I>() -> impl Parser<'src, I, Spanned<Type>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    let nullary = select! { Tok::Upper(u) => u }
        .map_with(|u, e| Spanned(canonical_con(&u, vec![]), span_range(e.span())));

    let type_var = select! { Tok::Ident(name) => name }
        .map_with(|name, e| Spanned(Type::Rigid(name), span_range(e.span())));

    let list = type_parser()
        .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
        .map_with(|inner: Spanned<Type>, e| {
            Spanned(Type::Con("List".to_string(), vec![inner.0]), span_range(e.span()))
        });

    let record_field = field_name()
        .then_ignore(just(Tok::Colon))
        .then(type_parser());

    let record = record_field
        .separated_by(just(Tok::Comma))
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just(Tok::LBrace), just(Tok::RBrace))
        .map_with(|pairs: Vec<(String, Spanned<Type>)>, e| {
            let mut fields = BTreeMap::new();
            for (k, v) in pairs {
                fields.insert(k, v.0);
            }
            Spanned(Type::Record(fields, Row::Closed), span_range(e.span()))
        });

    let paren = type_parser().delimited_by(just(Tok::LParen), just(Tok::RParen));

    choice((nullary, type_var, list, record, paren)).labelled("a type")
}

/// A top-level `type Name p1 p2 = V1 f1 f2 | V2 | V3 f` declaration. Layout
/// keeps the whole thing (including any indented multi-line `= … | …`) as one
/// block item, so no special layout support is needed. Each variant is an
/// `Upper` head plus zero or more type atoms.
///
/// This accepts any structurally-valid `Upper args*` — it does not check that a
/// referenced type constructor exists or is applied at the right arity, nor
/// that a variant name is unique. Those checks need the whole module's set of
/// declared types (so decls may reference each other in any order) and so live
/// in inference: see `infer::register_type_decls` and `infer::validate_type_refs`.
fn type_decl_parser<'src, I>(
) -> impl Parser<'src, I, TypeDecl, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    let variant = select! { Tok::Upper(u) => u }
        .map_with(|u, e| (u, span_range(e.span())))
        .then(type_atom_parser().repeated().collect::<Vec<_>>())
        .map(|((name, name_span), fields)| {
            let end = fields.last().map(|f| f.1.end).unwrap_or(name_span.end);
            Variant { name, fields, span: name_span.start..end }
        });

    just(Tok::Type)
        .map_with(|_, e| span_range(e.span()).start)
        .then(select! { Tok::Upper(u) => u })
        .then(ident().repeated().collect::<Vec<String>>())
        .then_ignore(just(Tok::Equals))
        .then(
            variant
                .separated_by(just(Tok::Op("|".to_string())))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .map(|(((start, name), params), variants)| {
            let end = variants.last().map(|v| v.span.end).unwrap_or(start);
            TypeDecl { name, params, variants, span: start..end }
        })
}

fn expr_parser<'src, I>() -> impl Parser<'src, I, Spanned<Expr>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    recursive(|expr| {
        let str_lit = select! { Tok::Str(s) => s }
            .map_with(|s, e| Spanned(Expr::Str(s), span_range(e.span())));

        let str_part = select! { Tok::StrPart(s) => s };

        let interp_segment = just(Tok::InterpStart)
            .ignore_then(expr.clone())
            .then_ignore(just(Tok::InterpEnd))
            .then(str_part.map_with(|s, e| Spanned(Expr::Str(s), span_range(e.span()))));

        let interpolated = str_part
            .map_with(|s, e| Spanned(Expr::Str(s), span_range(e.span())))
            .then(interp_segment.repeated().at_least(1).collect::<Vec<_>>())
            .map_with(|(head, rest), e| {
                let whole = span_range(e.span());
                let mut pieces = Vec::with_capacity(1 + rest.len() * 2);
                pieces.push(head);
                for (embedded, literal) in rest {
                    pieces.push(embedded);
                    pieces.push(literal);
                }
                let list = Spanned(Expr::List(pieces), whole.clone());
                let concat = Spanned(
                    Expr::Var("String.concat".to_string()),
                    whole.start..whole.start,
                );
                Spanned(Expr::App(Box::new(concat), Box::new(list)), whole)
            });

        let int_lit = select! { Tok::Int(n) => n }
            .map_with(|n, e| Spanned(Expr::Int(n), span_range(e.span())));

        let float_lit = select! { Tok::Float(x) => x }
            .map_with(|x, e| Spanned(Expr::Float(x), span_range(e.span())));

        let var = select! {
            Tok::Ident(name) if !is_reserved_constructor(&name) => name
        }
        .map_with(|name, e| Spanned(Expr::Var(name), span_range(e.span())));

        let qualified = select! { Tok::Upper(u) => u }
            .map_with(|u, e| (u, span_range(e.span())))
            .then(just(Tok::Dot).map_with(|_, e| span_range(e.span())))
            .then(field_name().map_with(|n, e| (n, span_range(e.span()))))
            .filter(|(((_, module_span), dot_span), (_, member_span))| {
                module_span.end == dot_span.start && dot_span.end == member_span.start
            })
            .map(|(((module, module_span), _dot_span), (member, member_span))| {
                Spanned(
                    Expr::Var(format!("{module}.{member}")),
                    module_span.start..member_span.end,
                )
            });

        let ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| Spanned(Expr::Ctor(u), span_range(e.span())));

        let constructor_field = field_name()
            .then_ignore(just(Tok::Equals))
            .then(expr.clone());

        let constructor_name = select! {
            Tok::Ident(name) if is_reserved_constructor(&name) => name,
        };

        // Every reserved word commits into one constructor branch, then `try_map`
        // dispatches on the name. Parsing them together (rather than as
        // alternatives) is why a wrong/missing field surfaces as a field error for
        // the named constructor instead of a bare parse failure.
        let constructor = constructor_name
            .then_ignore(just(Tok::LBrace))
            .then(
                constructor_field
                    .clone()
                    .separated_by(just(Tok::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .then_ignore(just(Tok::RBrace).labelled("`}`")),
            )
            .try_map(|(ctor, pairs): (String, Vec<(String, Spanned<Expr>)>), span| {
                let mut fields: BTreeMap<String, Spanned<Expr>> = BTreeMap::new();
                for (k, v) in pairs {
                    fields.insert(k, v);
                }
                build_constructor(&ctor, &mut fields, span)
                    .map(|expr| Spanned(expr, span_range(span)))
            });

        let list = expr
            .clone()
            .separated_by(just(Tok::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
            .map_with(|items, e| Spanned(Expr::List(items), span_range(e.span())));

        let record_field = field_name()
            .then_ignore(just(Tok::Equals))
            .then(expr.clone());

        let record = record_field
            .separated_by(just(Tok::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Tok::LBrace), just(Tok::RBrace))
            .map_with(|pairs: Vec<(String, Spanned<Expr>)>, e| {
                let mut fields = BTreeMap::new();
                for (k, v) in pairs {
                    fields.insert(k, v);
                }
                Spanned(Expr::Record(fields), span_range(e.span()))
            });

        let paren = expr
            .clone()
            .delimited_by(just(Tok::LParen), just(Tok::RParen));

        let atom = choice((interpolated, str_lit, float_lit, int_lit, constructor, var, qualified, ctor, paren, list, record));

        let postfix = atom.foldl(
            just(Tok::Dot).ignore_then(field_name()).repeated(),
            |e, field| {
                let span = e.1.start..e.1.end;
                Spanned(Expr::Field(Box::new(e), field), span)
            },
        );

        let app = postfix.clone().foldl(postfix.repeated(), |f, x| {
            let span = f.1.start..x.1.end;
            Spanned(Expr::App(Box::new(f), Box::new(x)), span)
        });

        let unary = just(Tok::Op("-".to_string()))
            .map_with(|_, e| span_range(e.span()).start)
            .repeated()
            .collect::<Vec<usize>>()
            .then(app.clone())
            .map(|(minuses, operand)| {
                minuses.into_iter().rev().fold(operand, |operand, start| {
                    let end = operand.1.end;
                    let negate = Spanned(Expr::Var("negate".to_string()), start..start);
                    Spanned(Expr::App(Box::new(negate), Box::new(operand)), start..end)
                })
            });

        let operator = select! { Tok::Op(s) => s };

        let binary = unary
            .clone()
            .then(
                operator
                    .map_with(|s, e| (s, span_range(e.span())))
                    .then(unary)
                    .repeated()
                    .collect::<Vec<((String, Span), Spanned<Expr>)>>(),
            )
            .try_map(|(head, rest), span| {
                fold_operators(head, rest).map_err(|msg| Rich::custom(span, msg))
            });

        let lambda = just(Tok::Backslash)
            .map_with(|_, e| span_range(e.span()).start)
            .then(ident().repeated().at_least(1).collect::<Vec<_>>())
            .then_ignore(just(Tok::Arrow))
            .then(expr.clone())
            .map(|((start, params), body)| {
                let end = body.1.end;
                let mut e = body;
                for p in params.into_iter().rev() {
                    e = Spanned(
                        Expr::Lam {
                            param: p,
                            body: Box::new(e),
                        },
                        start..end,
                    );
                }
                e
            });

        let block_open = choice((just(Tok::VLBrace), just(Tok::LBrace)));
        let block_close = choice((just(Tok::VRBrace), just(Tok::RBrace)));

        let if_expr = just(Tok::If)
            .map_with(|_, e| span_range(e.span()).start)
            .then(expr.clone())
            .then_ignore(just(Tok::Then))
            .then(expr.clone())
            .then_ignore(just(Tok::Else))
            .then(expr.clone())
            .map(|(((start, cond), then_), else_)| {
                let end = else_.1.end;
                Spanned(
                    Expr::If {
                        cond: Box::new(cond),
                        then_: Box::new(then_),
                        else_: Box::new(else_),
                    },
                    start..end,
                )
            });

        let arm = pattern_parser()
            .then_ignore(just(Tok::Arrow))
            .then(expr.clone())
            .map(|(pat, body)| Arm { pat, body });

        let arms = just(Tok::VSemi)
            .repeated()
            .ignore_then(
                arm.then_ignore(just(Tok::VSemi).repeated())
                    .repeated()
                    .collect::<Vec<_>>(),
            );

        let case_expr = just(Tok::Case)
            .map_with(|_, e| span_range(e.span()).start)
            .then(expr.clone())
            .then_ignore(just(Tok::Of))
            .then(arms.delimited_by(block_open.clone(), block_close.clone()))
            .map(|((start, scrutinee), arms)| {
                let end = scrutinee.1.end;
                Spanned(
                    Expr::Case {
                        scrutinee: Box::new(scrutinee),
                        arms,
                    },
                    start..end,
                )
            });

        let let_expr = just(Tok::Let)
            .map_with(|_, e| span_range(e.span()).start)
            .then(
                decls_parser(expr.clone())
                    .delimited_by(block_open, block_close),
            )
            .then_ignore(just(Tok::In))
            .then(expr.clone())
            .try_map(|((start, decls), body), span| {
                let decls = fold_decls(decls).map_err(|pe| Rich::custom(span, pe.msg))?;
                let end = body.1.end;
                Ok(Spanned(
                    Expr::Let {
                        decls,
                        body: Box::new(body),
                    },
                    start..end,
                ))
            });

        choice((lambda, let_expr, if_expr, case_expr, binary))
    })
}

enum OpAssoc {
    Left,
    Right,
    NonAssoc,
}

/// Precedence and associativity for each infix operator, matching Elm's fixity
/// table (ADR 0007). `None` for an unknown operator. Level-4 comparisons are
/// non-associative, so `a < b < c` is a parse error.
fn operator_fixity(op: &str) -> Option<(u8, OpAssoc)> {
    Some(match op {
        "^" => (7, OpAssoc::Right),
        "*" | "/" | "//" => (7, OpAssoc::Left),
        "+" | "-" => (6, OpAssoc::Left),
        "++" => (5, OpAssoc::Right),
        "==" | "/=" | "<" | ">" | "<=" | ">=" => (4, OpAssoc::NonAssoc),
        "&&" => (3, OpAssoc::Right),
        "||" => (2, OpAssoc::Right),
        _ => return None,
    })
}

/// The prelude builtin an operator desugars to (`+` → `add`, `++` →
/// `String.append`, …). Types and evaluation come entirely from the builtin,
/// so no operator-specific inference or eval code exists (ADR 0007).
fn operator_builtin(op: &str) -> &'static str {
    match op {
        "+" => "add",
        "-" => "sub",
        "*" => "mul",
        "/" => "fdiv",
        "//" => "idiv",
        "^" => "pow",
        "++" => "String.append",
        "<" => "lt",
        ">" => "gt",
        "<=" => "le",
        ">=" => "ge",
        "==" => "eq",
        "/=" => "neq",
        "&&" => "and",
        "||" => "or",
        _ => unreachable!("unknown operator `{op}`"),
    }
}

/// Resolve a flat `operand (op operand)*` sequence into a tree using operator
/// precedence and associativity. The grammar collects operators without
/// precedence; this pays that off with a precedence-climbing pass (`climb`).
fn fold_operators(
    head: Spanned<Expr>,
    rest: Vec<((String, Span), Spanned<Expr>)>,
) -> Result<Spanned<Expr>, String> {
    let mut ops: Vec<(String, Span)> = Vec::with_capacity(rest.len());
    let mut operands: Vec<Spanned<Expr>> = Vec::with_capacity(rest.len() + 1);
    operands.push(head);
    for ((op, op_span), operand) in rest {
        if operator_fixity(&op).is_none() {
            return Err(format!("unknown operator `{op}`"));
        }
        ops.push((op, op_span));
        operands.push(operand);
    }
    let mut pos = 0usize;
    climb(&operands, &ops, &mut pos, 0)
}

/// Precedence-climbing core: build the sub-tree of operators at or above
/// `min_prec`, folding each `a op b` into `App(App(builtin, a), b)`. Two
/// adjacent non-associative operators of equal precedence (e.g. `a < b < c`)
/// are rejected.
fn climb(
    operands: &[Spanned<Expr>],
    ops: &[(String, Span)],
    pos: &mut usize,
    min_prec: u8,
) -> Result<Spanned<Expr>, String> {
    let mut lhs = operands[*pos].clone();
    while *pos < ops.len() {
        let (op, _) = &ops[*pos];
        let (prec, assoc) = operator_fixity(op).expect("checked in fold_operators");
        if prec < min_prec {
            break;
        }
        let next_min = match assoc {
            OpAssoc::Left | OpAssoc::NonAssoc => prec + 1,
            OpAssoc::Right => prec,
        };
        let op = op.clone();
        *pos += 1;
        let rhs = climb(operands, ops, pos, next_min)?;
        if matches!(assoc, OpAssoc::NonAssoc) && *pos < ops.len() {
            let (next_op, _) = &ops[*pos];
            if let Some((next_prec, OpAssoc::NonAssoc)) = operator_fixity(next_op) {
                if next_prec == prec {
                    return Err(format!(
                        "operator `{op}` is non-associative; add parentheses"
                    ));
                }
            }
        }
        let span = lhs.1.start..rhs.1.end;
        let builtin = operator_builtin(&op);
        let f = Spanned(Expr::Var(builtin.to_string()), lhs.1.start..lhs.1.start);
        let applied_lhs = Spanned(
            Expr::App(Box::new(f), Box::new(lhs)),
            span.clone(),
        );
        lhs = Spanned(Expr::App(Box::new(applied_lhs), Box::new(rhs)), span);
    }
    Ok(lhs)
}

fn pattern_parser<'src, I>() -> impl Parser<'src, I, Spanned<Pattern>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    recursive(|pattern| {
        let wildcard = select! { Tok::Ident(name) if name == "_" => () }
            .map_with(|_, e| Spanned(Pattern::Wildcard, span_range(e.span())));

        let var = select! { Tok::Ident(name) if name != "_" => name }
            .map_with(|name, e| Spanned(Pattern::Var(name), span_range(e.span())));

        let str_lit = select! { Tok::Str(s) => s }
            .map_with(|s, e| Spanned(Pattern::Str(s), span_range(e.span())));

        let nullary_ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| Spanned(Pattern::Ctor(u, vec![]), span_range(e.span())));

        let paren = pattern
            .clone()
            .delimited_by(just(Tok::LParen), just(Tok::RParen));

        let atom = choice((wildcard, var, str_lit, nullary_ctor, paren));

        let applied_ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| (u, span_range(e.span()).start))
            .then(atom.clone().repeated().at_least(1).collect::<Vec<_>>())
            .map(|((name, start), args)| {
                let end = args.last().map(|a| a.1.end).unwrap_or(start);
                Spanned(Pattern::Ctor(name, args), start..end)
            });

        choice((applied_ctor, atom))
    })
}

fn decls_parser<'src, I, E>(
    expr: E,
) -> impl Parser<'src, I, Vec<DeclItem>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
    E: Parser<'src, I, Spanned<Expr>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone + 'src,
{
    let sig_tail = just(Tok::Colon)
        .ignore_then(type_parser())
        .map(SigOrBind::Sig);

    let binding_tail = ident()
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(just(Tok::Equals))
        .then(expr)
        .map(|(params, body)| SigOrBind::Bind { params, body });

    let item = ident()
        .map_with(|name, e| (name, span_range(e.span()).start))
        .then(sig_tail.or(binding_tail))
        .map(|((name, start), tail)| match tail {
            SigOrBind::Sig(ty) => {
                let end = ty.1.end;
                DeclItem::Sig {
                    name,
                    ty,
                    span: start..end,
                }
            }
            SigOrBind::Bind { params, body } => {
                let end = body.1.end;
                DeclItem::Bind {
                    name,
                    params,
                    body,
                    span: start..end,
                }
            }
        });

    just(Tok::VSemi)
        .repeated()
        .ignore_then(
            item.then_ignore(just(Tok::VSemi).repeated())
                .repeated()
                .collect::<Vec<_>>(),
        )
}

enum TopItem {
    Type(TypeDecl),
    Value(DeclItem),
}

fn module_parser<'src, I>(
) -> impl Parser<'src, I, Vec<TopItem>, extra::Err<Rich<'src, Tok, TokSpan>>>
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    let expr = expr_parser();

    let sig_tail = just(Tok::Colon)
        .ignore_then(type_parser())
        .map(SigOrBind::Sig);

    let binding_tail = ident()
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(just(Tok::Equals))
        .then(expr)
        .map(|(params, body)| SigOrBind::Bind { params, body });

    let value_item = ident()
        .map_with(|name, e| (name, span_range(e.span()).start))
        .then(sig_tail.or(binding_tail))
        .map(|((name, start), tail)| match tail {
            SigOrBind::Sig(ty) => {
                let end = ty.1.end;
                TopItem::Value(DeclItem::Sig { name, ty, span: start..end })
            }
            SigOrBind::Bind { params, body } => {
                let end = body.1.end;
                TopItem::Value(DeclItem::Bind { name, params, body, span: start..end })
            }
        });

    let item = choice((type_decl_parser().map(TopItem::Type), value_item));

    just(Tok::VSemi)
        .repeated()
        .ignore_then(
            item.then_ignore(just(Tok::VSemi).repeated())
                .repeated()
                .collect::<Vec<_>>(),
        )
        .delimited_by(
            just(Tok::VLBrace),
            choice((just(Tok::VRBrace), just(Tok::RBrace)))
                .repeated()
                .ignored(),
        )
        .then_ignore(end())
}

fn fold_decls(items: Vec<DeclItem>) -> Result<Vec<Decl>, ParseError> {
    let mut pending_sig: Option<(String, Spanned<Type>, Span)> = None;
    let mut out: Vec<Decl> = Vec::new();

    for item in items {
        match item {
            DeclItem::Sig { name, ty, span } => {
                if let Some((prev, _, _)) = pending_sig.take() {
                    return Err(ParseError {
                        msg: format!(
                            "signature for `{name}` follows an unused signature for `{prev}`"
                        ),
                        span,
                    });
                }
                pending_sig = Some((name, ty, span));
            }
            DeclItem::Bind {
                name,
                params,
                body,
                span,
            } => {
                let sig = match pending_sig.take() {
                    Some((signame, ty, _)) if signame == name => Some(ty),
                    Some((signame, _, sig_span)) => {
                        return Err(ParseError {
                            msg: format!(
                                "signature names `{signame}` but next binding is `{name}`"
                            ),
                            span: sig_span,
                        });
                    }
                    None => None,
                };
                out.push(Decl {
                    name,
                    sig,
                    params,
                    body,
                    span,
                });
            }
        }
    }

    if let Some((signame, _, sig_span)) = pending_sig {
        return Err(ParseError {
            msg: format!("signature for `{signame}` has no accompanying binding"),
            span: sig_span,
        });
    }
    Ok(out)
}

/// The lowercase words that are reserved as glyph/scroll constructors and so
/// cannot be used as ordinary variables (`var` excludes them; `constructor`
/// requires them).
fn is_reserved_constructor(name: &str) -> bool {
    matches!(name, "aptPackage" | "systemdService" | "file" | "lineInFile" | "scroll")
}

/// Dispatch a parsed `name { field = … }` to the right `Expr` variant, pulling
/// exactly the fields that constructor requires and erroring on a missing or
/// unknown field. `take_field` removes each expected field, so any left over is
/// unknown.
fn build_constructor(
    ctor: &str,
    fields: &mut BTreeMap<String, Spanned<Expr>>,
    span: TokSpan,
) -> Result<Expr, Rich<'static, Tok, TokSpan>> {
    let expr = match ctor {
        "aptPackage" => Expr::AptPackage(Box::new(take_field(ctor, fields, "name", span)?)),
        "systemdService" => {
            Expr::SystemdService(Box::new(take_field(ctor, fields, "unit", span)?))
        }
        "file" => Expr::File {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            contents: Box::new(take_field(ctor, fields, "contents", span)?),
            mode: Box::new(take_field(ctor, fields, "mode", span)?),
        },
        "lineInFile" => Expr::LineInFile {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            line: Box::new(take_field(ctor, fields, "line", span)?),
        },
        "scroll" => Expr::Scroll {
            name: Box::new(take_field(ctor, fields, "name", span)?),
            glyphs: Box::new(take_field(ctor, fields, "glyphs", span)?),
        },
        _ => unreachable!("unknown constructor `{ctor}`"),
    };
    if let Some(extra) = fields.keys().next() {
        return Err(Rich::custom(span, format!("unknown {ctor} field `{extra}`")));
    }
    Ok(expr)
}

fn take_field(
    ctor: &str,
    fields: &mut BTreeMap<String, Spanned<Expr>>,
    field: &str,
    span: TokSpan,
) -> Result<Spanned<Expr>, Rich<'static, Tok, TokSpan>> {
    fields.remove(field).ok_or_else(|| {
        Rich::custom(span, format!("`{ctor}` requires a `{field}` field"))
    })
}

/// Build the canonical `Type` for a type-constructor head, folding two surface
/// aliases into their real form: `Str` → `String` (the ADR 0003 migration
/// alias) and `Glyphs` → `List Glyph` (ADR 0002). Everything else is a plain
/// `Con`.
fn canonical_con(name: &str, args: Vec<Type>) -> Type {
    match name {
        "Str" if args.is_empty() => Type::Con("String".to_string(), vec![]),
        "Glyphs" if args.is_empty() => {
            Type::Con("List".to_string(), vec![Type::Con("Glyph".to_string(), vec![])])
        }
        _ => Type::Con(name.to_string(), args),
    }
}

fn span_range(span: TokSpan) -> Span {
    span.start..span.end
}

/// Parse a complete laid-out token stream (as produced by
/// `layout::layout_all`) into a `Module`. Returns every error chumsky
/// collected, or one synthetic "unexpected end of input" error if parsing
/// failed without producing any.
pub fn parse(
    tokens: &[Token],
    name: Option<String>,
    exposing: Exposing,
    imports: Vec<Import>,
) -> Result<Module, Vec<ParseError>> {
    let eoi = tokens
        .last()
        .map(|t| t.span.end)
        .unwrap_or(0);
    let spanned: Vec<(Tok, TokSpan)> = tokens
        .iter()
        .filter(|t| t.tok != Tok::Eof)
        .map(|t| (t.tok.clone(), TokSpan::from(t.span.clone())))
        .collect();

    let input = spanned
        .as_slice()
        .map(TokSpan::from(eoi..eoi), |(t, s)| (t, s));

    let (items, errors) = module_parser().parse(input).into_output_errors();

    if !errors.is_empty() || items.is_none() {
        let out: Vec<ParseError> = errors
            .into_iter()
            .map(|e| ParseError {
                msg: e.to_string(),
                span: span_range(*e.span()),
            })
            .collect();
        if out.is_empty() {
            return Err(vec![ParseError {
                msg: "unexpected end of input".to_string(),
                span: eoi..eoi,
            }]);
        }
        return Err(out);
    }

    let mut type_decls = Vec::new();
    let mut value_items = Vec::new();
    for item in items.unwrap() {
        match item {
            TopItem::Type(td) => type_decls.push(td),
            TopItem::Value(di) => value_items.push(di),
        }
    }

    fold_decls(value_items)
        .map(|decls| Module { name, exposing, imports, type_decls, decls })
        .map_err(|pe| vec![pe])
}
