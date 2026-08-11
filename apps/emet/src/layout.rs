//! Layout (the "offside rule"), following the Haskell 2010 report section 10.3.
//!
//! The report defines two functions:
//!   * a preprocessor that annotates the token stream with `{n}` (indentation
//!     of the first token of a line) and `<n>` markers, and
//!   * `L`, which consumes that annotated stream plus a stack `ms` of layout
//!     contexts and emits explicit `{`, `;`, `}` virtual tokens.
//!
//! The awkward clause is:  L (t : ts) (m : ms) = } : (L (t : ts) ms)   if the
//! enclosing context can be closed by a *parse error* — "parse-error(t)". A
//! pure lexer cannot know about parse errors in general, but the one place
//! this codebase needs the clause is fixed: closing a `let`/`where`/`of`
//! block on reaching `in`, so a single-line `let x = e in e` parses. So
//! layout special-cases that one trigger (`Ctx::Implicit` tagged with
//! `Origin::Keyword`, closed on `in` — see `advance`) and realises
//! parse-error(t) itself, with no feedback from the parser.
//!
//! `layout_all` runs the whole thing — offside rule plus the close-on-`in`
//! rule — eagerly, and returns a complete token stream that downstream
//! parsing (chumsky, in `parser.rs`) consumes with no further layout
//! involvement.

use crate::lexer::{Tok, Token};

/// Where an implicit layout context came from. `Module` is the block wrapping
/// the whole module (opened at the first token). `Keyword` is a block opened by
/// a layout keyword (`let`/`where`/`of`). Only a `Keyword` context is closed by
/// the parse-error(t) rule fired on `in` (see `advance`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    Module,
    Keyword,
}

/// A layout context: `Explicit` means the user wrote a literal `{`, and no
/// implicit `;`/`}` are inserted inside it. `Implicit { column, origin }` is a
/// layout block whose reference column is `column`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ctx {
    Explicit,
    Implicit { column: usize, origin: Origin },
}

/// Streaming layout processor. `next` pulls one laid-out token at a time,
/// internally applying the close-on-`in` rule (see the module doc) as it
/// goes — no caller feedback needed.
struct Layout {
    /// input tokens (already lexed), reversed for cheap pop from the back
    input: Vec<Token>,
    pos: usize,
    /// pending output already computed but not yet consumed
    queue: std::collections::VecDeque<Token>,
    stack: Vec<Ctx>,
    /// have we emitted the very first token yet? The report opens an implicit
    /// block for the whole module if it does not begin with `{` or `let`.
    started: bool,
    /// set right after opening an implicit block, so the block's first token is
    /// not treated as a same-column `;` continuation of that very block
    just_opened: bool,
    done: bool,
}

fn vtok(kind: Tok, model: &Token) -> Token {
    Token {
        tok: kind,
        span: model.span.start..model.span.start,
        line: model.line,
        col: model.col,
        first_on_line: false,
    }
}

impl Layout {
    fn new(input: Vec<Token>) -> Self {
        Layout {
            input,
            pos: 0,
            queue: std::collections::VecDeque::new(),
            stack: Vec::new(),
            started: false,
            just_opened: false,
            done: false,
        }
    }

    fn peek_input(&self) -> &Token {
        &self.input[self.pos.min(self.input.len() - 1)]
    }

    /// Is the given token one that OPENS a layout block (i.e. is a layout
    /// keyword: let / where / of)? Such a keyword is followed by an implicit
    /// `{` at the indentation of the next token (unless the next token is a
    /// literal `{`).
    fn opens_layout(t: &Tok) -> bool {
        matches!(t, Tok::Let | Tok::Where | Tok::Of)
    }

    /// Produce the next laid-out token, driving the L algorithm. Returns Eof
    /// tokens forever once exhausted.
    fn next(&mut self) -> Token {
        if let Some(t) = self.queue.pop_front() {
            return t;
        }
        self.advance();
        self.queue
            .pop_front()
            .unwrap_or_else(|| vtok(Tok::Eof, self.peek_input()))
    }

    fn top(&self) -> Option<Ctx> {
        self.stack.last().copied()
    }

    fn advance(&mut self) {
        if self.done {
            return;
        }

        // Module-level implicit open: if the first lexeme is not `{` and not
        // `let`... actually per report it's: if the module does not begin with
        // `{`, an implicit brace opens at the column of the first token.
        if !self.started {
            self.started = true;
            let first = self.peek_input().clone();
            if first.tok == Tok::Eof {
                self.queue.push_back(first);
                self.done = true;
                return;
            }
            if first.tok != Tok::LBrace {
                let n = first.col;
                self.stack.push(Ctx::Implicit {
                    column: n,
                    origin: Origin::Module,
                });
                self.queue.push_back(vtok(Tok::VLBrace, &first));
                self.just_opened = true;
            }
            // fall through to emit the first token normally below
        }

        let t = self.peek_input().clone();

        // End of input: close all implicit contexts.
        if t.tok == Tok::Eof {
            while let Some(ctx) = self.stack.last().copied() {
                match ctx {
                    Ctx::Implicit { .. } => {
                        self.stack.pop();
                        self.queue.push_back(vtok(Tok::VRBrace, &t));
                    }
                    Ctx::Explicit => {
                        // unmatched explicit brace at EOF: let the parser report it
                        self.stack.pop();
                    }
                }
            }
            self.queue.push_back(t);
            self.done = true;
            return;
        }

        // Newline handling: if this token is the first on its line and we are
        // inside an implicit context, compare columns (the `<n>` rule). The
        // `just_opened` flag suppresses a spurious leading `;`/dedent for the
        // very first token after a block opens; it is cleared unconditionally
        // below so it can never leak to a later token.
        let opened = self.just_opened;
        self.just_opened = false;
        if t.first_on_line && !opened {
            if let Some(Ctx::Implicit { column: m, .. }) = self.top() {
                if t.col == m {
                    // same indentation: new item in the block -> `;`
                    self.queue.push_back(vtok(Tok::VSemi, &t));
                } else if t.col < m {
                    // dedent: close this context and re-examine at the enclosing
                    // one on the next advance (do NOT consume t yet). Restore the
                    // flag we cleared, since we haven't consumed t.
                    self.stack.pop();
                    self.queue.push_back(vtok(Tok::VRBrace, &t));
                    self.just_opened = opened;
                    return;
                }
                // t.col > m: continuation line, emit nothing special
            }
        }

        // parse-error(t) for `in`: a single-line `let x = e in e` reaches `in`
        // with its keyword-opened block still on top (no dedent closed it), so
        // close that block here before emitting `in`. In the multi-line case a
        // dedent has already closed the block and the top is the module context,
        // so this fires nothing and the output is unchanged.
        if t.tok == Tok::In {
            if let Some(Ctx::Implicit {
                origin: Origin::Keyword,
                ..
            }) = self.top()
            {
                self.stack.pop();
                self.queue.push_back(vtok(Tok::VRBrace, &t));
            }
        }

        // Emit the token itself.
        self.pos += 1;
        self.queue.push_back(t.clone());

        // If this token opens a layout block, decide the implicit brace using
        // the NEXT token's column.
        if Self::opens_layout(&t.tok) {
            let next = self.peek_input().clone();
            if next.tok == Tok::LBrace {
                // explicit brace follows: user is overriding layout
                self.pos += 1;
                self.stack.push(Ctx::Explicit);
                self.queue.push_back(next);
            } else if next.tok == Tok::Eof {
                // `let` at end of input: open+close empty block
                self.queue.push_back(vtok(Tok::VLBrace, &next));
                self.queue.push_back(vtok(Tok::VRBrace, &next));
            } else {
                let n = next.col;
                // Per report, if the new indentation is not greater than the
                // enclosing implicit context, the block is empty `{}`.
                let enclosing = self
                    .stack
                    .iter()
                    .rev()
                    .find_map(|c| match c {
                        Ctx::Implicit { column: k, .. } => Some(*k),
                        Ctx::Explicit => None,
                    })
                    .unwrap_or(0);
                if n > enclosing {
                    self.stack.push(Ctx::Implicit {
                        column: n,
                        origin: Origin::Keyword,
                    });
                    self.queue.push_back(vtok(Tok::VLBrace, &next));
                    self.just_opened = true;
                } else {
                    self.queue.push_back(vtok(Tok::VLBrace, &next));
                    self.queue.push_back(vtok(Tok::VRBrace, &next));
                }
            }
        }

        // Explicit braces the user wrote adjust the stack too.
        match t.tok {
            Tok::LBrace => self.stack.push(Ctx::Explicit),
            Tok::RBrace => {
                if let Some(Ctx::Explicit) = self.top() {
                    self.stack.pop();
                }
            }
            _ => {}
        }
    }
}

/// Run layout eagerly to completion, including the close-on-`in` rule that
/// realises parse-error(t). Returns the full virtual-token stream ending in
/// Eof, ready for `parser::parse`.
pub fn layout_all(input: Vec<Token>) -> Vec<Token> {
    let mut l = Layout::new(input);
    let mut out = Vec::new();
    loop {
        let t = l.next();
        let is_eof = t.tok == Tok::Eof;
        out.push(t);
        if is_eof {
            break;
        }
    }
    out
}
