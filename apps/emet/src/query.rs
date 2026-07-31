//! The position index the LSP queries (ADR 0018). Inference records, per source
//! span, the type it inferred, the scope in effect, and where each used name is
//! defined; this module turns those recordings into byte-offset lookups. The
//! index is built only when inference runs with a `Recorder` attached
//! (`analyze_module`), so `emetc` pays nothing for it.

use std::collections::HashMap;

use crate::ast::{Module, Scheme, Span, Type, TypeDecl};

/// Identifies one lexical scope in `scope_table`. Every `let`/lambda/`case`-arm
/// body opens one, carrying the names visible inside it.
pub type ScopeId = usize;

/// Where a name is defined: the span of its defining occurrence, and the owning
/// module when the definition lives in another file (`None` for same-file
/// definitions). The `module` is what lets go-to-definition cross a file
/// boundary — see `resolve::import_def_sites`.
#[derive(Debug, Clone)]
pub struct DefSite {
    pub span: Span,
    pub module: Option<String>,
}

/// The span-keyed record inference leaves for the LSP. Each field is a flat list
/// of `(span, fact)` pairs (or a scope's name table); the `*_at` methods resolve
/// a byte offset against them by **smallest-enclosing-span**, so the innermost,
/// most specific fact at the cursor wins over any wider one that also contains
/// it.
#[derive(Default)]
pub struct QueryIndex {
    pub types: Vec<(Span, Type)>,
    pub scopes: Vec<(Span, ScopeId)>,
    pub scope_table: HashMap<ScopeId, Vec<(String, Scheme)>>,
    pub defs: Vec<(Span, DefSite)>,
    pub type_definitions: HashMap<String, TypeDefinition>,
}

#[derive(Debug, Clone)]
pub struct TypeDefinition {
    pub declaration: String,
    pub site: DefSite,
}

impl QueryIndex {
    /// The inferred type of the smallest recorded expression containing
    /// `offset` — the type the LSP shows on hover.
    pub fn type_at(&self, offset: usize) -> Option<&Type> {
        self.types
            .iter()
            .filter(|(span, _)| span.contains(&offset) || span.start == offset)
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
            .map(|(_, ty)| ty)
    }

    /// Every name visible at `offset`, paired with its rendered type — the
    /// completion candidates. Reads the innermost scope containing the offset.
    pub fn names_in_scope(&self, offset: usize) -> Vec<(String, String)> {
        let scope = self
            .scopes
            .iter()
            .filter(|(span, _)| span.contains(&offset) || span.start == offset)
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
            .map(|(_, id)| *id);
        match scope.and_then(|id| self.scope_table.get(&id)) {
            Some(names) => names
                .iter()
                .map(|(name, scheme)| (name.clone(), display_scheme(scheme)))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Where the name used at `offset` is defined — the go-to-definition
    /// target. A same-file result carries `module: None`; a cross-file one
    /// names the owning module for the adapter to resolve to a path.
    pub fn definition_at(&self, offset: usize) -> Option<&DefSite> {
        self.defs
            .iter()
            .filter(|(span, _)| span.contains(&offset) || span.start == offset)
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
            .map(|(_, site)| site)
    }
}

pub fn display_scheme(scheme: &Scheme) -> String {
    scheme.ty.to_string()
}

/// The type name written at `offset`, read from the token stream rather than the
/// index — types have no spans to record. Uppercase only: a lowercase token at a
/// cursor is a value, which the index answers for, so restricting the fallback
/// keeps it from guessing at names it knows nothing about.
pub fn type_name_at(source: &str, offset: usize) -> Option<String> {
    let tokens = crate::lexer::lex(source).ok()?;
    tokens
        .iter()
        .find(|token| token.span.contains(&offset) || token.span.start == offset)
        .and_then(|token| match &token.tok {
            crate::lexer::Tok::Upper(name) => Some(name.clone()),
            _ => None,
        })
}

/// This module's own `type` declarations, keyed by name. Always rendered with
/// their constructors: everything a module declares is in scope inside it.
/// `resolve::type_definitions` adds the imported ones, where visibility applies.
pub fn local_type_definitions(module: &Module) -> HashMap<String, TypeDefinition> {
    module
        .type_decls
        .iter()
        .map(|declaration| {
            (
                declaration.name.clone(),
                TypeDefinition {
                    declaration: render_type_declaration(declaration, true),
                    site: DefSite {
                        span: declaration.span.clone(),
                        module: None,
                    },
                },
            )
        })
        .collect()
}

/// A `type` declaration as source-shaped text for hover. `with_constructors` is
/// false when the reader cannot name them — an imported type exposed without
/// `(..)` — so the hover shows what is in scope, not the exporter's internals.
///
/// NOTE: record fields come out alphabetical, not in the order they were
/// written. `ast::Type::Record` is a `BTreeMap`; source order is retained
/// nowhere.
pub fn render_type_declaration(declaration: &TypeDecl, with_constructors: bool) -> String {
    let mut rendered = format!("type {}", declaration.name);
    for param in &declaration.params {
        rendered.push_str(&format!(" {param}"));
    }
    if !with_constructors {
        return rendered;
    }
    for (position, variant) in declaration.variants.iter().enumerate() {
        let bullet = if position == 0 { '=' } else { '|' };
        let fields: Vec<String> = variant.fields.iter().map(|f| render_field(&f.0)).collect();
        rendered.push_str(&format!("\n    {bullet} {}", variant.name));
        for field in fields {
            rendered.push_str(&format!(" {field}"));
        }
    }
    rendered
}

/// How a builtin type is shown on hover. Most builtins are rendered from the
/// prelude's own constructors, so `Glyph` shows its four arms; the scroll-side
/// types have none to read and come from `prelude::builtin_type_documentation`
/// instead. `List` is excluded because its `[]`/`::` members are synthetic —
/// they exist for the exhaustiveness checker, not as constructors an author
/// writes. Anything left over is its name alone.
pub fn builtin_type_declaration(name: &str) -> Option<String> {
    let arity = crate::infer::builtin_type_arity(name)?;
    if let Some(documented) = crate::prelude::builtin_type_documentation(name) {
        return Some(documented.shape.to_string());
    }
    let mut rendered = format!("type {name}");
    for param in TYPE_PARAMETER_NAMES.iter().take(arity) {
        rendered.push_str(&format!(" {param}"));
    }
    let members = match crate::prelude::sum_type_constructors(name) {
        Some(members) if name != "List" => members,
        _ => return Some(rendered),
    };
    for (position, (constructor, _)) in members.iter().enumerate() {
        let bullet = if position == 0 { '=' } else { '|' };
        rendered.push_str(&format!("\n    {bullet} {constructor}"));
        let Some(scheme) = crate::prelude::constructor_scheme(constructor) else {
            continue;
        };
        for argument in constructor_arguments(&scheme.ty) {
            rendered.push_str(&format!(" {}", render_field(argument)));
        }
    }
    Some(rendered)
}

/// The prose that accompanies `builtin_type_declaration`, for a hover to place
/// where an author's `--` block would go. Only the scroll-side builtins carry
/// one.
pub fn builtin_type_doc(name: &str) -> Option<String> {
    crate::prelude::builtin_type_documentation(name).map(|documented| documented.doc.to_string())
}

const TYPE_PARAMETER_NAMES: [&str; 2] = ["a", "b"];

fn constructor_arguments(ty: &Type) -> Vec<&Type> {
    let mut arguments = Vec::new();
    let mut remaining = ty;
    while let Type::Fun(argument, result) = remaining {
        arguments.push(argument.as_ref());
        remaining = result.as_ref();
    }
    arguments
}

fn render_field(ty: &Type) -> String {
    let rendered = crate::infer::render_type(ty);
    match ty {
        Type::Con(_, arguments) if !arguments.is_empty() => format!("({rendered})"),
        Type::Fun(_, _) => format!("({rendered})"),
        _ => rendered,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Module,
    Value,
    Function,
    Type,
    Constructor,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: Option<String>,
    pub span: Span,
    pub name_span: Span,
    pub children: Vec<Symbol>,
}

/// The document's top-level symbols in source order — the module header, its
/// types (with their constructors as children), and its declarations. A
/// declaration's detail is the type inference *recorded* at its name span, with
/// the written signature as fallback, so an unannotated declaration still shows
/// what it turned out to be.
pub fn outline(module: &Module, source: &str, index: &QueryIndex) -> Vec<Symbol> {
    let mut symbols: Vec<Symbol> = Vec::new();

    if let Some(name) = &module.name {
        if let Some(symbol) = module_header_symbol(source, name) {
            symbols.push(symbol);
        }
    }

    for declaration in &module.type_decls {
        let name_span = name_span_within(source, &declaration.span, &declaration.name);
        symbols.push(Symbol {
            name: declaration.name.clone(),
            kind: SymbolKind::Type,
            detail: type_parameters(&declaration.params),
            span: declaration.span.clone(),
            name_span,
            children: declaration.variants.iter().map(variant_symbol).collect(),
        });
    }

    for declaration in &module.decls {
        let name_span = declaration.span.start..declaration.span.start + declaration.name.len();
        let detail = index
            .type_at(declaration.span.start)
            .map(|ty| ty.to_string())
            .or_else(|| declaration.sig.as_ref().map(|sig| sig.0.to_string()));
        symbols.push(Symbol {
            name: declaration.name.clone(),
            kind: if declaration.params.is_empty() {
                SymbolKind::Value
            } else {
                SymbolKind::Function
            },
            detail,
            span: declaration.span.clone(),
            name_span,
            children: Vec::new(),
        });
    }

    symbols.sort_by_key(|symbol| symbol.span.start);
    symbols
}

fn variant_symbol(variant: &crate::ast::Variant) -> Symbol {
    let fields: Vec<String> = variant
        .fields
        .iter()
        .map(|field| field.0.to_string())
        .collect();
    Symbol {
        name: variant.name.clone(),
        kind: SymbolKind::Constructor,
        detail: if fields.is_empty() {
            None
        } else {
            Some(fields.join(" "))
        },
        span: variant.span.clone(),
        name_span: variant.span.start..variant.span.start + variant.name.len(),
        children: Vec::new(),
    }
}

fn type_parameters(params: &[String]) -> Option<String> {
    if params.is_empty() {
        None
    } else {
        Some(params.join(" "))
    }
}

fn name_span_within(source: &str, declaration: &Span, name: &str) -> Span {
    let text = source.get(declaration.clone()).unwrap_or("");
    match text.find(name) {
        Some(at) => declaration.start + at..declaration.start + at + name.len(),
        None => declaration.clone(),
    }
}

fn module_header_symbol(source: &str, name: &str) -> Option<Symbol> {
    let mut line_start = 0usize;
    loop {
        let line = line_at(source, line_start);
        if line.starts_with("module ") {
            let at = line_start + line.find(name)?;
            return Some(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Module,
                detail: None,
                span: line_start..line_start + line.len(),
                name_span: at..at + name.len(),
                children: Vec::new(),
            });
        }
        line_start += line.len() + 1;
        if line_start >= source.len() {
            return None;
        }
    }
}

/// The contiguous run of `--` lines immediately above a definition — the prose
/// hover shows under the type. A bare `--` renders as a blank line, so paragraph
/// breaks survive; a blank line ends the block.
///
/// Only a column-zero definition takes one. An indented binding — a `let`, a
/// lambda parameter, a `case` binder — would otherwise inherit the enclosing
/// declaration's prose from the line above it. The signature line is skipped
/// because `Decl::span` starts at the *binding*, leaving `name : …` between the
/// block and the span.
pub fn doc_comment_above_definition(source: &str, definition: &Span) -> Option<String> {
    let binding = line_start_before(source, definition.start);
    if binding != definition.start {
        return None;
    }
    let bound_name = leading_identifier(line_at(source, binding));
    let mut block_start = binding;
    if let Some(previous) = line_before(source, block_start) {
        if is_signature_for(line_at(source, previous), bound_name) {
            block_start = previous;
        }
    }

    let mut paragraphs: Vec<String> = Vec::new();
    while let Some(previous) = line_before(source, block_start) {
        match comment_body(line_at(source, previous)) {
            Some(body) => {
                paragraphs.push(body);
                block_start = previous;
            }
            None => break,
        }
    }
    paragraphs.reverse();

    let doc = paragraphs.join("\n");
    let doc = doc.trim_matches('\n').to_string();
    if doc.is_empty() {
        None
    } else {
        Some(doc)
    }
}

fn line_start_before(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map(|newline| newline + 1)
        .unwrap_or(0)
}

fn line_before(source: &str, line_start: usize) -> Option<usize> {
    if line_start == 0 {
        None
    } else {
        Some(line_start_before(source, line_start - 1))
    }
}

fn line_at(source: &str, line_start: usize) -> &str {
    let rest = &source[line_start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    rest[..end].trim_end_matches('\r')
}

fn leading_identifier(line: &str) -> &str {
    let end = line
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\''))
        .unwrap_or(line.len());
    &line[..end]
}

fn is_signature_for(line: &str, name: &str) -> bool {
    !name.is_empty()
        && line
            .strip_prefix(name)
            .map(|rest| rest.trim_start().starts_with(':'))
            .unwrap_or(false)
}

fn comment_body(line: &str) -> Option<String> {
    let body = line.strip_prefix("--")?;
    Some(body.strip_prefix(' ').unwrap_or(body).to_string())
}
