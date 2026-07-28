//! The position index the LSP queries (ADR 0018). Inference records, per source
//! span, the type it inferred, the scope in effect, and where each used name is
//! defined; this module turns those recordings into byte-offset lookups. The
//! index is built only when inference runs with a `Recorder` attached
//! (`analyze_module`), so `emetc` pays nothing for it.

use std::collections::HashMap;

use crate::ast::{Scheme, Span, Type};

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
