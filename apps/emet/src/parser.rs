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
//!
//! The top-level `item` recovers past a bad declaration (ADR 0022). The layout
//! pass already emits a virtual `;` (`Tok::VSemi`) between top-level decls, so
//! that token is the sync point: on a parse failure `skip_until` consumes tokens
//! up to the next `VSemi`, records the `Rich` error, and yields a
//! `TopItem::Recovered` sentinel that `parse` drops. The enclosing `.repeated()`
//! then restarts at the following decl, so two malformed decls produce two
//! errors and `parse` returns a `Vec<ParseError>`. Recovery is scoped to the
//! top level — the `let`-block `decls_parser` stays first-error, so a skip
//! cannot swallow the `in` or the virtual `}` the `parse-error(t)` handshake
//! (ADR 0001) depends on.

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

// The head of a top-level or `let` binding. Plain `ident()` accepted a reserved
// constructor word (`file`, `keep`, …) as a bind name, so `keep n = n + 1`
// parsed — the name was definable but unusable, since every later mention lexed
// back as the reserved word (audit #16). Rejecting it here names the trap at the
// point it happens. Params still use `ident()`: they shadow nothing at the head.
fn binding_head<'src, I>(
) -> impl Parser<'src, I, String, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    select! { Tok::Ident(name) => name }.try_map(|name, span| {
        if is_reserved_constructor(&name) {
            Err(Rich::custom(
                span,
                format!("`{name}` is a reserved word and can't be used as a name to bind"),
            ))
        } else {
            Ok(name)
        }
    })
}

fn field_name<'src, I>(
) -> impl Parser<'src, I, String, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    select! { Tok::Ident(name) => name }
}

fn type_parser<'src, I>(
) -> impl Parser<'src, I, Spanned<Type>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    recursive(|ty| {
        let con_head = select! { Tok::Upper(u) => u };

        let nullary = con_head.map_with(|u, e| Spanned(type_con(&u, vec![]), span_range(e.span())));

        let type_var = select! { Tok::Ident(name) => name }
            .map_with(|name, e| Spanned(Type::Rigid(name), span_range(e.span())));

        let list = ty
            .clone()
            .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
            .map_with(|inner: Spanned<Type>, e| {
                Spanned(
                    Type::Con("List".to_string(), vec![inner.0]),
                    span_range(e.span()),
                )
            });

        let record_field = field_name().then_ignore(just(Tok::Colon)).then(ty.clone());

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

        // Tuple *types* by the same count dispatch as the expr form above:
        // `()` unit, `(T)` grouping, `(A, B)`/`(A, B, C)` tuple, 4+ rejected.
        // `just(LParen).ignore_then(…)` for the same diagnostic reason (ADR 0027 §2).
        let paren = just(Tok::LParen)
            .ignore_then(
                ty.clone()
                    .separated_by(just(Tok::Comma))
                    .collect::<Vec<_>>()
                    .then_ignore(just(Tok::RParen)),
            )
            .try_map_with(|mut items: Vec<Spanned<Type>>, e| match items.len() {
                0 => Ok(Spanned(Type::Tuple(vec![]), span_range(e.span()))),
                1 => Ok(items.pop().unwrap()),
                2 | 3 => Ok(Spanned(
                    Type::Tuple(items.into_iter().map(|t| t.0).collect()),
                    span_range(e.span()),
                )),
                _ => Err(Rich::custom(e.span(), TUPLE_TOO_LARGE_MESSAGE)),
            });

        let atom = choice((nullary, type_var, list, record, paren)).labelled("a type");

        let application = con_head
            .then(atom.clone().repeated().at_least(1).collect::<Vec<_>>())
            .map_with(|(head, args), e| {
                Spanned(
                    type_con(&head, args.into_iter().map(|a| a.0).collect()),
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
fn type_atom_parser<'src, I>(
) -> impl Parser<'src, I, Spanned<Type>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
where
    I: ValueInput<'src, Token = Tok, Span = TokSpan>,
{
    let nullary = select! { Tok::Upper(u) => u }
        .map_with(|u, e| Spanned(type_con(&u, vec![]), span_range(e.span())));

    let type_var = select! { Tok::Ident(name) => name }
        .map_with(|name, e| Spanned(Type::Rigid(name), span_range(e.span())));

    let list = type_parser()
        .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
        .map_with(|inner: Spanned<Type>, e| {
            Spanned(
                Type::Con("List".to_string(), vec![inner.0]),
                span_range(e.span()),
            )
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

    // The tuple/unit/grouping paren form at the type-atom layer — same count
    // dispatch and same `ignore_then` form as `type_parser`'s `paren` (ADR 0027 §2).
    let paren = just(Tok::LParen)
        .ignore_then(
            type_parser()
                .separated_by(just(Tok::Comma))
                .collect::<Vec<_>>()
                .then_ignore(just(Tok::RParen)),
        )
        .try_map_with(|mut items: Vec<Spanned<Type>>, e| match items.len() {
            0 => Ok(Spanned(Type::Tuple(vec![]), span_range(e.span()))),
            1 => Ok(items.pop().unwrap()),
            2 | 3 => Ok(Spanned(
                Type::Tuple(items.into_iter().map(|t| t.0).collect()),
                span_range(e.span()),
            )),
            _ => Err(Rich::custom(e.span(), TUPLE_TOO_LARGE_MESSAGE)),
        });

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
            Variant {
                name,
                fields,
                span: name_span.start..end,
            }
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
            TypeDecl {
                name,
                params,
                variants,
                span: start..end,
            }
        })
}

fn expr_parser<'src, I>(
) -> impl Parser<'src, I, Spanned<Expr>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
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

        let char_lit = select! { Tok::Char(c) => c }
            .map_with(|c, e| Spanned(Expr::Char(c), span_range(e.span())));

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
            .map(
                |(((module, module_span), _dot_span), (member, member_span))| {
                    Spanned(
                        Expr::Var(format!("{module}.{member}")),
                        module_span.start..member_span.end,
                    )
                },
            );

        let ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| Spanned(Expr::Ctor(u), span_range(e.span())));

        let constructor_field = field_name()
            .then_ignore(just(Tok::Equals))
            .then(expr.clone());

        let constructor_name = select! {
            Tok::Ident(name) if is_reserved_constructor(&name) => name,
        };

        // `rollback` / `keep` build a policy without braces (the build/match
        // split of ADR 0017), so they are atoms in their own right rather than
        // record constructors. A `.rewind()` peek catches a following `{`: a
        // braced use is rejected here, in this `try_map`, with the "written
        // without braces" hint. Without the peek it read the `{ … }` as an
        // application argument and surfaced a bare type error (audit #19); the
        // `build_constructor` policy-word arm no longer sees the atom path.
        let policy_word = select! {
            Tok::Ident(name) if name == "rollback" || name == "keep" => name,
        }
        .then(just(Tok::LBrace).or_not().rewind())
        .try_map(|(name, braced), span| {
            if braced.is_some() {
                return Err(Rich::custom(
                    span,
                    format!("`{name}` is written without braces (e.g. `policy = {name}`)"),
                ));
            }
            let tag = if name == "rollback" {
                crate::ast::OnExhaustTag::Rollback
            } else {
                crate::ast::OnExhaustTag::Keep
            };
            Ok(Spanned(Expr::PolicyExhaust(tag), span_range(span)))
        });

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
            .try_map(
                |(ctor, pairs): (String, Vec<(String, Spanned<Expr>)>), span| {
                    let mut fields: BTreeMap<String, Spanned<Expr>> = BTreeMap::new();
                    for (k, v) in pairs {
                        fields.insert(k, v);
                    }
                    build_constructor(&ctor, &mut fields, span)
                        .map(|expr| Spanned(expr, span_range(span)))
                },
            );

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

        // The one parenthesized form, read by element count (ADR 0027 §2):
        // `()` (0) is unit, `(e)` (1) is grouping — the inner node itself, NOT a
        // 1-tuple, since Elm has none — `(a, b)` / `(a, b, c)` (2–3) is a tuple,
        // and 4+ is rejected at the whole-form span with the record redirect. A
        // trailing comma is disallowed (no `allow_trailing`), matching Elm.
        //
        // Spelled `just(LParen).ignore_then(… then_ignore(RParen))` rather than
        // the obvious `delimited_by(LParen, RParen)`: a naive `delimited_by`
        // disturbed chumsky's furthest-error merge and regressed the
        // reserved-constructor field diagnostics (a wrong `aptPackage` field
        // surfaced as a bare paren error instead). The explicit open/close form
        // preserves those messages.
        let paren = just(Tok::LParen)
            .ignore_then(
                expr.clone()
                    .separated_by(just(Tok::Comma))
                    .collect::<Vec<_>>()
                    .then_ignore(just(Tok::RParen)),
            )
            .try_map_with(|mut items: Vec<Spanned<Expr>>, e| match items.len() {
                0 => Ok(Spanned(Expr::Tuple(vec![]), span_range(e.span()))),
                1 => Ok(items.pop().unwrap()),
                2 | 3 => Ok(Spanned(Expr::Tuple(items), span_range(e.span()))),
                _ => Err(Rich::custom(e.span(), TUPLE_TOO_LARGE_MESSAGE)),
            });

        let atom = choice((
            interpolated,
            str_lit,
            char_lit,
            float_lit,
            int_lit,
            policy_word,
            constructor,
            var,
            qualified,
            ctor,
            paren,
            list,
            record,
        ));

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

        // A `=>` where an arm's `->` belongs is redirected here rather than in
        // `humanize_expected`. `.validate` + `emitter.emit` for the same reason
        // as `float_reject` above: the arm sits inside `repeated()`, which
        // rewinds and swallows a hard failure, so the message must be emitted
        // non-fatally to survive (ADR 0026). A humanizer clause could not reach
        // it — `compile` shows `errors[0]`, and the collected `=>` error sorts
        // behind an earlier one, so the humanizer path never fired (audit #12).
        let arm_arrow = choice((
            just(Tok::Arrow).ignored(),
            just(Tok::Op("=>".to_string())).validate(|_, e, emitter| {
                emitter.emit(Rich::<Tok, TokSpan>::custom(
                    e.span(),
                    "case arms use `->`, not `=>`",
                ));
            }),
        ));

        // A `let … in`-bodied arm parses today, but real support is deferred
        // (ADR 0032 §4; `docs/TODO.md` #26). Reject it with a specific "not yet
        // supported here" error rather than letting it slip through to a
        // misleading downstream `main : List Scroll` type error. Emitted
        // non-fatally, like `arm_arrow` above, so the arm's `repeated()` cannot
        // backtrack it away.
        let arm_body = expr.clone().validate(|body, _, emitter| {
            let body_is_unsupported_let = matches!(body.0, Expr::Let { .. });
            if body_is_unsupported_let {
                emitter.emit(Rich::<Tok, TokSpan>::custom(
                    body.1.clone().into(),
                    "`let … in` inside a `case` arm is not yet supported here — lift the binding out of the arm",
                ));
            }
            body
        });

        let arm = pattern_parser()
            .then_ignore(arm_arrow)
            .then(arm_body)
            .map(|(pat, body)| Arm { pat, body });

        let arms = just(Tok::VSemi).repeated().ignore_then(
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

        // The duplicate check must emit here, on the delimited decls, not inside
        // the `let_expr` `try_map` below. A hard failure in that `try_map` makes
        // chumsky backtrack the whole `let … in` branch and report a positional
        // "found `let`" that outranks the custom error — the same swallowing the
        // `=>` redirect and the arm-body check work around. `.validate` +
        // `emitter.emit` is non-fatal, so the error survives backtracking and
        // still reaches `errors[0]` (mirrors the `arm_arrow` precedent above).
        let let_decls = decls_parser(expr.clone())
            .delimited_by(block_open, block_close)
            .validate(|decls, e, emitter| {
                if let Err(pe) = fold_decls_check_duplicates(&decls) {
                    emitter.emit(Rich::custom(e.span(), pe.msg));
                }
                decls
            });

        let let_expr = just(Tok::Let)
            .map_with(|_, e| span_range(e.span()).start)
            .then(let_decls)
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
        // Cons shares level 5 and is right-associative, so `a :: b :: xs`
        // groups as `a :: (b :: xs)` — a value prepended onto a list.
        "::" => (5, OpAssoc::Right),
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
        "++" => "append",
        "::" => "cons",
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
        let applied_lhs = Spanned(Expr::App(Box::new(f), Box::new(lhs)), span.clone());
        lhs = Spanned(Expr::App(Box::new(applied_lhs), Box::new(rhs)), span);
    }
    Ok(lhs)
}

fn pattern_parser<'src, I>(
) -> impl Parser<'src, I, Spanned<Pattern>, extra::Err<Rich<'src, Tok, TokSpan>>> + Clone
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

        // `int_lit`/`char_lit` reuse the same span-carrying `select!` shapes
        // `expr_parser` uses for literal expressions (ADR 0026 §2), building the
        // matching `Pattern` variant.
        let int_lit = select! { Tok::Int(n) => n }
            .map_with(|n, e| Spanned(Pattern::Int(n), span_range(e.span())));

        let char_lit = select! { Tok::Char(c) => c }
            .map_with(|c, e| Spanned(Pattern::Char(c), span_range(e.span())));

        // The pattern-side unary-minus fold (ADR 0026 §5). The lexer never puts
        // `-` inside a numeric token, and pattern position has no `unary` layer
        // to fold it, so a `-` immediately adjacent to the following `Int` (its
        // span end touching the int's span start — the adjacency test
        // `qualified` uses at parser.rs:321) folds to `Pattern::Int(-n)`. A
        // non-adjacent `-` is a plain parse error.
        let neg_int = just(Tok::Op("-".to_string()))
            .map_with(|_, e| span_range(e.span()))
            .then(select! { Tok::Int(n) => n }.map_with(|n, e| (n, span_range(e.span()))))
            .try_map(|(minus_span, (n, int_span)), span| {
                if minus_span.end == int_span.start {
                    Ok(Spanned(Pattern::Int(-n), minus_span.start..int_span.end))
                } else {
                    Err(Rich::<Tok, TokSpan>::custom(span, "expected a pattern"))
                }
            });

        // Float literals in pattern position are rejected with the dedicated
        // redirect diagnostic below — the helpful message IS the feature (ADR
        // 0026 §3). Catches a `Tok::Float`, optionally `-`-prefixed (a negative
        // float is still a float). `.validate` + `emitter.emit` — not `try_map`
        // — because this atom sits inside a `repeated()` over the `case` arms:
        // `repeated()` rewinds and swallows a hard parse failure, so the error
        // must be emitted non-fatally to survive. The returned `Pattern::Wildcard`
        // is an inert placeholder that never reaches inference. Placed first in
        // the atom `choice` so a float commits to this rejection.
        let float_reject = just(Tok::Op("-".to_string()))
            .or_not()
            .ignore_then(select! { Tok::Float(_) => () })
            .validate(|(), e, emitter| {
                emitter.emit(Rich::<Tok, TokSpan>::custom(
                    e.span(),
                    FLOAT_PATTERN_MESSAGE,
                ));
                Spanned(Pattern::Wildcard, span_range(e.span()))
            });

        let nullary_ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| Spanned(Pattern::Ctor(u, vec![]), span_range(e.span())));

        // A `[a, b, c]` literal pattern desugars right-to-left into nested
        // `Cons` ending in `Nil` — `Cons(a, Cons(b, Cons(c, Nil)))` — so
        // downstream inference and matching see only the two list constructors.
        // `[]` folds to `Nil` directly.
        let list_literal = pattern
            .clone()
            .separated_by(just(Tok::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Tok::LBracket), just(Tok::RBracket))
            .map_with(|items: Vec<Spanned<Pattern>>, e| {
                let whole = span_range(e.span());
                let nil = Spanned(Pattern::Nil, whole.end..whole.end);
                let folded = items.into_iter().rev().fold(nil, |tail, head| {
                    let span = head.1.start..whole.end;
                    Spanned(Pattern::Cons(Box::new(head), Box::new(tail)), span)
                });
                Spanned(folded.0, whole)
            });

        // Tuple *patterns* by the same count dispatch (ADR 0027 §2). Living at
        // the pattern-`atom` layer is what lets `(a, b)` compose with the `::`
        // cons tail and applied constructors, exactly as a parenthesized pattern
        // did before tuples. `()` unit, `(p)` grouping, `(a, b)`/`(a, b, c)`
        // tuple, 4+ rejected with the record redirect.
        let paren = just(Tok::LParen)
            .ignore_then(
                pattern
                    .clone()
                    .separated_by(just(Tok::Comma))
                    .collect::<Vec<_>>()
                    .then_ignore(just(Tok::RParen)),
            )
            .try_map_with(|mut items: Vec<Spanned<Pattern>>, e| match items.len() {
                0 => Ok(Spanned(Pattern::Tuple(vec![]), span_range(e.span()))),
                1 => Ok(items.pop().unwrap()),
                2 | 3 => Ok(Spanned(Pattern::Tuple(items), span_range(e.span()))),
                _ => Err(Rich::custom(e.span(), TUPLE_TOO_LARGE_MESSAGE)),
            });

        let atom = choice((
            float_reject,
            wildcard,
            var,
            str_lit,
            char_lit,
            neg_int,
            int_lit,
            nullary_ctor,
            list_literal,
            paren,
        ));

        let applied_ctor = select! { Tok::Upper(u) => u }
            .map_with(|u, e| (u, span_range(e.span()).start))
            .then(atom.clone().repeated().at_least(1).collect::<Vec<_>>())
            .map(|((name, start), args)| {
                let end = args.last().map(|a| a.1.end).unwrap_or(start);
                Spanned(Pattern::Ctor(name, args), start..end)
            });

        let ctor_or_atom = choice((applied_ctor, atom));

        // An optional trailing `:: pattern` turns the head into a `Cons`
        // (`(x :: xs)`, `(a :: b :: rest)`); the tail recurses through `pattern`
        // so cons chains nest right-associatively, matching the expression form.
        ctor_or_atom
            .clone()
            .then(
                just(Tok::Op("::".to_string()))
                    .ignore_then(pattern)
                    .or_not(),
            )
            .map(|(head, tail)| match tail {
                Some(tail) => {
                    let span = head.1.start..tail.1.end;
                    Spanned(Pattern::Cons(Box::new(head), Box::new(tail)), span)
                }
                None => head,
            })
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

    let item = binding_head()
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

    just(Tok::VSemi).repeated().ignore_then(
        item.then_ignore(just(Tok::VSemi).repeated())
            .repeated()
            .collect::<Vec<_>>(),
    )
}

enum TopItem {
    Type(TypeDecl),
    Value(DeclItem),
    Recovered,
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

    let value_item = binding_head()
        .map_with(|name, e| (name, span_range(e.span()).start))
        .then(sig_tail.or(binding_tail))
        .map(|((name, start), tail)| match tail {
            SigOrBind::Sig(ty) => {
                let end = ty.1.end;
                TopItem::Value(DeclItem::Sig {
                    name,
                    ty,
                    span: start..end,
                })
            }
            SigOrBind::Bind { params, body } => {
                let end = body.1.end;
                TopItem::Value(DeclItem::Bind {
                    name,
                    params,
                    body,
                    span: start..end,
                })
            }
        });

    let item = choice((type_decl_parser().map(TopItem::Type), value_item)).recover_with(
        skip_until(any().ignored(), just(Tok::VSemi).ignored(), || {
            TopItem::Recovered
        }),
    );

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

/// Reject a name bound twice in one declaration block — top level or `let`
/// (ADR 0032 §2d; audit #61, formerly a silent drop of the second binding).
/// Only `Bind` heads collide; a signature and its binding legitimately share a
/// name, so `Sig` items are skipped. Split out of `fold_decls` so the two call
/// paths can invoke it separately: the top level runs it fatally, but the `let`
/// path must not (see the `let_decls` emission).
fn fold_decls_check_duplicates(items: &[DeclItem]) -> Result<(), ParseError> {
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        if let DeclItem::Bind { name, span, .. } = item {
            if seen.contains(&name.as_str()) {
                return Err(ParseError {
                    msg: format!("`{name}` is defined twice — remove or rename one"),
                    span: span.clone(),
                });
            }
            seen.push(name);
        }
    }
    Ok(())
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
    matches!(
        name,
        "aptPackage"
            | "systemdService"
            | "file"
            | "directory"
            | "symlink"
            | "lineInFile"
            | "scroll"
            | "rollback"
            | "keep"
            | "retry"
    )
}

/// Dispatch a parsed `name { field = … }` to the right `Expr` variant, pulling
/// exactly the fields that constructor requires and erroring on a missing or
/// unknown field. `take_field` removes each expected field, so any left over is
/// unknown.
///
/// This is where the filesystem glyph's per-arm field set is enforced at the
/// surface (ADR 0019 §2): `file`, `directory`, and `symlink` all build
/// `Expr::Filesystem` but take different fields — `symlink` never reads a `mode`,
/// `directory` never reads `contents` — so `symlink { path, target, mode = … }`
/// leaves `mode` unclaimed and fails as an "unknown symlink field". The illegal
/// combinations (a symlink with a mode, a directory with contents) cannot be
/// written down, matching the minimal `Entry` sum they lower to.
fn build_constructor(
    ctor: &str,
    fields: &mut BTreeMap<String, Spanned<Expr>>,
    span: TokSpan,
) -> Result<Expr, Rich<'static, Tok, TokSpan>> {
    let expr = match ctor {
        "aptPackage" => Expr::AptPackage(Box::new(take_field(ctor, fields, "name", span)?)),
        "systemdService" => Expr::SystemdService(Box::new(take_field(ctor, fields, "unit", span)?)),
        "file" => Expr::Filesystem {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            entry: EntryExpr::File {
                contents: Box::new(take_field(ctor, fields, "contents", span)?),
                mode: Box::new(take_field(ctor, fields, "mode", span)?),
            },
        },
        "directory" => Expr::Filesystem {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            entry: EntryExpr::Directory {
                mode: Box::new(take_field(ctor, fields, "mode", span)?),
            },
        },
        "symlink" => Expr::Filesystem {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            entry: EntryExpr::Symlink {
                target: Box::new(take_field(ctor, fields, "target", span)?),
            },
        },
        "lineInFile" => Expr::LineInFile {
            path: Box::new(take_field(ctor, fields, "path", span)?),
            line: Box::new(take_field(ctor, fields, "line", span)?),
        },
        "scroll" => {
            let name = Box::new(take_field(ctor, fields, "name", span)?);
            let policy = fields.remove("policy").map(Box::new);
            let notifies = fields.remove("notifies").map(Box::new);
            let glyphs = fields.remove("glyphs");
            let groups = fields.remove("groups");
            // Leaf-xor-branch enforced at the surface, the same per-arm field
            // discipline `build_constructor` already applies to the filesystem
            // glyph (ADR 0019 §2 / ADR 0031 §7). The wording is load-bearing —
            // `recursive_scroll.rs` asserts on "exactly one of `glyphs` or
            // `groups`".
            let contents = match (glyphs, groups) {
                (Some(g), None) => ContentsExpr::Glyphs(Box::new(g)),
                (None, Some(g)) => ContentsExpr::Groups(Box::new(g)),
                (Some(_), Some(_)) => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` has both `glyphs` and `groups`, but needs exactly one of `glyphs` or `groups`".to_string(),
                    ))
                }
                (None, None) => {
                    return Err(Rich::custom(
                        span,
                        "`scroll` needs exactly one of `glyphs` or `groups`".to_string(),
                    ))
                }
            };
            Expr::Scroll {
                name,
                policy,
                notifies,
                contents,
            }
        }
        "retry" => Expr::PolicyRetry(std::mem::take(fields)),
        // `rollback` / `keep` parse as atoms (see `policy_word`); reaching here
        // means the author braced them, so point them at the braceless form.
        "rollback" | "keep" => {
            return Err(Rich::custom(
                span,
                format!("`{ctor}` is written without braces (e.g. `policy = {ctor}`)"),
            ))
        }
        _ => unreachable!("unknown constructor `{ctor}`"),
    };
    if let Some(extra) = fields.keys().next() {
        return Err(Rich::custom(
            span,
            format!("unknown {ctor} field `{extra}`"),
        ));
    }
    Ok(expr)
}

fn take_field(
    ctor: &str,
    fields: &mut BTreeMap<String, Spanned<Expr>>,
    field: &str,
    span: TokSpan,
) -> Result<Spanned<Expr>, Rich<'static, Tok, TokSpan>> {
    fields
        .remove(field)
        .ok_or_else(|| Rich::custom(span, format!("`{ctor}` requires a `{field}` field")))
}

/// Build an applied type constructor from a written `Upper` head. No name is
/// special-cased here: the former `Str`/`Glyphs` migration aliases are gone, so
/// they parse as ordinary `Con` heads and fail later as unknown type
/// constructors (`String` and `List Glyph` are the sole spellings).
fn type_con(name: &str, args: Vec<Type>) -> Type {
    Type::Con(name.to_string(), args)
}

fn span_range(span: TokSpan) -> Span {
    span.start..span.end
}

// The Elm-faithful redirect shown when a float literal appears in a pattern
// (ADR 0026 §3). NOTE: tests assert on this text (`tests/literal_patterns.rs`),
// so rewording it ripples.
const FLOAT_PATTERN_MESSAGE: &str = "`Float` literals can't be matched in a pattern. Floating-point equality is unreliable, so Emet — like Elm — forbids it. Bind the value with a name and compare it with `<`, `>`, `<=`, or `>=` in an `if` instead.";

// The Elm-style redirect shown when a paren form has 4+ elements: steer the
// author to a record instead of a larger tuple (ADR 0027 §2). NOTE: tests assert
// on this text — `tests/tuples.rs` checks for "3" (the cap) and "record" — so
// rewording those two anchors ripples.
const TUPLE_TOO_LARGE_MESSAGE: &str = "A tuple can have at most 3 elements. For a larger grouping, use a record with named fields instead.";

/// Rewrite chumsky's raw `expected …` error into plain language (ADR 0032 §1).
/// Chumsky's default `Display` leaks compiler internals a reader should never
/// see: duplicate entries, the virtual layout token `';'` (the offside rule's
/// statement separator, `layout.rs`), and the placeholder `something else`. This
/// keeps the message's `head` (the "found X" clause) verbatim, then rebuilds the
/// expected list — dropping the virtual `';'`, deduping, replacing `something
/// else` with `an expression`, and joining with Oxford-comma "or".
///
/// When every remaining expectation is a closing delimiter, the author most
/// likely left an opener unclosed rather than genuinely wanting a `)`/`]`/`}`
/// there, so the message ends with an "unclosed" hint naming that closer.
fn humanize_expected(raw: &str) -> String {
    let Some(idx) = raw.find("expected ") else {
        return raw.replace("something else", "an expression");
    };
    let (head, tail) = raw.split_at(idx);
    let list = &tail["expected ".len()..];
    let mut items: Vec<String> = Vec::new();
    for piece in list.split(", ") {
        let piece = piece.strip_prefix("or ").unwrap_or(piece);
        let cleaned = piece.trim().replace("something else", "an expression");
        if cleaned == "';'" {
            continue;
        }
        if items.iter().any(|existing| existing == &cleaned) {
            continue;
        }
        items.push(cleaned);
    }
    if items.is_empty() {
        return format!("{head}expected an expression");
    }
    let only_closers = items
        .iter()
        .all(|i| matches!(i.as_str(), "','" | "')'" | "']'" | "'}'" | "`}`"));
    let close_hint = if only_closers {
        items
            .iter()
            .find(|i| matches!(i.as_str(), "')'" | "']'" | "'}'" | "`}`"))
            .map(|d| format!(" — this looks like an unclosed {d}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    // Hints keyed on the shape of an unexpected token, appended after the
    // `expected …` list. The `=>` case here is the fallback for a `=>` that
    // reaches the humanizer un-redirected; an arm's `=>` is caught earlier at
    // `arm_arrow`. `\` covers a lambda written with the wrong head (audit
    // #23/#24); an expected `=` covers a definition missing it (audit #04).
    let found_hint = if raw.contains("found '=>'") {
        " — case arms use `->`, not `=>`"
    } else if raw.contains("found '\\'") {
        " — a lambda is written `\\x -> body`"
    } else if items.iter().any(|i| i == "'='") {
        " — this looks like a definition missing its `=`"
    } else {
        ""
    };
    let joined = match items.len() {
        1 => items[0].clone(),
        2 => format!("{} or {}", items[0], items[1]),
        _ => {
            let last = items.pop().unwrap();
            format!("{}, or {}", items.join(", "), last)
        }
    };
    format!("{head}expected {joined}{close_hint}{found_hint}")
}

/// Parse a complete laid-out token stream (as produced by
/// `layout::layout_all`) into a `Module`. Returns every error chumsky
/// collected — each run through `humanize_expected` — or one synthetic
/// "unexpected end of input" error if parsing failed without producing any.
pub fn parse(
    tokens: &[Token],
    name: Option<String>,
    exposing: Exposing,
    imports: Vec<Import>,
) -> Result<Module, Vec<ParseError>> {
    let eoi = tokens.last().map(|t| t.span.end).unwrap_or(0);
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
                msg: humanize_expected(&e.to_string()),
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
            TopItem::Recovered => {}
        }
    }

    fold_decls_check_duplicates(&value_items)
        .and_then(|()| fold_decls(value_items))
        .map(|decls| Module {
            name,
            exposing,
            imports,
            type_decls,
            decls,
        })
        .map_err(|pe| vec![pe])
}
