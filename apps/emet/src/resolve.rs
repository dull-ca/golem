//! The resolve / import-graph stage (ADR 0016), which runs before inference for
//! a multi-module program. It loads the entry file, follows its `import` lines
//! to the modules they name (file path = module name, resolved over the ADR 0024
//! search path — the entry directory first, then the `source-directories` of the
//! nearest `emet.json`, first match winning; see `manifest::search_path_for`),
//! rejects import cycles, orders the modules so every import precedes its
//! importer, then type-checks and evaluates each module against the *interfaces*
//! of the modules it imports.
//!
//! An [`Interface`] is the harvested public surface of an already-processed
//! library: the type env and value env plus which names it exposes. Only
//! exposed values, exposed type names (`exposed_type_arities`), and — for a
//! `Type(..)` export — exposed constructors are importable; the visibility gate
//! is what distinguishes a module's public API from its internals.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Exposed, Exposing, Import, ImportExposing, Module, Scheme, Span};
use crate::eval::{self, Env};
use crate::infer::{self, ImportedConstructors, TyEnv};
use crate::manifest::{self, SearchPath};
use crate::query::QueryIndex;
use crate::{Error, Phase};

struct Loaded {
    module: Module,
    path: PathBuf,
    source: String,
}

pub struct ProjectAnalysis {
    pub diagnostics: Vec<Error>,
    pub indexes: HashMap<PathBuf, QueryIndex>,
}

impl ProjectAnalysis {
    pub fn index_for(&self, path: &Path) -> Option<&QueryIndex> {
        self.indexes.get(path)
    }
}

/// The importable surface of a processed library module: its full type and
/// value envs, plus the gate over them — exposed value names, exposed
/// constructor names (only for `Type(..)` open exports), and exposed type
/// names with their parameter counts (`exposed_type_arities`, which
/// `import_type_arities` feeds to inference so an importer may name the type).
/// `exposed_def_spans` additionally maps each exposed name to the span of its
/// definition in *this* module's source, so an importer's go-to-definition can
/// jump across the file boundary (ADR 0018) — see `import_def_sites`.
struct Interface {
    ty_env: TyEnv,
    value_env: Env,
    exposed_values: HashSet<String>,
    exposed_constructors: Vec<String>,
    exposed_type_arities: HashMap<String, usize>,
    exposed_ctor_schemes: HashMap<String, Scheme>,
    exposed_sum_ctors: HashMap<String, Vec<(String, usize)>>,
    exposed_def_spans: HashMap<String, Span>,
}

/// Compile a multi-module program from its entry file to `main`'s type and the
/// evaluated scrolls. Loads and orders the import graph, then does the
/// type-check-and-evaluate pass on the evaluation thread: `check_and_eval`
/// keeps each module's non-`Send` value env alive across the whole pass, so the
/// work runs on the deep-stack eval thread rather than moving those envs across
/// a thread boundary (see `eval::on_eval_thread`).
///
/// The error type is `Vec<Error>` (ADR 0022): `load_graph` parses each module
/// through the recovering path, so a build reports every parse error in a bad
/// file at once. Later phases stay first-error, so the vec is either several
/// parse errors or one type/eval/analyze error.
pub fn compile_entry(
    entry: &Path,
) -> Result<(crate::ast::Type, Vec<crate::ir::Scroll>), Vec<Error>> {
    let search_path = manifest::search_path_for(entry);

    let mut loaded: HashMap<String, Loaded> = HashMap::new();
    let entry_name = load_graph(entry, &search_path, &mut loaded)?;
    let order = topo_order(&entry_name, &loaded).map_err(|e| vec![e])?;

    eval::on_eval_thread(move || check_and_eval(entry_name, order, loaded)).map_err(|e| vec![e])
}

pub fn analyze_entry(entry: &Path) -> ProjectAnalysis {
    let search_path = manifest::search_path_for(entry);

    let mut loaded: HashMap<String, Loaded> = HashMap::new();
    let entry_name = match load_graph(entry, &search_path, &mut loaded) {
        Ok(name) => name,
        Err(errors) => {
            return ProjectAnalysis {
                diagnostics: errors,
                indexes: HashMap::new(),
            }
        }
    };
    let order = match topo_order(&entry_name, &loaded) {
        Ok(order) => order,
        Err(e) => {
            return ProjectAnalysis {
                diagnostics: vec![e],
                indexes: HashMap::new(),
            }
        }
    };

    let mut interfaces: HashMap<String, Interface> = HashMap::new();
    let mut diagnostics: Vec<Error> = Vec::new();
    let mut indexes: HashMap<PathBuf, QueryIndex> = HashMap::new();

    for name in &order {
        let loaded_mod = &loaded[name];

        let base_ty = match import_ty_env(&loaded_mod.module, &interfaces, loaded_mod) {
            Ok(env) => env,
            Err(e) => {
                diagnostics.push(e);
                continue;
            }
        };
        let imported_types = import_type_arities(&loaded_mod.module, &interfaces);
        let imported_ctors = import_constructors(&loaded_mod.module, &interfaces);
        let imported_defs = import_def_sites(&loaded_mod.module, &interfaces);

        let (error, index) = infer::analyze_module(
            &loaded_mod.module,
            base_ty.clone(),
            &imported_types,
            &imported_ctors,
            imported_defs,
            0..loaded_mod.source.len(),
        );
        if let Some(e) = error {
            diagnostics.push(type_error(loaded_mod, e));
        }
        indexes.insert(loaded_mod.path.clone(), index);

        if name != &entry_name {
            if let Ok(final_ty) = infer::check_library(
                &loaded_mod.module,
                base_ty,
                &imported_types,
                &imported_ctors,
            ) {
                let iface = interface_of(&loaded_mod.module, final_ty, eval::prelude_env());
                interfaces.insert(name.clone(), iface);
            }
        }
    }

    ProjectAnalysis {
        diagnostics,
        indexes,
    }
}

fn check_and_eval(
    entry_name: String,
    order: Vec<String>,
    loaded: HashMap<String, Loaded>,
) -> Result<(crate::ast::Type, Vec<crate::ir::Scroll>), Error> {
    let mut interfaces: HashMap<String, Interface> = HashMap::new();
    let mut entry_result: Option<(crate::ast::Type, Vec<crate::ir::Scroll>)> = None;

    for name in &order {
        let loaded_mod = &loaded[name];
        let is_entry = name == &entry_name;

        let base_ty = import_ty_env(&loaded_mod.module, &interfaces, loaded_mod)?;
        let base_val = import_value_env(&loaded_mod.module, &interfaces);
        let imported_types = import_type_arities(&loaded_mod.module, &interfaces);
        let imported_ctors = import_constructors(&loaded_mod.module, &interfaces);

        if is_entry {
            let (_, main_ty) = infer::check_entry(
                &loaded_mod.module,
                base_ty,
                &imported_types,
                &imported_ctors,
            )
            .map_err(|e| type_error(loaded_mod, e))?;
            let scrolls = eval::eval_entry(&loaded_mod.module, base_val)
                .map_err(|e| analyze_error(loaded_mod, e))?;
            crate::analyze(&scrolls).map_err(|msg| Error {
                phase: Phase::Analyze,
                msg,
                span: 0..0,
                note: None,
                file: Some(loaded_mod.path.clone()),
            })?;
            entry_result = Some((main_ty, scrolls));
        } else {
            reject_library_main(loaded_mod)?;
            let final_ty = infer::check_library(
                &loaded_mod.module,
                base_ty,
                &imported_types,
                &imported_ctors,
            )
            .map_err(|e| type_error(loaded_mod, e))?;
            let final_val = eval::eval_library(&loaded_mod.module, base_val)
                .map_err(|e| analyze_error(loaded_mod, e))?;
            interfaces.insert(
                name.clone(),
                interface_of(&loaded_mod.module, final_ty, final_val),
            );
        }
    }

    entry_result.ok_or_else(|| Error {
        phase: Phase::Type,
        msg: "no entry module produced".to_string(),
        span: 0..0,
        note: None,
        file: None,
    })
}

/// Parse the module at `path` and recursively load every module it imports,
/// keyed by module name in `loaded`. An import resolves to the first
/// `<Name>.emet` found along `search_path` (ADR 0024, `find_module`); a name
/// found in no search directory is a parse-phase error naming every directory
/// tried. Returns this module's name.
///
/// Parses through `parse_source_multi`, so a malformed module yields every parse
/// error it contains as the `Vec<Error>` (ADR 0022), rather than only the first.
/// The errors carry no file-path prefix — the report header already names the
/// file, and prefixing it into the message wrecked the ariadne layout (ADR 0032 §1).
fn load_graph(
    path: &Path,
    search_path: &SearchPath,
    loaded: &mut HashMap<String, Loaded>,
) -> Result<String, Vec<Error>> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        vec![Error {
            phase: Phase::Parse,
            msg: format!("cannot read {}: {e}", path.display()),
            span: 0..0,
            note: None,
            file: Some(path.to_path_buf()),
        }]
    })?;
    let module = crate::parse_source_multi(&source).map_err(|mut errors| {
        for error in &mut errors {
            error.file = Some(path.to_path_buf());
        }
        errors
    })?;
    let name = module
        .name
        .clone()
        .unwrap_or_else(|| module_name_from_path(path));

    let imports = module.imports.clone();
    loaded.insert(
        name.clone(),
        Loaded {
            module,
            path: path.to_path_buf(),
            source,
        },
    );

    for import in &imports {
        if loaded.contains_key(&import.module) {
            continue;
        }
        let import_path = find_module(&import.module, search_path).ok_or_else(|| {
            vec![Error {
                phase: Phase::Parse,
                msg: missing_module_message(&import.module, search_path),
                span: import.span.clone(),
                note: None,
                file: Some(path.to_path_buf()),
            }]
        })?;
        load_graph(&import_path, search_path, loaded)?;
    }
    Ok(name)
}

fn find_module(module: &str, search_path: &SearchPath) -> Option<PathBuf> {
    for dir in search_path.directories() {
        let candidate = dir.join(format!("{module}.emet"));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn missing_module_message(module: &str, search_path: &SearchPath) -> String {
    let searched = search_path
        .directories()
        .iter()
        .map(|dir| dir.join(format!("{module}.emet")).display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("cannot find imported module `{module}` (searched {searched})")
}

fn module_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Main")
        .to_string()
}

/// Order the loaded modules so every import comes before its importer, by a
/// depth-first post-order walk from the entry. A back edge (a module reappearing
/// on the active `stack`) is an import cycle and an error — Elm forbids them and
/// so does emet (ADR 0016).
fn topo_order(entry: &str, loaded: &HashMap<String, Loaded>) -> Result<Vec<String>, Error> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    visit(entry, loaded, &mut visited, &mut stack, &mut order)?;
    Ok(order)
}

fn visit(
    name: &str,
    loaded: &HashMap<String, Loaded>,
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
    order: &mut Vec<String>,
) -> Result<(), Error> {
    if visited.contains(name) {
        return Ok(());
    }
    if stack.iter().any(|m| m == name) {
        let mut chain: Vec<String> = stack.clone();
        chain.push(name.to_string());
        let start = chain.iter().position(|m| m == name).unwrap();
        let cycle = chain[start..].join(" -> ");
        return Err(Error {
            phase: Phase::Parse,
            msg: format!("import cycle detected: {cycle}"),
            span: 0..0,
            note: None,
            file: None,
        });
    }
    stack.push(name.to_string());
    for import in &loaded[name].module.imports {
        visit(&import.module, loaded, visited, stack, order)?;
    }
    stack.pop();
    visited.insert(name.to_string());
    order.push(name.to_string());
    Ok(())
}

fn import_ty_env(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
    site: &Loaded,
) -> Result<TyEnv, Error> {
    let mut env = crate::prelude::ty_env();
    for import in &module.imports {
        let iface = &interfaces[&import.module];
        let qualifier = import
            .alias
            .clone()
            .unwrap_or_else(|| import.module.clone());
        for value in &iface.exposed_values {
            if let Some(scheme) = iface.ty_env.scheme(value) {
                env = env.bind(format!("{qualifier}.{value}"), scheme);
            }
        }
        for ctor in &iface.exposed_constructors {
            if let Some(scheme) = iface.ty_env.scheme(ctor) {
                env = env.bind(ctor.clone(), scheme);
            }
        }
        bind_import_exposing_ty(&mut env, import, iface, site)?;
    }
    Ok(env)
}

fn bind_import_exposing_ty(
    env: &mut TyEnv,
    import: &Import,
    iface: &Interface,
    site: &Loaded,
) -> Result<(), Error> {
    if let ImportExposing::Explicit(items) = &import.exposing {
        for item in items {
            match item {
                Exposed::Value(name) => {
                    if !iface.exposed_values.contains(name) {
                        return Err(not_exposed(site, import, name));
                    }
                    if let Some(scheme) = iface.ty_env.scheme(name) {
                        *env = env.clone().bind(name.clone(), scheme);
                    }
                }
                Exposed::Type { name, .. } => {
                    if let Some(scheme) = iface.ty_env.scheme(name) {
                        *env = env.clone().bind(name.clone(), scheme);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Collect the arities of types this module imports via `exposing (Type)`, so
/// inference can validate signatures that mention an imported type name (fed to
/// `check_entry`/`check_library` as `imported_types`). A type is importable only
/// if the exporting module exposed it.
fn import_type_arities(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> HashMap<String, usize> {
    let mut arities = HashMap::new();
    for import in &module.imports {
        let iface = &interfaces[&import.module];
        if let ImportExposing::Explicit(items) = &import.exposing {
            for item in items {
                if let Exposed::Type { name, .. } = item {
                    if let Some(arity) = iface.exposed_type_arities.get(name) {
                        arities.insert(name.clone(), *arity);
                    }
                }
            }
        }
    }
    arities
}

/// Collect the constructor schemes and full variant sets of the open-exposed
/// types this module imports, mirroring `import_type_arities` but for the
/// pattern side. The exporting module's `exposed_ctor_schemes` /
/// `exposed_sum_ctors` already hold only `Type(..)` constructors, so a type
/// exposed without `(..)` contributes nothing here and stays unmatchable in the
/// importer. Fed to inference so `infer_pattern` resolves imported constructors
/// and the exhaustiveness checker sees their type's complete signature.
fn import_constructors(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> ImportedConstructors {
    let mut ctor_schemes = HashMap::new();
    let mut sum_ctors = HashMap::new();
    for import in &module.imports {
        let iface = &interfaces[&import.module];
        for (name, scheme) in &iface.exposed_ctor_schemes {
            ctor_schemes.insert(name.clone(), scheme.clone());
        }
        for (name, members) in &iface.exposed_sum_ctors {
            sum_ctors.insert(name.clone(), members.clone());
        }
    }
    ImportedConstructors {
        ctor_schemes,
        sum_ctors,
    }
}

/// Resolve each name this module imports to the `DefSite` in its owning module
/// (ADR 0018): the definition's span from the exporter's `exposed_def_spans`,
/// tagged with the owning module so the LSP adapter can open the right file. The
/// key mirrors how the name is used in this module — a qualified `Qual.value`
/// for plain imports (honoring `as` aliases), the bare name for open-exposed
/// constructors and `exposing`-listed items — so `definition_at` finds it at the
/// use site. Fed into inference as `imported_defs`, the cross-file half of
/// go-to-definition; the same-file half is `decl_def_sites`.
fn import_def_sites(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> HashMap<String, crate::query::DefSite> {
    let mut defs = HashMap::new();
    for import in &module.imports {
        let iface = &interfaces[&import.module];
        let owner = import.module.clone();
        let qualifier = import
            .alias
            .clone()
            .unwrap_or_else(|| import.module.clone());
        for value in &iface.exposed_values {
            if let Some(span) = iface.exposed_def_spans.get(value) {
                defs.insert(
                    format!("{qualifier}.{value}"),
                    crate::query::DefSite {
                        span: span.clone(),
                        module: Some(owner.clone()),
                    },
                );
            }
        }
        for ctor in &iface.exposed_constructors {
            if let Some(span) = iface.exposed_def_spans.get(ctor) {
                defs.insert(
                    ctor.clone(),
                    crate::query::DefSite {
                        span: span.clone(),
                        module: Some(owner.clone()),
                    },
                );
            }
        }
        if let ImportExposing::Explicit(items) = &import.exposing {
            for item in items {
                let name = match item {
                    Exposed::Value(name) => name,
                    Exposed::Type { name, .. } => name,
                };
                if let Some(span) = iface.exposed_def_spans.get(name) {
                    defs.insert(
                        name.clone(),
                        crate::query::DefSite {
                            span: span.clone(),
                            module: Some(owner.clone()),
                        },
                    );
                }
            }
        }
    }
    defs
}

fn import_value_env(module: &Module, interfaces: &HashMap<String, Interface>) -> Env {
    let mut env = eval::prelude_env();
    for import in &module.imports {
        let iface = &interfaces[&import.module];
        let qualifier = import
            .alias
            .clone()
            .unwrap_or_else(|| import.module.clone());
        for value in &iface.exposed_values {
            if let Some(v) = iface.value_env.lookup(value) {
                env = env.insert(format!("{qualifier}.{value}"), v);
            }
        }
        for ctor in &iface.exposed_constructors {
            if let Some(v) = iface.value_env.lookup(ctor) {
                env = env.insert(ctor.clone(), v);
            }
        }
        if let ImportExposing::Explicit(items) = &import.exposing {
            for item in items {
                if let Exposed::Value(name) = item {
                    if let Some(v) = iface.value_env.lookup(name) {
                        env = env.insert(name.clone(), v);
                    }
                }
            }
        }
    }
    env
}

/// Harvest a processed module's importable [`Interface`] from its `exposing`
/// list. `exposing (..)` exposes every value decl, every type, and every
/// constructor; an explicit list exposes only the named values and types, with
/// a type's constructors exposed only when written `Type(..)`. The visibility
/// rule is enforced here: a name absent from the exposing list never enters the
/// interface and so cannot be imported.
///
/// A type exposed open (`Type(..)`) additionally carries, per constructor, its
/// value scheme (`exposed_ctor_schemes`, harvested from the module's own
/// `ty_env`) and, per type, its full variant set (`exposed_sum_ctors`), so an
/// importer can both build and pattern-match its values and have the
/// exhaustiveness checker see the complete constructor set. A type exposed
/// without `(..)` contributes none of this, so its constructors stay invisible
/// to the importer.
fn interface_of(module: &Module, ty_env: TyEnv, value_env: Env) -> Interface {
    let mut exposed_values = HashSet::new();
    let mut exposed_constructors = Vec::new();
    let mut exposed_type_arities = HashMap::new();
    let mut exposed_ctor_schemes = HashMap::new();
    let mut exposed_sum_ctors = HashMap::new();

    let ctor_names: BTreeMap<String, Vec<String>> = module
        .type_decls
        .iter()
        .map(|td| {
            (
                td.name.clone(),
                td.variants.iter().map(|v| v.name.clone()).collect(),
            )
        })
        .collect();
    let sum_ctors: BTreeMap<String, Vec<(String, usize)>> = module
        .type_decls
        .iter()
        .map(|td| {
            (
                td.name.clone(),
                td.variants
                    .iter()
                    .map(|v| (v.name.clone(), v.fields.len()))
                    .collect(),
            )
        })
        .collect();
    let type_arities: BTreeMap<String, usize> = module
        .type_decls
        .iter()
        .map(|td| (td.name.clone(), td.params.len()))
        .collect();

    let mut open_type_names: Vec<String> = Vec::new();
    match &module.exposing {
        Exposing::All => {
            for decl in &module.decls {
                exposed_values.insert(decl.name.clone());
            }
            open_type_names.extend(ctor_names.keys().cloned());
            for (name, arity) in &type_arities {
                exposed_type_arities.insert(name.clone(), *arity);
            }
        }
        Exposing::Explicit(items) => {
            for item in items {
                match item {
                    Exposed::Value(name) => {
                        exposed_values.insert(name.clone());
                    }
                    Exposed::Type { name, open } => {
                        if let Some(arity) = type_arities.get(name) {
                            exposed_type_arities.insert(name.clone(), *arity);
                        }
                        if *open {
                            open_type_names.push(name.clone());
                        }
                    }
                }
            }
        }
    }

    for type_name in &open_type_names {
        if let Some(variants) = ctor_names.get(type_name) {
            exposed_constructors.extend(variants.iter().cloned());
            for ctor in variants {
                if let Some(scheme) = ty_env.scheme(ctor) {
                    exposed_ctor_schemes.insert(ctor.clone(), scheme);
                }
            }
        }
        if let Some(members) = sum_ctors.get(type_name) {
            exposed_sum_ctors.insert(type_name.clone(), members.clone());
        }
    }

    let mut exposed_def_spans: HashMap<String, Span> = HashMap::new();
    for decl in &module.decls {
        if exposed_values.contains(&decl.name) {
            let name_span = decl.span.start..decl.span.start + decl.name.len();
            exposed_def_spans.insert(decl.name.clone(), name_span);
        }
    }
    for td in &module.type_decls {
        if exposed_type_arities.contains_key(&td.name) {
            let name_span = td.span.start..td.span.start + td.name.len();
            exposed_def_spans.insert(td.name.clone(), name_span);
        }
        for variant in &td.variants {
            if exposed_constructors.contains(&variant.name) {
                let name_span = variant.span.start..variant.span.start + variant.name.len();
                exposed_def_spans.insert(variant.name.clone(), name_span);
            }
        }
    }

    Interface {
        ty_env,
        value_env,
        exposed_values,
        exposed_constructors,
        exposed_type_arities,
        exposed_ctor_schemes,
        exposed_sum_ctors,
        exposed_def_spans,
    }
}

fn reject_library_main(loaded: &Loaded) -> Result<(), Error> {
    if loaded.module.decls.iter().any(|d| d.name == "main") {
        return Err(Error {
            phase: Phase::Type,
            msg: format!(
                "library module `{}` declares `main`; only the entry module may (ADR 0009)",
                loaded
                    .module
                    .name
                    .clone()
                    .unwrap_or_else(|| module_name_from_path(&loaded.path))
            ),
            span: 0..0,
            note: None,
            file: Some(loaded.path.clone()),
        });
    }
    Ok(())
}

fn not_exposed(site: &Loaded, import: &Import, name: &str) -> Error {
    Error {
        phase: Phase::Type,
        msg: format!("module `{}` does not expose `{name}`", import.module),
        span: import.span.clone(),
        note: None,
        file: Some(site.path.clone()),
    }
}

fn type_error(loaded: &Loaded, e: infer::TypeError) -> Error {
    Error {
        phase: Phase::Type,
        msg: e.msg,
        span: e.span,
        note: e.note,
        file: Some(loaded.path.clone()),
    }
}

fn analyze_error(loaded: &Loaded, e: eval::EvalError) -> Error {
    Error {
        phase: Phase::Analyze,
        msg: e.msg,
        span: 0..0,
        note: None,
        file: Some(loaded.path.clone()),
    }
}
