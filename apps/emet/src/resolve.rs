//! The resolve / import-graph stage (ADR 0016), which runs before inference for
//! a multi-module program. It loads the entry file, follows its `import` lines
//! to the modules they name (file path = module name, resolved over the ADR 0024
//! search path — the entry directory first, then the `source-directories` of the
//! nearest `emet.json`, first match winning; see `manifest::search_path_for`),
//! rejects import cycles, orders the modules so every import precedes its
//! importer, then type-checks and evaluates each module against the *interfaces*
//! of the modules it imports.
//!
//! An `Interface` (crate-private) is the harvested public surface of an
//! already-processed library: the type env and value env plus which names it
//! exposes. Only exposed values, exposed type names (`exposed_type_arities`),
//! and — for a `Type(..)` export — exposed constructors are importable; the
//! visibility gate is what distinguishes a module's public API from its
//! internals. What may pass that gate is bounded on the writing side too:
//! `reject_undeclared_exposures` holds an `exposing` list to the module's own
//! declarations (ADR 0049), so a module's surface is its own and never a relay
//! for another's.
//!
//! A type is identified by its declaring module (ADR 0049): `qualify_module_types`
//! rewrites each module's declarations and signatures from the bare names an
//! author writes to `Owner.Bare`, so two modules' `Thing` are two types and
//! neither is accepted where the other is proved. That retires ADR 0045's
//! one-owner-per-type rule, which rejected the ambiguity because identity could
//! not carry it. What survives is narrower and lives in `qualify_type`: a *bare
//! reference* with two candidates in scope still has no single meaning, and is
//! an error naming both.
//!
//! A constructor is identified the same way (ADR 0051), by
//! `qualify_module_constructors` over the same cloned AST: a variant declared in
//! `M` is `M.Ctor`, and every `Expr::Ctor` and `Pattern::Ctor` in the module is
//! rewritten from what the author wrote to the identity it means, through
//! `ConstructorScope`. Bare stays the ordinary spelling; `M.Ctor` is what an
//! author writes when two are in scope. That retires ADR 0046's
//! one-owner-per-constructor rule, which rejected at the `import` because no use
//! site could disambiguate — the grammar now has the spelling that rule said did
//! not exist, so the rejection moved to the *bare reference* with two candidates,
//! exactly as ADR 0049 moved ADR 0045's.
//!
//! Both passes run on a clone, and inference *and* evaluation read that clone.
//! Constructor identity has to reach `eval` because a `Value::Data` tag is
//! matched against the name in a `Pattern::Ctor`; leaving eval on the unqualified
//! AST would compare `M.Ctor` against `Ctor` and match nothing. Only the query
//! index and the LSP's type rendering read the module as written, since those
//! report source back to a reader.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{
    Decl, Exposed, Exposing, Expr, Import, ImportExposing, Module, Pattern, Scheme, Span, Spanned,
};
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
/// `type_owners` maps every type name this module's exposed surface can put in
/// front of an importer to the module that *declared* it, which is what makes
/// ADR 0045's one-owner rule checkable — see `interface_of`.
///
/// NOTE: everything constructor-shaped here is keyed by identity, `Owner.Ctor`
/// (ADR 0051), because `interface_of` harvests a module *after*
/// `qualify_module_constructors` has rewritten it. The constructor namespace
/// needs no `exposed_type_identity` counterpart: a constructor cannot be
/// re-exposed, so its owner is always this module and the bare tail is
/// recoverable by splitting at the last dot.
struct Interface {
    ty_env: TyEnv,
    value_env: Env,
    exposed_values: HashSet<String>,
    exposed_constructors: Vec<String>,
    exposed_type_arities: HashMap<String, usize>,
    /// Each exposed type's bare name mapped to the identity it actually carries,
    /// `Owner.Bare` (ADR 0049). The bare name is what an author writes in an
    /// `exposing` list and an annotation; the identity is what unifies.
    exposed_type_identity: HashMap<String, String>,
    exposed_ctor_schemes: HashMap<String, Scheme>,
    exposed_sum_ctors: HashMap<String, Vec<(String, usize)>>,
    exposed_def_spans: HashMap<String, Span>,
    type_owners: BTreeMap<String, String>,
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
    secrets: crate::secrets::SecretOptions,
) -> Result<(crate::ast::Type, Vec<crate::ir::Scroll>), Vec<Error>> {
    let search_path = manifest::search_path_for(entry);

    let mut loaded: HashMap<String, Loaded> = HashMap::new();
    let entry_name = load_graph(entry, &search_path, &mut loaded)?;
    let order = topo_order(&entry_name, &loaded).map_err(|e| vec![e])?;

    let entry = entry.to_path_buf();
    eval::on_eval_thread(move || {
        crate::secrets::with_session(&entry, secrets, || {
            check_and_eval(entry_name, order, loaded)
        })
    })
    .map_err(|e| vec![e])
}

pub fn analyze_entry(entry: &Path) -> ProjectAnalysis {
    match read_source(entry) {
        Ok(source) => analyze_entry_source(entry, source),
        Err(errors) => ProjectAnalysis {
            diagnostics: errors,
            indexes: HashMap::new(),
        },
    }
}

/// `analyze_entry` for an entry whose text the caller already holds — an unsaved
/// editor buffer. Only the entry is overlaid; every imported module is still read
/// from disk, so another dirty buffer in the same project is analyzed as last
/// saved.
///
/// Unlike the compile path this keeps going after a module fails, so the editor
/// still gets an index for the file in front of the reader.
pub fn analyze_entry_source(entry: &Path, source: String) -> ProjectAnalysis {
    let search_path = manifest::search_path_for(entry);

    let mut loaded: HashMap<String, Loaded> = HashMap::new();
    let entry_name = match load_module(entry, source, &search_path, &mut loaded) {
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

        // NOTE: no `continue` — an undeclared exposure says nothing about how this
        // module's own bodies infer, so the pass goes on and the editor still gets
        // an index for the file in front of the reader.
        if let Err(e) = reject_undeclared_exposures(loaded_mod, name) {
            diagnostics.push(e);
        }
        let aliases = type_aliases(&loaded_mod.module, name, &interfaces);
        let constructors = ConstructorScope::of(&loaded_mod.module, name, &interfaces);
        let mut qualified = loaded_mod.module.clone();
        if let Err(mut e) = qualify_module_types(&mut qualified, name, &aliases)
            .and_then(|()| qualify_module_constructors(&mut qualified, name, &constructors))
        {
            e.file = Some(loaded_mod.path.clone());
            diagnostics.push(e);
            continue;
        }
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

        let (error, mut index) = infer::analyze_module(
            &qualified,
            base_ty.clone(),
            &imported_types,
            &imported_ctors,
            imported_defs.clone(),
            0..loaded_mod.source.len(),
        );
        index.type_definitions = type_definitions(&loaded_mod.module, &loaded, &interfaces);
        crate::query::record_exposing_sites(
            &mut index,
            &loaded_mod.module,
            &loaded_mod.source,
            &base_ty,
            &imported_defs,
        );
        if let Some(e) = error {
            diagnostics.push(type_error(loaded_mod, e));
        }
        indexes.insert(loaded_mod.path.clone(), index);

        if name != &entry_name {
            if let Ok(final_ty) =
                infer::check_library(&qualified, base_ty, &imported_types, &imported_ctors)
            {
                let iface = interface_of(
                    &qualified,
                    name,
                    &inherited_type_owners(&loaded_mod.module, &interfaces),
                    final_ty,
                    eval::prelude_env(),
                );
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

        reject_undeclared_exposures(loaded_mod, name)?;
        let aliases = type_aliases(&loaded_mod.module, name, &interfaces);
        let constructors = ConstructorScope::of(&loaded_mod.module, name, &interfaces);
        let mut qualified = loaded_mod.module.clone();
        qualify_module_types(&mut qualified, name, &aliases)
            .and_then(|()| qualify_module_constructors(&mut qualified, name, &constructors))
            .map_err(|mut e| {
                e.file = Some(loaded_mod.path.clone());
                e
            })?;
        let base_ty = import_ty_env(&loaded_mod.module, &interfaces, loaded_mod)?;
        let base_val = import_value_env(&loaded_mod.module, &interfaces);
        let imported_types = import_type_arities(&loaded_mod.module, &interfaces);
        let imported_ctors = import_constructors(&loaded_mod.module, &interfaces);

        if is_entry {
            let (_, main_ty) =
                infer::check_entry(&qualified, base_ty, &imported_types, &imported_ctors)
                    .map_err(|e| type_error(loaded_mod, e))?;
            let (scrolls, glyph_spans) =
                eval::eval_entry(&qualified, base_val).map_err(|e| analyze_error(loaded_mod, e))?;
            crate::analyze(&scrolls, &glyph_spans).map_err(|mut e| {
                e.file = Some(loaded_mod.path.clone());
                e
            })?;
            entry_result = Some((main_ty, scrolls));
        } else {
            reject_library_main(loaded_mod)?;
            let final_ty =
                infer::check_library(&qualified, base_ty, &imported_types, &imported_ctors)
                    .map_err(|e| type_error(loaded_mod, e))?;
            let final_val = eval::eval_library(&qualified, base_val)
                .map_err(|e| analyze_error(loaded_mod, e))?;
            let owners = inherited_type_owners(&loaded_mod.module, &interfaces);
            interfaces.insert(
                name.clone(),
                interface_of(&qualified, name, &owners, final_ty, final_val),
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
    let source = read_source(path)?;
    load_module(path, source, search_path, loaded)
}

fn read_source(path: &Path) -> Result<String, Vec<Error>> {
    std::fs::read_to_string(path).map_err(|e| {
        vec![Error {
            phase: Phase::Parse,
            msg: format!("cannot read {}: {e}", path.display()),
            span: 0..0,
            note: None,
            file: Some(path.to_path_buf()),
        }]
    })
}

/// `load_graph` minus the read: parse this module, record it, then walk its
/// imports over the search path. Split out so a caller holding the source — the
/// LSP, with a buffer — can enter the graph without the file on disk.
fn load_module(
    path: &Path,
    source: String,
    search_path: &SearchPath,
    loaded: &mut HashMap<String, Loaded>,
) -> Result<String, Vec<Error>> {
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
        let declared = load_graph(&import_path, search_path, loaded)?;
        if declared != import.module {
            return Err(vec![Error {
                phase: Phase::Parse,
                msg: format!(
                    "`{}` resolves to {}, which declares `module {declared}`",
                    import.module,
                    import_path.display()
                ),
                span: import.span.clone(),
                note: Some(format!(
                    "a module's header has to agree with where its file sits — rename the \
                     header to `module {}`, or import `{declared}` from where that module's \
                     name says it lives",
                    import.module
                )),
                file: Some(path.to_path_buf()),
            }]);
        }
    }
    Ok(name)
}

/// A module name's file, relative to a search root: each dot is a directory
/// separator, so `Limesurvey.Database` is `Limesurvey/Database.emet` (ADR 0049).
/// An undotted name keeps its existing meaning, a file in the root itself.
fn module_relative_path(module: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in module.split('.') {
        path.push(segment);
    }
    path.set_extension("emet");
    path
}

fn find_module(module: &str, search_path: &SearchPath) -> Option<PathBuf> {
    let relative = module_relative_path(module);
    for dir in search_path.directories() {
        let candidate = dir.join(&relative);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn missing_module_message(module: &str, search_path: &SearchPath) -> String {
    let relative = module_relative_path(module);
    let searched = search_path
        .directories()
        .iter()
        .map(|dir| dir.join(&relative).display().to_string())
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
        // NOTE: an imported module has no interface when it failed to
        // type-check and `analyze_entry_source` carried on past it — so every
        // interface lookup in this file skips a missing one rather than indexing
        // the map. Indexing here aborted the LSP process on the first broken
        // library. The compile path never reaches it: `check_and_eval` returns
        // on the first library error.
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
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
                Exposed::Value { name, .. } => {
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

/// Every type name this module can write, rendered for hover: its own
/// declarations plus the ones it imports through `exposing`. An imported type
/// shows its constructors only when this module wrote `(..)` *and* the exporter
/// exposed them — the same visibility the pattern side enforces, so hover never
/// advertises a constructor a `case` here could not use. Rendering reads the
/// exporter's own `TypeDecl`, which is why the loaded modules are passed in
/// alongside their interfaces.
fn type_definitions(
    module: &Module,
    loaded: &HashMap<String, Loaded>,
    interfaces: &HashMap<String, Interface>,
) -> HashMap<String, crate::query::TypeDefinition> {
    let mut definitions = crate::query::local_type_definitions(module);
    for import in &module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        let Some(exporter) = loaded.get(&import.module) else {
            continue;
        };
        let ImportExposing::Explicit(items) = &import.exposing else {
            continue;
        };
        for item in items {
            let Exposed::Type { name, open, .. } = item else {
                continue;
            };
            if !iface.exposed_type_arities.contains_key(name) {
                continue;
            }
            let Some(declaration) = exporter
                .module
                .type_decls
                .iter()
                .find(|decl| &decl.name == name)
            else {
                continue;
            };
            let constructors_visible = *open
                && iface
                    .exposed_type_identity
                    .get(name)
                    .is_some_and(|identity| iface.exposed_sum_ctors.contains_key(identity));
            definitions.insert(
                name.clone(),
                crate::query::TypeDefinition {
                    declaration: crate::query::render_type_declaration(
                        declaration,
                        constructors_visible,
                    ),
                    site: crate::query::DefSite {
                        span: declaration.span.clone(),
                        module: Some(import.module.clone()),
                    },
                },
            );
        }
    }
    definitions
}

fn import_type_arities(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> HashMap<String, usize> {
    let mut arities = HashMap::new();
    for import in &module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        for (bare, identity) in &iface.exposed_type_identity {
            if let Some(arity) = iface.exposed_type_arities.get(bare) {
                arities.insert(identity.clone(), *arity);
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
///
/// NOTE: both maps are keyed by identity and inserted per import in order, so
/// the last import wins — and nothing can be lost to it, because an identity
/// names its own owner. `sum_ctors` is keyed by a type identity (ADR 0049),
/// `ctor_schemes` by a constructor identity (ADR 0051), so a key arriving twice
/// arrived from one module imported twice and the overwrite rewrites an entry
/// with itself. When these keys were bare names, two imports' `Wrap` collapsed to
/// one here and the earlier one vanished without a word — the defect ADR 0046
/// rejected programs to avoid and ADR 0051 fixed.
fn import_constructors(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> ImportedConstructors {
    let mut ctor_schemes = HashMap::new();
    let mut sum_ctors = HashMap::new();
    for import in &module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
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

/// The type-name ownership a module takes on from its imports. Each name keeps
/// the module that declared it, never the one it arrived through, so a type
/// re-exposed down a chain of modules stays one type all the way along.
///
/// NOTE: called only after `reject_type_name_collisions` has passed, so the
/// last-write-wins insert cannot lose an owner — a name arriving twice here
/// arrives under one owner.
fn inherited_type_owners(
    module: &Module,
    interfaces: &HashMap<String, Interface>,
) -> BTreeMap<String, String> {
    let mut owners = BTreeMap::new();
    for import in &module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        for (type_name, owner) in &iface.type_owners {
            owners.insert(type_name.clone(), owner.clone());
        }
    }
    owners
}

/// Every bare type name this module can write, mapped to the identities it could
/// mean: its own declarations, then everything its imports expose (ADR 0049).
/// A name with two candidates has no single meaning, and referencing it is the
/// error ADR 0045 used to raise at import time.
fn type_aliases(
    module: &Module,
    module_name: &str,
    interfaces: &HashMap<String, Interface>,
) -> BTreeMap<String, Vec<String>> {
    let mut aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for td in &module.type_decls {
        aliases
            .entry(td.name.clone())
            .or_default()
            .push(qualified_type_name(module_name, &td.name));
    }
    for import in &module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        for (bare, identity) in &iface.exposed_type_identity {
            let slot = aliases.entry(bare.clone()).or_default();
            if !slot.contains(identity) {
                slot.push(identity.clone());
            }
        }
    }
    aliases
}

fn qualified_type_name(module_name: &str, bare: &str) -> String {
    format!("{module_name}.{bare}")
}

/// The bare tail of an identity — `Split.Database.Config` is `Config`. Splitting
/// at the last dot is enough because an identity is exactly `Owner.Bare` and a
/// bare type or constructor name never contains one.
fn bare_name(identity: &str) -> &str {
    identity.rsplit('.').next().unwrap_or(identity)
}

fn qualified_constructor_name(module_name: &str, bare: &str) -> String {
    format!("{module_name}.{bare}")
}

/// Everything one module can mean by a constructor name, in the two spellings a
/// use site may write it (ADR 0051).
///
/// `aliases` answers the bare spelling: each bare name mapped to the identities
/// it could stand for — this module's own variants first, then everything its
/// imports open-expose. One candidate resolves; two is the ambiguity ADR 0046
/// used to reject at the `import`. `modules_by_qualifier` answers the dotted
/// spelling, mapping what an author may write in front of the dot — an `as`
/// alias, an import's own name, or this module's — to the module that owns the
/// constructor. `identities` is every constructor actually in scope, which is
/// what tells `CtorA.Wrup` from `CtorA.Wrap`.
struct ConstructorScope {
    aliases: BTreeMap<String, Vec<String>>,
    modules_by_qualifier: BTreeMap<String, String>,
    identities: BTreeSet<String>,
}

impl ConstructorScope {
    fn of(
        module: &Module,
        module_name: &str,
        interfaces: &HashMap<String, Interface>,
    ) -> ConstructorScope {
        let mut aliases: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut identities: BTreeSet<String> = BTreeSet::new();
        let mut modules_by_qualifier: BTreeMap<String, String> = BTreeMap::new();
        modules_by_qualifier.insert(module_name.to_string(), module_name.to_string());

        for td in &module.type_decls {
            for variant in &td.variants {
                let identity = qualified_constructor_name(module_name, &variant.name);
                aliases
                    .entry(variant.name.clone())
                    .or_default()
                    .push(identity.clone());
                identities.insert(identity);
            }
        }
        for import in &module.imports {
            modules_by_qualifier.insert(
                import
                    .alias
                    .clone()
                    .unwrap_or_else(|| import.module.clone()),
                import.module.clone(),
            );
            let Some(iface) = interfaces.get(&import.module) else {
                continue;
            };
            for identity in &iface.exposed_constructors {
                let slot = aliases.entry(bare_name(identity).to_string()).or_default();
                if !slot.contains(identity) {
                    slot.push(identity.clone());
                }
                identities.insert(identity.clone());
            }
        }
        ConstructorScope {
            aliases,
            modules_by_qualifier,
            identities,
        }
    }

    /// The identity a written constructor name means here, or `None` for a name
    /// this stage has no opinion about — a prelude constructor (`Just`, `True`),
    /// a glyph match tag (`AptPackage`, `Symlink`), an unknown name. Those pass
    /// through untouched for `infer` to bind or reject, which is also why prelude
    /// constructors cannot be shadowed by a module: they are never in `aliases`,
    /// and `register_type_decls` refuses a variant that would take one of their
    /// bare names.
    ///
    /// Splitting the dotted spelling at the *last* dot is what makes a nested
    /// module reachable: `Amb.Ctor.Hold` is `Hold` of module `Amb.Ctor`, never
    /// `Ctor.Hold` of `Amb`.
    fn resolve(&self, name: &str, span: &Span) -> Result<Option<String>, Error> {
        if let Some((qualifier, bare)) = name.rsplit_once('.') {
            let Some(owner) = self.modules_by_qualifier.get(qualifier) else {
                return Ok(None);
            };
            let identity = qualified_constructor_name(owner, bare);
            if !self.identities.contains(&identity) {
                return Err(Error {
                    phase: Phase::Type,
                    msg: format!("`{owner}` has no constructor named `{bare}` in scope here"),
                    span: span.clone(),
                    note: Some(format!(
                        "a constructor is in scope only where its type is exposed open — check that `{owner}` declares `{bare}` and exposes its type as `Type(..)`"
                    )),
                    file: None,
                });
            }
            return Ok(Some(identity));
        }
        match self.aliases.get(name) {
            None => Ok(None),
            Some(candidates) if candidates.len() == 1 => Ok(Some(candidates[0].clone())),
            Some(candidates) => Err(ambiguous_constructor(name, candidates, span)),
        }
    }
}

/// Two constructors of the same bare name are both in scope, so the bare
/// spelling names neither. Rendered at the reference rather than at an `import`
/// line — the imports are legitimate, and only a use site can be ambiguous —
/// which is the whole difference between this and the ADR 0046 rule it replaces.
/// The note offers both qualified spellings, because escaping into one is now the
/// repair; renaming a constructor no longer has to be.
fn ambiguous_constructor(name: &str, candidates: &[String], span: &Span) -> Error {
    let owners = candidates
        .iter()
        .map(|identity| format!("`{}`", owner_of(identity)))
        .collect::<Vec<_>>()
        .join(" and ");
    let spellings = candidates
        .iter()
        .map(|identity| format!("`{identity}`"))
        .collect::<Vec<_>>()
        .join(" or ");
    Error {
        phase: Phase::Type,
        msg: format!("`{name}` is ambiguous here: {owners} both define one"),
        span: span.clone(),
        note: Some(format!(
            "name the one you mean in full — {spellings} — or import only one of them"
        )),
        file: None,
    }
}

fn owner_of(identity: &str) -> &str {
    identity
        .rsplit_once('.')
        .map(|(owner, _)| owner)
        .unwrap_or(identity)
}

/// Rewrite a module's constructor declarations and references from the names an
/// author writes to the identities they mean, `Owner.Ctor`, so two modules'
/// `Wrap` are two constructors and both stay reachable (ADR 0051). The
/// constructor-namespace twin of `qualify_module_types`.
///
/// References are rewritten before declarations, and the order is load-bearing:
/// `scope` was built from the module as written, so rewriting `Variant::name`
/// first would leave the walk resolving `M.Wrap` against a scope that only knows
/// `Wrap`.
///
/// Unlike the type pass, which leaves `TypeDecl::name` bare for the `exposing`
/// list to match on, this one qualifies the variant name outright. Nothing
/// matches a constructor by name across the boundary — `exposing` lists types,
/// and `interface_of` reads constructors off the type decls it already holds — so
/// there is no bare spelling left to preserve.
fn qualify_module_constructors(
    module: &mut Module,
    module_name: &str,
    scope: &ConstructorScope,
) -> Result<(), Error> {
    for decl in &mut module.decls {
        qualify_decl_constructors(decl, scope)?;
    }
    for td in &mut module.type_decls {
        for variant in &mut td.variants {
            variant.name = qualified_constructor_name(module_name, &variant.name);
        }
    }
    Ok(())
}

fn qualify_decl_constructors(decl: &mut Decl, scope: &ConstructorScope) -> Result<(), Error> {
    for param in &mut decl.params {
        qualify_pattern_constructors(param, scope)?;
    }
    qualify_expr_constructors(&mut decl.body, scope)
}

/// The pattern half of the rewrite, and not an optional one: an author who can
/// only *build* `CtorA.Wrap` and not match it has half a spelling. `Nil` and
/// `Cons` never reach `scope` — the parser desugars `[]` and `::` into their own
/// [`Pattern`] arms rather than into named constructors, so the list sum stays
/// out of the constructor namespace.
fn qualify_pattern_constructors(
    pattern: &mut Spanned<Pattern>,
    scope: &ConstructorScope,
) -> Result<(), Error> {
    let span = pattern.1.clone();
    match &mut pattern.0 {
        Pattern::Wildcard
        | Pattern::Var(_)
        | Pattern::Str(_)
        | Pattern::Int(_)
        | Pattern::Char(_)
        | Pattern::Nil => Ok(()),
        Pattern::Ctor(name, subpatterns) => {
            if let Some(identity) = scope.resolve(name, &span)? {
                *name = identity;
            }
            for subpattern in subpatterns {
                qualify_pattern_constructors(subpattern, scope)?;
            }
            Ok(())
        }
        Pattern::Cons(head, tail) => {
            qualify_pattern_constructors(head, scope)?;
            qualify_pattern_constructors(tail, scope)
        }
        Pattern::Tuple(elements) => elements
            .iter_mut()
            .try_for_each(|element| qualify_pattern_constructors(element, scope)),
    }
}

/// Walks every expression a declaration can hold, since a constructor can appear
/// anywhere one does — including inside a glyph's fields and a `scroll`'s policy.
///
/// NOTE: the `match` is exhaustive with no catch-all deliberately. A new [`Expr`]
/// variant that carries a sub-expression fails the build here instead of silently
/// leaving the constructors inside it unqualified, which would surface much later
/// as an unknown constructor or a pattern that matches nothing.
fn qualify_expr_constructors(
    expr: &mut Spanned<Expr>,
    scope: &ConstructorScope,
) -> Result<(), Error> {
    let span = expr.1.clone();
    match &mut expr.0 {
        Expr::Str(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Char(_)
        | Expr::Var(_)
        | Expr::PolicyExhaust(_) => Ok(()),
        Expr::Ctor(name) => {
            if let Some(identity) = scope.resolve(name, &span)? {
                *name = identity;
            }
            Ok(())
        }
        Expr::AptPackage(inner) | Expr::SystemdService(inner) => {
            qualify_expr_constructors(inner, scope)
        }
        Expr::Filesystem { path, entry } => {
            qualify_expr_constructors(path, scope)?;
            match entry {
                crate::ast::EntryExpr::File { contents, mode } => {
                    qualify_expr_constructors(contents, scope)?;
                    qualify_expr_constructors(mode, scope)
                }
                crate::ast::EntryExpr::Directory { mode } => qualify_expr_constructors(mode, scope),
                crate::ast::EntryExpr::Symlink { target } => {
                    qualify_expr_constructors(target, scope)
                }
            }
        }
        Expr::LineInFile { path, line } => {
            qualify_expr_constructors(path, scope)?;
            qualify_expr_constructors(line, scope)
        }
        Expr::Scroll {
            name,
            policy,
            notifies,
            contents,
        } => {
            qualify_expr_constructors(name, scope)?;
            for optional in [policy, notifies].into_iter().flatten() {
                qualify_expr_constructors(optional, scope)?;
            }
            match contents {
                crate::ast::ContentsExpr::Glyphs(inner)
                | crate::ast::ContentsExpr::Groups(inner) => {
                    qualify_expr_constructors(inner, scope)
                }
            }
        }
        Expr::PolicyRetry(fields) => fields
            .values_mut()
            .try_for_each(|field| qualify_expr_constructors(field, scope)),
        Expr::List(items) | Expr::Tuple(items) => items
            .iter_mut()
            .try_for_each(|item| qualify_expr_constructors(item, scope)),
        Expr::Lam { param, body } => {
            qualify_pattern_constructors(param, scope)?;
            qualify_expr_constructors(body, scope)
        }
        Expr::App(f, x) => {
            qualify_expr_constructors(f, scope)?;
            qualify_expr_constructors(x, scope)
        }
        Expr::Let { decls, body } => {
            for decl in decls {
                qualify_decl_constructors(decl, scope)?;
            }
            qualify_expr_constructors(body, scope)
        }
        Expr::Record(fields) => fields
            .values_mut()
            .try_for_each(|field| qualify_expr_constructors(field, scope)),
        Expr::RecordUpdate { base, fields } => {
            qualify_expr_constructors(base, scope)?;
            fields
                .iter_mut()
                .try_for_each(|(_, value)| qualify_expr_constructors(value, scope))
        }
        Expr::Field(inner, _) => qualify_expr_constructors(inner, scope),
        Expr::Case { scrutinee, arms } => {
            qualify_expr_constructors(scrutinee, scope)?;
            for arm in arms {
                qualify_pattern_constructors(&mut arm.pat, scope)?;
                qualify_expr_constructors(&mut arm.body, scope)?;
            }
            Ok(())
        }
        Expr::If { cond, then_, else_ } => {
            qualify_expr_constructors(cond, scope)?;
            qualify_expr_constructors(then_, scope)?;
            qualify_expr_constructors(else_, scope)
        }
    }
}

/// Rewrite a module's type declarations and signatures from bare names to the
/// identities they mean, so inference unifies on `Owner.Bare` and two modules'
/// `Config` stay distinct (ADR 0049). Declaration names are rewritten too;
/// `interface_of` maps them back for the `exposing` list, which names types as
/// the author wrote them.
fn qualify_module_types(
    module: &mut Module,
    module_name: &str,
    aliases: &BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for td in &mut module.type_decls {
        for variant in &mut td.variants {
            for field in &mut variant.fields {
                qualify_type(&mut field.0, aliases)?;
            }
        }
    }
    for decl in &mut module.decls {
        if let Some(sig) = &mut decl.sig {
            qualify_type(&mut sig.0, aliases)?;
        }
    }
    for td in &mut module.type_decls {
        td.name = qualified_type_name(module_name, &td.name);
    }
    Ok(())
}

fn qualify_type(
    ty: &mut crate::ast::Type,
    aliases: &BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    match ty {
        crate::ast::Type::Con(name, args) => {
            if let Some(candidates) = aliases.get(name.as_str()) {
                if candidates.len() > 1 {
                    let owners = candidates
                        .iter()
                        .map(|identity| {
                            identity
                                .rsplit_once('.')
                                .map(|(owner, _)| owner.to_string())
                                .unwrap_or_else(|| identity.clone())
                        })
                        .collect::<Vec<_>>()
                        .join("` and `");
                    return Err(Error {
                        phase: Phase::Type,
                        msg: format!("`{name}` is ambiguous here: `{owners}` both expose one"),
                        span: 0..0,
                        note: Some(format!(
                            "name the one you mean in full — `{}` — or import only one of them",
                            candidates[0]
                        )),
                        file: None,
                    });
                }
                *name = candidates[0].clone();
            }
            for arg in args {
                qualify_type(arg, aliases)?;
            }
        }
        crate::ast::Type::Fun(from, to) => {
            qualify_type(from, aliases)?;
            qualify_type(to, aliases)?;
        }
        crate::ast::Type::Record(fields, _) => {
            for field in fields.values_mut() {
                qualify_type(field, aliases)?;
            }
        }
        crate::ast::Type::Tuple(items) => {
            for item in items {
                qualify_type(item, aliases)?;
            }
        }
        crate::ast::Type::Var(_, _) | crate::ast::Type::Rigid(_) => {}
    }
    Ok(())
}

fn type_con_names(ty: &crate::ast::Type, found: &mut BTreeSet<String>) {
    match ty {
        crate::ast::Type::Var(_, _) | crate::ast::Type::Rigid(_) => {}
        crate::ast::Type::Con(name, args) => {
            found.insert(name.clone());
            for arg in args {
                type_con_names(arg, found);
            }
        }
        crate::ast::Type::Fun(from, to) => {
            type_con_names(from, found);
            type_con_names(to, found);
        }
        crate::ast::Type::Record(fields, _) => {
            for field in fields.values() {
                type_con_names(field, found);
            }
        }
        crate::ast::Type::Tuple(elements) => {
            for element in elements {
                type_con_names(element, found);
            }
        }
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
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
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
                let name = item.name();
                if let Some(span) = iface.exposed_def_spans.get(name) {
                    defs.insert(
                        name.to_string(),
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
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
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
                if let Exposed::Value { name, .. } = item {
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
///
/// `type_owners` is harvested here too, and its scope is wider than the
/// exposing list: the exposed type names, *plus* every `Type::Con` head in the
/// schemes of the exposed values and constructors. That second half is the
/// load-bearing one. A module can hold a type back and still put it in front of
/// an importer by naming it in an exposed signature — the importer cannot write
/// the name, but its inference unifies the type by that name all the same, so a
/// same-named type from elsewhere would still be accepted for it (ADR 0045).
/// Ownership follows the declaration: a name this module declares is owned
/// here, otherwise its owner comes from `inherited_owners`, so re-exposing a
/// type never reassigns it. A head belonging to neither — `String`, `List`,
/// `Scroll` and the rest of the prelude — is owned by no module and drops out.
fn interface_of(
    module: &Module,
    module_name: &str,
    inherited_owners: &BTreeMap<String, String>,
    ty_env: TyEnv,
    value_env: Env,
) -> Interface {
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
    // `type_decls` carry identities after `qualify_module_types`; an `exposing`
    // list names types as the author wrote them, so match on the bare tail.
    let identity_of_bare: BTreeMap<String, String> = type_arities
        .keys()
        .map(|identity| (bare_name(identity).to_string(), identity.clone()))
        .collect();
    let mut exposed_type_identity: HashMap<String, String> = HashMap::new();

    let mut open_type_names: Vec<String> = Vec::new();
    match &module.exposing {
        Exposing::All => {
            for decl in &module.decls {
                exposed_values.insert(decl.name.clone());
            }
            open_type_names.extend(ctor_names.keys().cloned());
            for (identity, arity) in &type_arities {
                let bare = bare_name(identity).to_string();
                exposed_type_arities.insert(bare.clone(), *arity);
                exposed_type_identity.insert(bare, identity.clone());
            }
        }
        Exposing::Explicit(items) => {
            for item in items {
                match item {
                    Exposed::Value { name, .. } => {
                        exposed_values.insert(name.clone());
                    }
                    Exposed::Type { name, open, .. } => {
                        if let Some(arity) = identity_of_bare
                            .get(name)
                            .and_then(|identity| type_arities.get(identity))
                        {
                            exposed_type_arities.insert(name.clone(), *arity);
                            if let Some(identity) = identity_of_bare.get(name) {
                                exposed_type_identity.insert(name.clone(), identity.clone());
                            }
                        }
                        if *open {
                            if let Some(identity) = identity_of_bare.get(name) {
                                open_type_names.push(identity.clone());
                            }
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
            let name_span = td.span.start..td.span.start + bare_name(&td.name).len();
            exposed_def_spans.insert(td.name.clone(), name_span);
        }
        for variant in &td.variants {
            if exposed_constructors.contains(&variant.name) {
                let name_span =
                    variant.span.start..variant.span.start + bare_name(&variant.name).len();
                exposed_def_spans.insert(variant.name.clone(), name_span);
            }
        }
    }

    let mut surface_type_names: BTreeSet<String> = exposed_type_arities.keys().cloned().collect();
    for name in exposed_values.iter().chain(exposed_constructors.iter()) {
        if let Some(scheme) = ty_env.scheme(name) {
            type_con_names(&scheme.ty, &mut surface_type_names);
        }
    }
    let declared_here: HashSet<&str> = module
        .type_decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect();
    let type_owners = surface_type_names
        .into_iter()
        .filter_map(|type_name| {
            if declared_here.contains(type_name.as_str()) {
                Some((type_name, module_name.to_string()))
            } else {
                inherited_owners
                    .get(&type_name)
                    .map(|owner| (type_name, owner.clone()))
            }
        })
        .collect();

    Interface {
        ty_env,
        value_env,
        exposed_values,
        exposed_constructors,
        exposed_type_arities,
        exposed_type_identity,
        exposed_ctor_schemes,
        exposed_sum_ctors,
        exposed_def_spans,
        type_owners,
    }
}

/// Enforce ADR 0049: an `exposing` list names only what this module declares.
/// Re-export is Elm's rule adopted whole — a module that did not declare a name
/// cannot become the door through which importers reach it.
///
/// The two halves of a re-export behaved differently before this check, and
/// neither was an error. A re-exposed **value** worked: `interface_of` recorded
/// the name, and `ty_env` / `value_env` carry the bindings an `import … exposing`
/// brought in, so an importer wrote `B.thing` and got `A`'s value. A re-exposed
/// **type** silently vanished instead — `interface_of` matches the list against
/// `identity_of_bare`, built from this module's own `type_decls`, so the entry
/// dropped and the importer was told `B` does not expose it. Rejecting at the
/// exposing list replaces both with one message at the line that is wrong.
///
/// Runs on the module as parsed, before `qualify_module_types`: afterwards
/// `type_decls` carry `Owner.Bare` identities while the exposing list still holds
/// the bare names the author wrote, and the two would no longer compare.
///
/// An uppercase item is always a *type* — only `Type(..)` carries constructors,
/// and there is no spelling for exposing one alone — so a lone constructor name
/// gets `exposed_constructor_is_not_a_type` rather than the undeclared message,
/// which would otherwise deny a name the reader can see declared two lines down.
fn reject_undeclared_exposures(loaded: &Loaded, module_name: &str) -> Result<(), Error> {
    let Exposing::Explicit(items) = &loaded.module.exposing else {
        return Ok(());
    };
    let declared_values: HashSet<&str> = loaded
        .module
        .decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect();
    let declared_types: HashSet<&str> = loaded
        .module
        .type_decls
        .iter()
        .map(|decl| decl.name.as_str())
        .collect();
    for item in items {
        match item {
            Exposed::Value { name, .. } => {
                if !declared_values.contains(name.as_str()) {
                    return Err(undeclared_exposure(loaded, module_name, item));
                }
            }
            Exposed::Type { name, .. } => {
                if declared_types.contains(name.as_str()) {
                    continue;
                }
                return Err(match declaring_type_of_variant(&loaded.module, name) {
                    Some(owning_type) => {
                        exposed_constructor_is_not_a_type(loaded, module_name, item, owning_type)
                    }
                    None => undeclared_exposure(loaded, module_name, item),
                });
            }
        }
    }
    Ok(())
}

fn declaring_type_of_variant<'a>(module: &'a Module, variant: &str) -> Option<&'a str> {
    module
        .type_decls
        .iter()
        .find(|decl| decl.variants.iter().any(|v| v.name == variant))
        .map(|decl| decl.name.as_str())
}

fn undeclared_exposure(loaded: &Loaded, module_name: &str, item: &Exposed) -> Error {
    let name = item.name();
    Error {
        phase: Phase::Type,
        msg: format!("module `{module_name}` exposes `{name}`, which it does not declare"),
        span: item.span().clone(),
        note: Some(format!(
            "an `exposing` list may name only this module's own declarations (ADR 0049) — \
             declare `{name}` in `{module_name}`, or leave it out and let an importer take it \
             from the module that does declare it"
        )),
        file: Some(loaded.path.clone()),
    }
}

fn exposed_constructor_is_not_a_type(
    loaded: &Loaded,
    module_name: &str,
    item: &Exposed,
    owning_type: &str,
) -> Error {
    let name = item.name();
    Error {
        phase: Phase::Type,
        msg: format!(
            "module `{module_name}` exposes `{name}`, which is a constructor of `{owning_type}` \
             rather than a type it declares"
        ),
        span: item.span().clone(),
        note: Some(format!(
            "an `exposing` list names types, and a type carries its constructors with it — write \
             `{owning_type}(..)` to expose `{owning_type}` and `{name}` together"
        )),
        file: Some(loaded.path.clone()),
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
        span: e.span,
        note: None,
        file: Some(loaded.path.clone()),
    }
}
