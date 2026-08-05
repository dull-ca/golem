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
//! internals.
//!
//! A type and a constructor are each reached by bare name, so this stage also
//! enforces one owner per name in both namespaces, before inference runs on any
//! module: `reject_type_name_collisions` (ADR 0045), then
//! `reject_constructor_name_collisions` (ADR 0046). Type collisions are checked
//! first because they are unsoundness — two modules' `Thing` are one type, and a
//! value of either is accepted where the other is proved — while a constructor
//! collision costs an unreachable constructor and a diagnostic that names the
//! wrong type.
//!
//! The two rules are **not** mirrors of one another; reading either as the
//! other's counterpart gets both wrong. The type rule spans a module's exposed
//! *surface*, because a private type named in an exposed signature still unifies
//! by name in the importer. The constructor rule stops at the exposing list,
//! because only a `Type(..)` export puts constructors in scope at all, so a
//! constructor behind a closed export can be neither built nor matched anywhere
//! else. Type ownership also propagates — a re-exposed type keeps its declaring
//! module (`inherited_type_owners`) — while constructor ownership cannot,
//! because `interface_of` harvests constructors from the module's own
//! `type_decls` only, so no module can re-expose one it did not declare.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
/// `type_owners` maps every type name this module's exposed surface can put in
/// front of an importer to the module that *declared* it, which is what makes
/// ADR 0045's one-owner rule checkable — see `interface_of`.
/// `ctor_owners` is the constructor namespace's counterpart, for ADR 0046, and
/// its scope is narrower on both axes: it holds only the constructors of a
/// `Type(..)` export, since no other constructor is in scope in an importer, and
/// its owner is always *this* module, since a constructor cannot be re-exposed.
/// Each entry also carries the type the constructor builds, which the diagnostic
/// names.
struct Interface {
    ty_env: TyEnv,
    value_env: Env,
    exposed_values: HashSet<String>,
    exposed_constructors: Vec<String>,
    exposed_type_arities: HashMap<String, usize>,
    exposed_ctor_schemes: HashMap<String, Scheme>,
    exposed_sum_ctors: HashMap<String, Vec<(String, usize)>>,
    exposed_def_spans: HashMap<String, Span>,
    type_owners: BTreeMap<String, String>,
    ctor_owners: BTreeMap<String, ConstructorOrigin>,
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

        if let Err(e) = reject_type_name_collisions(loaded_mod, name, &interfaces)
            .and_then(|()| reject_constructor_name_collisions(loaded_mod, name, &interfaces))
        {
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
            &loaded_mod.module,
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
            if let Ok(final_ty) = infer::check_library(
                &loaded_mod.module,
                base_ty,
                &imported_types,
                &imported_ctors,
            ) {
                let iface = interface_of(
                    &loaded_mod.module,
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

        reject_type_name_collisions(loaded_mod, name, &interfaces)?;
        reject_constructor_name_collisions(loaded_mod, name, &interfaces)?;
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
            let (scrolls, glyph_spans) = eval::eval_entry(&loaded_mod.module, base_val)
                .map_err(|e| analyze_error(loaded_mod, e))?;
            crate::analyze(&scrolls, &glyph_spans).map_err(|mut e| {
                e.file = Some(loaded_mod.path.clone());
                e
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
            let owners = inherited_type_owners(&loaded_mod.module, &interfaces);
            interfaces.insert(
                name.clone(),
                interface_of(&loaded_mod.module, name, &owners, final_ty, final_val),
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
            let constructors_visible = *open && iface.exposed_sum_ctors.contains_key(name);
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
///
/// NOTE: both maps are keyed by bare name and inserted per import in order, so
/// the last import wins. That is safe on both sides only because this module has
/// already passed both collision checks. `sum_ctors` is keyed by a *type* name,
/// which `reject_type_name_collisions` gives one owner (ADR 0045); `ctor_schemes`
/// is keyed by a bare *constructor* name, which `reject_constructor_name_collisions`
/// gives one owner (ADR 0046). Either key can therefore arrive twice only from a
/// single owner — one module imported twice — so an overwrite rewrites an entry
/// with itself. Before ADR 0046 the `ctor_schemes` side did lose constructors
/// here, silently.
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

/// How one type name reached the module being checked: `owner` declared it,
/// `via` is the import that carried it in. The two differ when a module
/// re-exposes a type it did not declare, and the diagnostic says so — otherwise
/// it would name a module the author would search in vain for the `type` line.
struct TypeNameOrigin {
    owner: String,
    via: String,
}

/// Enforce ADR 0045: within one module, a type name has exactly one owner.
/// Rejected are two imports contributing the same type name under different
/// declaring modules, and a local `type` declaration whose name an import
/// already contributes.
///
/// This guards soundness, not hygiene. Emet identifies a type by its bare name
/// — `Type::Con` carries a `String` and nothing else — so two modules' `Thing`
/// are one type: a function typed `A.Thing -> …` accepts a `B.Thing`, and a
/// value of one reaches code the compiler proved held the other. Rejection is
/// the only available answer because there is no qualified spelling for a type
/// to disambiguate with.
///
/// The check reads each interface's `type_owners`, which covers the exposed
/// *surface* rather than the exposing list, so a private type named in an
/// exposed signature collides even though the importer cannot write its name.
/// A name arriving twice under the *same* owner is one type reached by two
/// paths, not a collision, and passes.
fn reject_type_name_collisions(
    loaded: &Loaded,
    module_name: &str,
    interfaces: &HashMap<String, Interface>,
) -> Result<(), Error> {
    let mut origins: HashMap<String, TypeNameOrigin> = HashMap::new();
    for import in &loaded.module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        for (type_name, owner) in &iface.type_owners {
            match origins.get(type_name) {
                Some(seen) if &seen.owner != owner => {
                    return Err(imports_define_the_same_type(
                        loaded,
                        type_name,
                        seen,
                        &TypeNameOrigin {
                            owner: owner.clone(),
                            via: import.module.clone(),
                        },
                        import.span.clone(),
                    ));
                }
                Some(_) => {}
                None => {
                    origins.insert(
                        type_name.clone(),
                        TypeNameOrigin {
                            owner: owner.clone(),
                            via: import.module.clone(),
                        },
                    );
                }
            }
        }
    }
    for declaration in &loaded.module.type_decls {
        if let Some(seen) = origins.get(&declaration.name) {
            return Err(local_type_shadows_an_imported_one(
                loaded,
                module_name,
                &declaration.name,
                seen,
                declaration.span.clone(),
            ));
        }
    }
    Ok(())
}

/// How one constructor reached the module being checked: `owner` declared it, on
/// type `type_name`. Both are in the diagnostic — two colliding constructors sit
/// on two differently-named types, and naming only the modules would leave the
/// author hunting for a `type` line without knowing which type to look for.
///
/// There is no counterpart to `TypeNameOrigin::via`: a constructor cannot be
/// re-exposed, so the import that carried it in always declared it.
#[derive(Clone)]
struct ConstructorOrigin {
    owner: String,
    type_name: String,
}

/// Enforce ADR 0046: within one module, a constructor name has exactly one
/// owner. Rejected are two imports contributing the same constructor name under
/// different declaring modules, and a local `type` declaration with a variant
/// whose name an import already contributes.
///
/// This guards reachability and diagnostics, not soundness: ADR 0045 keeps the
/// two types distinct, so no value ever crosses. Without the check, the later
/// import's constructor displaced the earlier one in `ctor_schemes` and in
/// `import_ty_env`'s bindings; the displaced one became unreachable without a
/// word, and writing it reported a mismatch against the surviving constructor's
/// type — correct code indicted by a type the author never named.
///
/// Rejection rather than a map keyed by owning module, because no use site could
/// select an entry from such a map: `CtorA.Wrap` is a *parse* error, in
/// expression and in pattern position alike, so every occurrence of a
/// constructor carries exactly its bare name. A shadowed constructor was never
/// reachable by any spelling, so admitting it buys an author nothing. What this
/// does is carry across the module boundary the rule
/// `infer::register_type_decls` already applies within one module (`duplicate
/// constructor`).
///
/// Narrower than `reject_type_name_collisions` on two counts — see this module's
/// doc comment for why neither rule generalizes to the other.
fn reject_constructor_name_collisions(
    loaded: &Loaded,
    module_name: &str,
    interfaces: &HashMap<String, Interface>,
) -> Result<(), Error> {
    let mut origins: HashMap<String, ConstructorOrigin> = HashMap::new();
    for import in &loaded.module.imports {
        let Some(iface) = interfaces.get(&import.module) else {
            continue;
        };
        for (ctor_name, origin) in &iface.ctor_owners {
            match origins.get(ctor_name) {
                Some(seen) if seen.owner != origin.owner => {
                    return Err(imports_define_the_same_constructor(
                        loaded,
                        ctor_name,
                        seen,
                        origin,
                        import.span.clone(),
                    ));
                }
                Some(_) => {}
                None => {
                    origins.insert(ctor_name.clone(), origin.clone());
                }
            }
        }
    }
    for declaration in &loaded.module.type_decls {
        for variant in &declaration.variants {
            if let Some(seen) = origins.get(&variant.name) {
                return Err(local_constructor_shadows_an_imported_one(
                    loaded,
                    &ConstructorOrigin {
                        owner: module_name.to_string(),
                        type_name: declaration.name.clone(),
                    },
                    &variant.name,
                    seen,
                    variant.span.clone(),
                ));
            }
        }
    }
    Ok(())
}

/// The half both collision diagnostics share: which module puts the name on
/// which type, and why only one of the two can be reached. Each caller appends
/// its own repair, because the repairs differ — two imports can also be split
/// across two modules, while a local declaration against an imported
/// constructor can only be renamed.
fn unreachable_constructor_note(
    ctor_name: &str,
    first: &ConstructorOrigin,
    second: &ConstructorOrigin,
) -> String {
    format!(
        "`{}` defines `{ctor_name}` on type `{}`, and `{}` defines it on type `{}`. Emet reaches a constructor only by its bare name — there is no qualified `Module.Constructor` spelling — so only one `{ctor_name}` can be reachable here; the other vanishes, and using it reports a type error against the surviving one's type. ",
        first.owner, first.type_name, second.owner, second.type_name
    )
}

/// Rendered at the second `import` line — the one that brought the duplicate in
/// — matching where `imports_define_the_same_type` renders its own.
fn imports_define_the_same_constructor(
    site: &Loaded,
    ctor_name: &str,
    first: &ConstructorOrigin,
    second: &ConstructorOrigin,
    span: Span,
) -> Error {
    let mut note = unreachable_constructor_note(ctor_name, first, second);
    note.push_str(
        "Rename one of the two constructors, or import the two modules from separate modules.",
    );
    Error {
        phase: Phase::Type,
        msg: format!(
            "`{}` and `{}` both define a constructor named `{ctor_name}`",
            first.owner, second.owner
        ),
        span,
        note: Some(note),
        file: Some(site.path.clone()),
    }
}

/// Rendered at the offending variant rather than at the import: the import is
/// legitimate, and the variant is the line this module wrote.
fn local_constructor_shadows_an_imported_one(
    site: &Loaded,
    local: &ConstructorOrigin,
    ctor_name: &str,
    imported: &ConstructorOrigin,
    span: Span,
) -> Error {
    let mut note = unreachable_constructor_note(ctor_name, local, imported);
    note.push_str("Rename one of the two constructors.");
    Error {
        phase: Phase::Type,
        msg: format!(
            "`{}` and `{}` both define a constructor named `{ctor_name}`",
            local.owner, imported.owner
        ),
        span,
        note: Some(note),
        file: Some(site.path.clone()),
    }
}

fn imports_define_the_same_type(
    site: &Loaded,
    type_name: &str,
    first: &TypeNameOrigin,
    second: &TypeNameOrigin,
    span: Span,
) -> Error {
    let mut note = String::new();
    for origin in [first, second] {
        if origin.owner != origin.via {
            note.push_str(&format!(
                "`{}` exposes the `{type_name}` defined in `{}`. ",
                origin.via, origin.owner
            ));
        }
    }
    note.push_str(&format!(
        "Emet knows a type only by its bare name, so with both modules imported here the two `{type_name}` types would be interchangeable — a value of one would be accepted wherever the other is expected. Rename one of the two types, or import the two modules from separate modules."
    ));
    Error {
        phase: Phase::Type,
        msg: format!(
            "`{}` and `{}` both define a type named `{type_name}`",
            first.owner, second.owner
        ),
        span,
        note: Some(note),
        file: Some(site.path.clone()),
    }
}

fn local_type_shadows_an_imported_one(
    site: &Loaded,
    module_name: &str,
    type_name: &str,
    imported: &TypeNameOrigin,
    span: Span,
) -> Error {
    let mut note = String::new();
    if imported.owner != imported.via {
        note.push_str(&format!(
            "`{}` exposes the `{type_name}` defined in `{}`. ",
            imported.via, imported.owner
        ));
    }
    note.push_str(&format!(
        "Emet knows a type only by its bare name, so this `{type_name}` and `{}`'s would be interchangeable — a value of one would be accepted wherever the other is expected. Rename one of the two types.",
        imported.owner
    ));
    Error {
        phase: Phase::Type,
        msg: format!(
            "`{module_name}` and `{}` both define a type named `{type_name}`",
            imported.owner
        ),
        span,
        note: Some(note),
        file: Some(site.path.clone()),
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
                    Exposed::Value { name, .. } => {
                        exposed_values.insert(name.clone());
                    }
                    Exposed::Type { name, open, .. } => {
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

    let mut ctor_owners: BTreeMap<String, ConstructorOrigin> = BTreeMap::new();
    for type_name in &open_type_names {
        if let Some(variants) = ctor_names.get(type_name) {
            exposed_constructors.extend(variants.iter().cloned());
            for ctor in variants {
                ctor_owners.insert(
                    ctor.clone(),
                    ConstructorOrigin {
                        owner: module_name.to_string(),
                        type_name: type_name.clone(),
                    },
                );
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
        exposed_ctor_schemes,
        exposed_sum_ctors,
        exposed_def_spans,
        type_owners,
        ctor_owners,
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
