//! Lexer: source text -> `Vec<Token>` with spans.
//!
//! The lexer is layout-agnostic: it records the column of the first token on
//! each line (via `Token::col`) so the separate layout pass (`layout.rs`) can
//! apply the offside rule. Newlines are NOT emitted as tokens; instead each
//! token knows its line and column.
//!
//! Numbers and operators (ADR 0007): a digit run lexes to `Int`, or `Float`
//! when a `.` with a following digit is present. Operator symbols are lexed by
//! maximal munch into a single `Op` — so `<=` is one token, not `<` then `=` —
//! with `->` and `--` (arrow, comment) taking priority over `Op`.
//!
//! Interpolated strings (ADR 0004): a `"…"` with no `${` lexes to a single
//! `Str`, unchanged. With interpolation it lexes into a sub-token sequence —
//! `StrPart` literal chunks around `InterpStart` / embedded tokens /
//! `InterpEnd` — so the parser can desugar it to `String.concat`. Inside
//! `${ … }` the lexer tracks brace depth (`brace_depth` vs. the depth recorded
//! in `interp_open_depths`) so a record or `case` brace does not close the
//! interpolation early; `\${` escapes a literal `${`.

use std::fmt;
use std::ops::Range;

pub type Span = Range<usize>;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // literals / names
    Ident(String), // lowercase-initial: variables, fields
    Upper(String), // uppercase-initial: type/data constructors (Str, Glyph, Just, ...)
    /// A complete string literal with no interpolation.
    Str(String),
    /// A literal chunk of an interpolated string — the text around each `${…}`.
    StrPart(String),
    /// `${` — opens an embedded expression inside an interpolated string.
    InterpStart,
    /// `}` — closes an embedded expression (recognized by brace-depth tracking,
    /// not by the raw `}`).
    InterpEnd,
    Int(i64),
    Float(f64),
    /// A maximal-munch run of operator characters (`+`, `<=`, `++`, …).
    Op(String),

    // keywords
    Let,
    In,
    Where,
    Of, // opens a `case ... of` layout block
    Case,
    If,
    Then,
    Else,
    Type,
    Module,
    Import,
    Exposing,
    As,

    // punctuation
    Equals,   // =
    Backslash,// \
    Arrow,    // ->
    Colon,    // :
    Comma,    // ,
    Dot,      // .
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,   // explicit { written by the user
    RBrace,   // explicit }

    // virtual tokens inserted by the layout algorithm
    VLBrace,  // {n}  -> virtual open
    VRBrace,  // virtual close
    VSemi,    // virtual ;  (separates decls at same indent)

    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Tok::*;
        match self {
            Ident(s) => write!(f, "{s}"),
            Upper(s) => write!(f, "{s}"),
            Str(s) => write!(f, "{s:?}"),
            StrPart(s) => write!(f, "{s:?}"),
            InterpStart => write!(f, "${{"),
            InterpEnd => write!(f, "}}"),
            Int(n) => write!(f, "{n}"),
            Float(x) => write!(f, "{x}"),
            Op(s) => write!(f, "{s}"),
            Let => write!(f, "let"),
            In => write!(f, "in"),
            Where => write!(f, "where"),
            Of => write!(f, "of"),
            Case => write!(f, "case"),
            If => write!(f, "if"),
            Then => write!(f, "then"),
            Else => write!(f, "else"),
            Type => write!(f, "type"),
            Module => write!(f, "module"),
            Import => write!(f, "import"),
            Exposing => write!(f, "exposing"),
            As => write!(f, "as"),
            Equals => write!(f, "="),
            Backslash => write!(f, "\\"),
            Arrow => write!(f, "->"),
            Colon => write!(f, ":"),
            Comma => write!(f, ","),
            Dot => write!(f, "."),
            LParen => write!(f, "("),
            RParen => write!(f, ")"),
            LBracket => write!(f, "["),
            RBracket => write!(f, "]"),
            LBrace => write!(f, "{{"),
            RBrace => write!(f, "}}"),
            VLBrace => write!(f, "{{"),
            VRBrace => write!(f, "}}"),
            VSemi => write!(f, ";"),
            Eof => write!(f, "<eof>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    pub line: usize,
    pub col: usize,       // 1-based column of this token's first char
    pub first_on_line: bool,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub msg: String,
    pub span: Span,
}

fn is_operator_char(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '^' | '<' | '>' | '=' | '&' | '|')
}

enum SegmentEnd {
    ClosingQuote,
    Interpolation,
}

struct Segment {
    literal: String,
    end: SegmentEnd,
    next_i: usize,
    next_col: usize,
}

struct SegmentEmission {
    next_i: usize,
    next_col: usize,
    entered_interpolation: bool,
}

/// Scan and push one string segment, then (if it ended at a `${`) push an
/// `InterpStart`. The `is_first_segment` + closing-quote case is coalesced into
/// a single plain `Str` token — the common non-interpolated path — spanning the
/// whole `"…"`; every other segment becomes a `StrPart`. Returns whether an
/// interpolation was entered so the caller can update its brace-depth stack.
#[allow(clippy::too_many_arguments)]
fn emit_string_segment(
    toks: &mut Vec<Token>,
    chars: &[char],
    byte_at: &[usize],
    string_start_byte: usize,
    line: usize,
    first_on_line: bool,
    is_first_segment: bool,
    seg_start_i: usize,
    seg_start_col: usize,
) -> Result<SegmentEmission, LexError> {
    let seg = scan_string_segment(chars, byte_at, seg_start_i, seg_start_col, string_start_byte)?;
    let entered_interpolation = matches!(seg.end, SegmentEnd::Interpolation);
    let content_end_i = match seg.end {
        SegmentEnd::ClosingQuote => seg.next_i - 1,
        SegmentEnd::Interpolation => seg.next_i - 2,
    };
    let coalesced_literal = is_first_segment && matches!(seg.end, SegmentEnd::ClosingQuote);
    let part_span = if coalesced_literal {
        string_start_byte..byte_at[seg.next_i]
    } else {
        byte_at[seg_start_i]..byte_at[content_end_i]
    };
    let part_col = if coalesced_literal { seg_start_col - 1 } else { seg_start_col };
    let part_tok = if coalesced_literal {
        Tok::Str(seg.literal)
    } else {
        Tok::StrPart(seg.literal)
    };
    toks.push(Token {
        tok: part_tok,
        span: part_span,
        line,
        col: part_col,
        first_on_line,
    });
    if entered_interpolation {
        toks.push(Token {
            tok: Tok::InterpStart,
            span: byte_at[content_end_i]..byte_at[seg.next_i],
            line,
            col: seg.next_col - 2,
            first_on_line: false,
        });
    }
    Ok(SegmentEmission {
        next_i: seg.next_i,
        next_col: seg.next_col,
        entered_interpolation,
    })
}

/// Consume characters of one string segment up to the next `"` or `${`,
/// resolving backslash escapes (`\n`, `\t`, `\"`, `\\`, `\${` → literal `${`,
/// and `\x` → `x` for anything else). A raw newline or end-of-input before the
/// closing quote is an "unterminated string literal" error.
fn scan_string_segment(
    chars: &[char],
    byte_at: &[usize],
    start: usize,
    start_col: usize,
    string_start_byte: usize,
) -> Result<Segment, LexError> {
    let mut i = start;
    let mut col = start_col;
    let mut literal = String::new();
    loop {
        if i >= chars.len() {
            return Err(LexError {
                msg: "unterminated string literal".into(),
                span: string_start_byte..byte_at[i.min(chars.len())],
            });
        }
        let c = chars[i];
        if c == '"' {
            return Ok(Segment {
                literal,
                end: SegmentEnd::ClosingQuote,
                next_i: i + 1,
                next_col: col + 1,
            });
        }
        if c == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            return Ok(Segment {
                literal,
                end: SegmentEnd::Interpolation,
                next_i: i + 2,
                next_col: col + 2,
            });
        }
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '$' && i + 2 < chars.len() && chars[i + 2] == '{' {
                literal.push('$');
                literal.push('{');
                i += 3;
                col += 3;
                continue;
            }
            let repl = match next {
                'n' => '\n',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            };
            literal.push(repl);
            i += 2;
            col += 2;
            continue;
        }
        if c == '\n' {
            return Err(LexError {
                msg: "unterminated string literal".into(),
                span: string_start_byte..byte_at[i],
            });
        }
        literal.push(c);
        i += 1;
        col += 1;
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    // byte offset of each char index, for spans
    let mut byte_at = Vec::with_capacity(chars.len() + 1);
    {
        let mut b = 0;
        for c in &chars {
            byte_at.push(b);
            b += c.len_utf8();
        }
        byte_at.push(b);
    }

    let mut toks = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut seen_on_line = false; // whether a non-space token has appeared
    let mut interp_open_depths: Vec<usize> = Vec::new();
    let mut brace_depth = 0usize;

    let is_ident_start = |c: char| c.is_ascii_lowercase() || c == '_';
    let is_upper_start = |c: char| c.is_ascii_uppercase();
    let is_ident_cont = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '\'';

    while i < chars.len() {
        let c = chars[i];

        // whitespace
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            seen_on_line = false;
            continue;
        }
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }
        // line comment `-- ...`
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '-' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        let start_i = i;
        let start_col = col;
        let start_byte = byte_at[i];
        let first_on_line = !seen_on_line;
        seen_on_line = true;

        macro_rules! push {
            ($t:expr, $len_chars:expr) => {{
                let len = $len_chars;
                let end_byte = byte_at[i + len];
                toks.push(Token {
                    tok: $t,
                    span: start_byte..end_byte,
                    line,
                    col: start_col,
                    first_on_line,
                });
                i += len;
                col += len;
            }};
        }

        // multi-char punctuation first
        if c == '-' && i + 1 < chars.len() && chars[i + 1] == '>' {
            push!(Tok::Arrow, 2);
            continue;
        }

        if c == '=' && !(i + 1 < chars.len() && is_operator_char(chars[i + 1])) {
            push!(Tok::Equals, 1);
            continue;
        }

        // `::` is the cons operator, lexed before the single `:` (type
        // annotation) so `head :: tail` does not read as two colons. Parsed as
        // an ordinary infix `Op` (right-assoc, level 5) desugaring to the
        // `cons` builtin; also the pattern separator in `(head :: tail)`.
        if c == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            push!(Tok::Op("::".to_string()), 2);
            continue;
        }

        match c {
            '\\' => { push!(Tok::Backslash, 1); continue; }
            ':' => { push!(Tok::Colon, 1); continue; }
            ',' => { push!(Tok::Comma, 1); continue; }
            '.' => { push!(Tok::Dot, 1); continue; }
            '(' => { push!(Tok::LParen, 1); continue; }
            ')' => { push!(Tok::RParen, 1); continue; }
            '[' => { push!(Tok::LBracket, 1); continue; }
            ']' => { push!(Tok::RBracket, 1); continue; }
            '{' => {
                brace_depth += 1;
                push!(Tok::LBrace, 1);
                continue;
            }
            '}' => {
                // A `}` closes an interpolation only if it sits at the exact
                // brace depth where that `${` opened; otherwise it is an
                // ordinary record/`case` closing brace. This is what lets
                // `"${ {f = 1}.f }"` nest braces inside an interpolation.
                if interp_open_depths.last() == Some(&brace_depth) {
                    interp_open_depths.pop();
                    push!(Tok::InterpEnd, 1);
                    let emission = emit_string_segment(
                        &mut toks, &chars, &byte_at, start_byte, line,
                        false, false, i, col,
                    )?;
                    if emission.entered_interpolation {
                        interp_open_depths.push(brace_depth);
                    }
                    i = emission.next_i;
                    col = emission.next_col;
                    continue;
                }
                brace_depth -= 1;
                push!(Tok::RBrace, 1);
                continue;
            }
            _ => {}
        }

        if c.is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let is_float = j + 1 < chars.len()
                && chars[j] == '.'
                && chars[j + 1].is_ascii_digit();
            if is_float {
                j += 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let text: String = chars[i..j].iter().collect();
                let value: f64 = text.parse().map_err(|_| LexError {
                    msg: format!("invalid float literal `{text}`"),
                    span: start_byte..byte_at[j],
                })?;
                let len = j - i;
                let end_byte = byte_at[j];
                toks.push(Token { tok: Tok::Float(value), span: start_byte..end_byte, line, col: start_col, first_on_line });
                i = j;
                col += len;
                continue;
            }
            let text: String = chars[i..j].iter().collect();
            let value: i64 = text.parse().map_err(|_| LexError {
                msg: format!("integer literal `{text}` out of range"),
                span: start_byte..byte_at[j],
            })?;
            let len = j - i;
            let end_byte = byte_at[j];
            toks.push(Token { tok: Tok::Int(value), span: start_byte..end_byte, line, col: start_col, first_on_line });
            i = j;
            col += len;
            continue;
        }

        if is_operator_char(c) {
            let mut j = i + 1;
            while j < chars.len() && is_operator_char(chars[j]) {
                j += 1;
            }
            let sym: String = chars[i..j].iter().collect();
            let len = j - i;
            let end_byte = byte_at[j];
            toks.push(Token { tok: Tok::Op(sym), span: start_byte..end_byte, line, col: start_col, first_on_line });
            i = j;
            col += len;
            continue;
        }

        // string literal
        if c == '"' {
            let emission = emit_string_segment(
                &mut toks, &chars, &byte_at, start_byte, line, first_on_line,
                true, i + 1, start_col + 1,
            )?;
            if emission.entered_interpolation {
                interp_open_depths.push(brace_depth);
            }
            i = emission.next_i;
            col = emission.next_col;
            continue;
        }

        // identifiers / keywords
        if is_ident_start(c) {
            let mut j = i + 1;
            while j < chars.len() && is_ident_cont(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let tok = match word.as_str() {
                "let" => Tok::Let,
                "in" => Tok::In,
                "where" => Tok::Where,
                "of" => Tok::Of,
                "case" => Tok::Case,
                "if" => Tok::If,
                "then" => Tok::Then,
                "else" => Tok::Else,
                "type" => Tok::Type,
                "module" => Tok::Module,
                "import" => Tok::Import,
                "exposing" => Tok::Exposing,
                "as" => Tok::As,
                _ => Tok::Ident(word),
            };
            let len = j - i;
            let end_byte = byte_at[j];
            toks.push(Token { tok, span: start_byte..end_byte, line, col: start_col, first_on_line });
            i = j;
            col += len;
            continue;
        }
        if is_upper_start(c) {
            let mut j = i + 1;
            while j < chars.len() && is_ident_cont(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let len = j - i;
            let end_byte = byte_at[j];
            toks.push(Token {
                tok: Tok::Upper(word),
                span: start_byte..end_byte,
                line,
                col: start_col,
                first_on_line,
            });
            i = j;
            col += len;
            continue;
        }

        return Err(LexError {
            msg: format!("unexpected character `{c}`"),
            span: start_byte..byte_at[start_i + 1],
        });
    }

    toks.push(Token {
        tok: Tok::Eof,
        span: byte_at[chars.len()]..byte_at[chars.len()],
        line,
        col,
        first_on_line: !seen_on_line,
    });
    Ok(toks)
}
