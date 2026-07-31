//! The position index the LSP queries (ADR 0018). Inference records, per source
//! span, the type it inferred, the scope in effect, and where each used name is
//! defined; this module turns those recordings into byte-offset lookups. The
//! index is built only when inference runs with a `Recorder` attached
//! (`analyze_module`), so `emetc` pays nothing for it.

use std::collections::HashMap;

use crate::ast::{Module, Scheme, Span, Type};

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
