//! The module header, peeled off the raw token stream *before* layout runs.
//!
//! `module Name exposing (..)` and the `import` lines sit at column zero and do
//! not follow the offside rule the rest of the file does, so [`split`] consumes
//! them as a flat token prefix and hands the remaining `body` tokens to
//! `layout.rs`. A file with no `module` line is a valid entry module: `name` is
//! `None` (the resolver derives it from the path) and everything is exposed
//! (`Exposing::All`). The module system this parses is specified in ADR 0016.

use crate::ast::{Exposed, Exposing, Import, ImportExposing};
use crate::lexer::{Tok, Token};

pub struct HeaderError {
    pub msg: String,
    pub span: std::ops::Range<usize>,
}

/// The split result: the optional module name and its `exposing` list, the
/// `import` declarations, and the `body` tokens that still need laying out.
pub struct Header {
    pub name: Option<String>,
    pub exposing: Exposing,
    pub imports: Vec<Import>,
    pub body: Vec<Token>,
}

struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].tok
    }

    fn span(&self) -> std::ops::Range<usize> {
        self.tokens[self.pos.min(self.tokens.len() - 1)]
            .span
            .clone()
    }

    fn bump(&mut self) -> &Token {
        let t = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> Result<(), HeaderError> {
        if self.peek() == tok {
            self.bump();
            Ok(())
        } else {
            Err(HeaderError {
                msg: format!("expected {what}, found `{}`", self.peek()),
                span: self.span(),
            })
        }
    }
}

/// Consume the leading `module` header (if any) and every `import` line,
/// returning them alongside the untouched body tokens. A file that does not
/// start with `module` is treated as a header-less entry module.
pub fn split(tokens: Vec<Token>) -> Result<Header, HeaderError> {
    let mut cursor = Cursor {
        tokens: &tokens,
        pos: 0,
    };

    let (name, exposing) = if *cursor.peek() == Tok::Module {
        parse_module_header(&mut cursor)?
    } else {
        (None, Exposing::All)
    };

    let mut imports = Vec::new();
    while *cursor.peek() == Tok::Import {
        imports.push(parse_import(&mut cursor)?);
    }

    let body = tokens[cursor.pos..].to_vec();
    Ok(Header {
        name,
        exposing,
        imports,
        body,
    })
}

fn parse_module_header(cursor: &mut Cursor) -> Result<(Option<String>, Exposing), HeaderError> {
    cursor.bump();
    let name = parse_module_name(cursor)?;
    cursor.expect(&Tok::Exposing, "`exposing`")?;
    let exposing = parse_exposing(cursor)?;
    Ok((Some(name), exposing))
}

fn parse_module_name(cursor: &mut Cursor) -> Result<String, HeaderError> {
    match cursor.peek().clone() {
        Tok::Upper(name) => {
            cursor.bump();
            Ok(name)
        }
        other => Err(HeaderError {
            msg: format!("expected a module name, found `{other}`"),
            span: cursor.span(),
        }),
    }
}

fn parse_exposing(cursor: &mut Cursor) -> Result<Exposing, HeaderError> {
    cursor.expect(&Tok::LParen, "`(`")?;
    if *cursor.peek() == Tok::Dot {
        parse_double_dot(cursor)?;
        cursor.expect(&Tok::RParen, "`)`")?;
        return Ok(Exposing::All);
    }
    let items = parse_exposed_list(cursor)?;
    cursor.expect(&Tok::RParen, "`)`")?;
    Ok(Exposing::Explicit(items))
}

fn parse_exposed_list(cursor: &mut Cursor) -> Result<Vec<Exposed>, HeaderError> {
    let mut items = Vec::new();
    loop {
        items.push(parse_exposed(cursor)?);
        if *cursor.peek() == Tok::Comma {
            cursor.bump();
        } else {
            break;
        }
    }
    Ok(items)
}

fn parse_exposed(cursor: &mut Cursor) -> Result<Exposed, HeaderError> {
    match cursor.peek().clone() {
        Tok::Ident(name) => {
            cursor.bump();
            Ok(Exposed::Value(name))
        }
        Tok::Upper(name) => {
            cursor.bump();
            let open = if *cursor.peek() == Tok::LParen {
                cursor.bump();
                parse_double_dot(cursor)?;
                cursor.expect(&Tok::RParen, "`)`")?;
                true
            } else {
                false
            };
            Ok(Exposed::Type { name, open })
        }
        other => Err(HeaderError {
            msg: format!("expected an exposed name, found `{other}`"),
            span: cursor.span(),
        }),
    }
}

fn parse_double_dot(cursor: &mut Cursor) -> Result<(), HeaderError> {
    cursor.expect(&Tok::Dot, "`..`")?;
    cursor.expect(&Tok::Dot, "`..`")?;
    Ok(())
}

fn parse_import(cursor: &mut Cursor) -> Result<Import, HeaderError> {
    let start = cursor.span().start;
    cursor.bump();
    let module = parse_module_name(cursor)?;
    let alias = if *cursor.peek() == Tok::As {
        cursor.bump();
        Some(parse_module_name(cursor)?)
    } else {
        None
    };
    let exposing = if *cursor.peek() == Tok::Exposing {
        cursor.bump();
        cursor.expect(&Tok::LParen, "`(`")?;
        let items = parse_exposed_list(cursor)?;
        cursor.expect(&Tok::RParen, "`)`")?;
        ImportExposing::Explicit(items)
    } else {
        ImportExposing::None
    };
    let end = cursor.span().start;
    Ok(Import {
        module,
        alias,
        exposing,
        span: start..end,
    })
}
