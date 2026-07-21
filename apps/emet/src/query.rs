use std::collections::HashMap;

use crate::ast::{Scheme, Span, Type};

pub type ScopeId = usize;

#[derive(Debug, Clone)]
pub struct DefSite {
    pub span: Span,
    pub module: Option<String>,
}

#[derive(Default)]
pub struct QueryIndex {
    pub types: Vec<(Span, Type)>,
    pub scopes: Vec<(Span, ScopeId)>,
    pub scope_table: HashMap<ScopeId, Vec<(String, Scheme)>>,
    pub defs: Vec<(Span, DefSite)>,
}

impl QueryIndex {
    pub fn type_at(&self, offset: usize) -> Option<&Type> {
        self.types
            .iter()
            .filter(|(span, _)| span.contains(&offset) || span.start == offset)
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
            .map(|(_, ty)| ty)
    }

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
