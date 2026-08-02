use std::fs;
use std::path::PathBuf;

use emet::ir::{Chunk, Contents, Entry, Glyph, Scroll, Secret, Text};
use emet::secrets::SecretOptions;
use emet::{compile_file_all, compile_file_all_with, Compiled};
use scroll_format::{to_bytes, Manifest};

const FLEET_KEY: &str = "00112233445566778899aabbccddeeff\
                         00112233445566778899aabbccddeeff\
                         ffeeddccbbaa99887766554433221100\
                         ffeeddccbbaa99887766554433221100";

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Project {
        let root = std::env::temp_dir().join(format!("emet_secrets_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Project { root }
    }

    fn declaring(tag: &str, declarations: &str, dotenv: &str) -> Project {
        let project = Project::new(tag);
        project.write(
            "secretspec.toml",
            &format!(
                "[project]\nname = \"emet-test\"\nrevision = \"1.0\"\nrequire_reason = false\n\n\
                 [profiles.default]\n{declarations}"
            ),
        );
        project.write(".env", dotenv);
        project.write("fleet.key", FLEET_KEY);
        project
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn options(&self) -> SecretOptions {
        SecretOptions {
            key_file: Some(self.root.join("fleet.key")),
            provider: Some("dotenv".to_string()),
            profile: Some("default".to_string()),
        }
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn only_glyph(compiled: &Compiled) -> &Glyph {
    match &compiled.scrolls[..] {
        [Scroll {
            contents: Contents::Glyphs(glyphs),
            ..
        }] => &glyphs[0],
        other => panic!("expected one leaf scroll, got {other:?}"),
    }
}

fn line_of(compiled: &Compiled) -> &Text {
    match only_glyph(compiled) {
        Glyph::LineInFile { line, .. } => line,
        other => panic!("expected a lineInFile glyph, got {other:?}"),
    }
}

fn contents_of(compiled: &Compiled) -> &Text {
    match only_glyph(compiled) {
        Glyph::Filesystem {
            entry: Entry::File { contents, .. },
            ..
        } => contents,
        other => panic!("expected a file glyph, got {other:?}"),
    }
}

fn manifest_bytes(compiled: Compiled) -> Vec<u8> {
    to_bytes(&Manifest::from_scrolls(compiled.scrolls, "0.1.0"))
}

fn declared_db() -> &'static str {
    "DB_PASSWORD = { description = \"the database password\" }\n"
}

fn interpolating_program() -> &'static str {
    r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = "Environment=PW=${Secretspec.get "DB_PASSWORD"}"
              }
          ]
      }
  ]
"#
}

#[test]
fn interpolating_a_secret_keeps_the_literal_chunks_and_seals_only_the_secret() {
    let project = Project::declaring("interpolate", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write("app.emet", interpolating_program());

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();

    match line_of(&compiled) {
        Text::Composed(chunks) => match &chunks[..] {
            [Chunk::Lit(literal), Chunk::Hole(Secret::Sealed { ciphertext, .. })] => {
                assert_eq!(literal, "Environment=PW=");
                assert!(!ciphertext.windows(7).any(|w| w == b"hunter2"));
            }
            other => panic!("expected one literal and one sealed hole, got {other:?}"),
        },
        other => panic!("expected a composed value, got {other:?}"),
    }
}

#[test]
fn a_manifest_carrying_a_secret_contains_no_plaintext() {
    let project = Project::declaring("plaintext", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write("app.emet", interpolating_program());

    let bytes = manifest_bytes(compile_file_all_with(&entry, project.options()).unwrap());

    assert!(!bytes.windows(7).any(|window| window == b"hunter2"));
    assert!(bytes
        .windows("Environment=PW=".len())
        .any(|window| window == b"Environment=PW="));
}

#[test]
fn compiling_the_same_source_twice_produces_identical_manifest_bytes() {
    let project = Project::declaring("determinism", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write("app.emet", interpolating_program());

    let first = manifest_bytes(compile_file_all_with(&entry, project.options()).unwrap());
    let second = manifest_bytes(compile_file_all_with(&entry, project.options()).unwrap());

    assert_eq!(first, second);
}

#[test]
fn rotating_the_secret_moves_the_ciphertext() {
    let project = Project::declaring("rotation", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write("app.emet", interpolating_program());
    let before = manifest_bytes(compile_file_all_with(&entry, project.options()).unwrap());

    project.write(".env", "DB_PASSWORD=\"hunter3\"\n");
    let after = manifest_bytes(compile_file_all_with(&entry, project.options()).unwrap());

    assert_ne!(before, after);
}

#[test]
fn an_undeclared_key_is_refused_by_name_and_lists_the_declared_ones() {
    let project = Project::declaring("undeclared", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        &interpolating_program().replace("DB_PASSWORD", "DB_PASWORD"),
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0]
            .msg
            .contains("secret `DB_PASWORD` is not declared")
            && errors[0].msg.contains("declared secrets are: DB_PASSWORD"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_declared_key_the_provider_cannot_supply_names_the_provider() {
    let project = Project::declaring("missing", declared_db(), "");
    let entry = project.write("app.emet", interpolating_program());

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("secret `DB_PASSWORD` is declared")
            && errors[0].msg.contains("provider `dotenv")
            && errors[0].msg.contains("has no value for it"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_reaching_a_path_is_refused_by_naming_the_field() {
    let project = Project::declaring("path", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/${Secretspec.get "DB_PASSWORD"}"
              , line = "x"
              }
          ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("a secret cannot be used as `path`"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_reaching_a_scroll_name_is_refused_by_naming_the_field() {
    let project = Project::declaring("scrollname", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = Secretspec.get "DB_PASSWORD"
      , glyphs = [ aptPackage { name = "nginx" } ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("a secret cannot be used as `name`"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_reaching_a_unit_name_is_refused_by_naming_the_field() {
    let project = Project::declaring("unitname", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs = [ systemdService { unit = Secretspec.get "DB_PASSWORD" } ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("a secret cannot be used as `unit`"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_cannot_be_inspected_by_a_string_predicate() {
    let project = Project::declaring("inspect", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = if String.contains " " (Secretspec.get "DB_PASSWORD") then "a" else "b"
              }
          ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0]
            .msg
            .contains("`String.contains` cannot inspect a secret"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_survives_a_string_transformation_and_stays_sealed() {
    let project = Project::declaring("transform", declared_db(), "DB_PASSWORD=\"hun\\\"ter\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = "PW=" ++ String.replace "\"" "_" (Secretspec.get "DB_PASSWORD")
              }
          ]
      }
  ]
"#,
    );

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();

    match line_of(&compiled) {
        Text::Composed(chunks) => match &chunks[..] {
            [Chunk::Lit(literal), Chunk::Hole(_)] => assert_eq!(literal, "PW="),
            other => panic!("expected one literal and one sealed hole, got {other:?}"),
        },
        other => panic!("expected a composed value, got {other:?}"),
    }
}

#[test]
fn taint_survives_being_reported_as_a_secret_so_a_library_can_branch_on_it() {
    let project = Project::declaring("issecret", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"marked : String -> String
marked value =
  if String.isSecret value then "sealed:" ++ value else "plain:" ++ value

main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile { path = "/etc/a", line = marked (Secretspec.get "DB_PASSWORD") }
          , lineInFile { path = "/etc/b", line = marked "public" }
          ]
      }
  ]
"#,
    );

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();
    let glyphs = match &compiled.scrolls[0].contents {
        Contents::Glyphs(glyphs) => glyphs,
        other => panic!("expected a leaf scroll, got {other:?}"),
    };

    match &glyphs[0] {
        Glyph::LineInFile {
            line: Text::Composed(chunks),
            ..
        } => assert_eq!(chunks[0], Chunk::Lit("sealed:".to_string())),
        other => panic!("expected a composed line, got {other:?}"),
    }
    match &glyphs[1] {
        Glyph::LineInFile { line, .. } => assert_eq!(line.plain(), Some("plain:public")),
        other => panic!("expected a plain line, got {other:?}"),
    }
}

#[test]
fn a_multi_line_file_stays_readable_around_its_one_sealed_hole() {
    let project = Project::declaring("filecontents", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ file
              { path = "/etc/app.conf"
              , contents = String.join "\n" [ "[db]", "password=" ++ Secretspec.get "DB_PASSWORD", "port=5432" ]
              , mode = "0600"
              }
          ]
      }
  ]
"#,
    );

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();

    match contents_of(&compiled) {
        Text::Composed(chunks) => match &chunks[..] {
            [Chunk::Lit(before), Chunk::Hole(_), Chunk::Lit(after)] => {
                assert_eq!(before, "[db]\npassword=");
                assert_eq!(after, "\nport=5432");
            }
            other => panic!("expected literal-hole-literal, got {other:?}"),
        },
        other => panic!("expected a composed value, got {other:?}"),
    }
}

#[test]
fn a_program_with_no_secret_compiles_with_no_key_and_no_provider() {
    let project = Project::new("nosecret");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll { name = "app", glyphs = [ aptPackage { name = "nginx" } ] } ]
"#,
    );

    let compiled = compile_file_all(&entry).unwrap();

    assert_eq!(compiled.scrolls[0].all_glyphs().len(), 1);
}

#[test]
fn a_secret_with_no_fleet_key_configured_says_which_flag_is_missing() {
    let project = Project::declaring("nokey", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write("app.emet", interpolating_program());

    let errors = compile_file_all_with(
        &entry,
        SecretOptions {
            key_file: None,
            provider: Some("dotenv".to_string()),
            profile: Some("default".to_string()),
        },
    )
    .unwrap_err();

    assert!(
        errors[0].msg.contains("--secret-key") && errors[0].msg.contains("GOLEM_SECRET_KEY_FILE"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_malformed_fleet_key_is_refused_before_any_provider_is_consulted() {
    let project = Project::declaring("badkey", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    project.write("fleet.key", "not-hex");
    let entry = project.write("app.emet", interpolating_program());

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("hexadecimal characters"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_used_twice_seals_to_the_same_ciphertext() {
    let project = Project::declaring("repeat", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile { path = "/etc/a", line = "PW=" ++ Secretspec.get "DB_PASSWORD" }
          , lineInFile { path = "/etc/b", line = "PW=" ++ Secretspec.get "DB_PASSWORD" }
          ]
      }
  ]
"#,
    );

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();
    let glyphs = match &compiled.scrolls[0].contents {
        Contents::Glyphs(glyphs) => glyphs,
        other => panic!("expected a leaf scroll, got {other:?}"),
    };
    let sealed = |glyph: &Glyph| match glyph {
        Glyph::LineInFile {
            line: Text::Composed(chunks),
            ..
        } => chunks
            .iter()
            .filter_map(|chunk| match chunk {
                Chunk::Hole(Secret::Sealed { ciphertext, .. }) => Some(ciphertext.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        other => panic!("expected a composed line, got {other:?}"),
    };

    assert_eq!(sealed(&glyphs[0]), sealed(&glyphs[1]));
}

fn repo_lib_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../lib")
        .canonicalize()
        .unwrap()
}

fn quadlet_workload_program() -> &'static str {
    r#"module Main exposing (..)

import Quadlet exposing
  ( image
  , env
  , Network(..)
  , Restart(..)
  , Expose(..)
  , Workload(..)
  , workloadGlyphs
  )

app : Workload
app =
  Workload
    { name = "app"
    , image = image "docker.io/library" "postgres" "16"
    , env = [ env "POSTGRES_PASSWORD" (Secretspec.get "DB_PASSWORD") ]
    , labels = []
    , ports = []
    , networks = []
    , volumes = []
    , restart = Always
    , expose = Unexposed
    }

main : List Scroll
main = [ scroll { name = "app", glyphs = workloadGlyphs app } ]
"#
}

#[test]
fn a_quadlet_workload_carries_a_secret_env_var_as_a_sealed_hole() {
    let project = Project::declaring("quadlet", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    project.write(
        "emet.json",
        &format!(
            "{{ \"source-directories\": [{:?}] }}",
            repo_lib_dir().display().to_string()
        ),
    );
    let entry = project.write("app.emet", quadlet_workload_program());

    let compiled = compile_file_all_with(&entry, project.options()).unwrap();
    let unit = compiled.scrolls[0]
        .all_glyphs()
        .into_iter()
        .find_map(|glyph| match glyph {
            Glyph::Filesystem {
                path,
                entry: Entry::File { contents, .. },
            } if path.ends_with("app.container") => Some(contents),
            _ => None,
        })
        .expect("the workload writes a quadlet unit file");

    let literals: String = unit
        .chunks()
        .filter_map(|chunk| match chunk {
            Chunk::Lit(s) => Some(s.as_str()),
            Chunk::Hole(_) => None,
        })
        .collect();
    assert!(
        literals.contains("Environment=POSTGRES_PASSWORD="),
        "{literals}"
    );
    assert_eq!(unit.holes().count(), 1);
    assert!(
        !to_bytes(&Manifest::from_scrolls(compiled.scrolls.clone(), "0.1.0"))
            .windows(7)
            .any(|w| w == b"hunter2")
    );
}

#[test]
fn comparing_a_secret_is_refused_and_names_the_operator_as_written() {
    let project = Project::declaring("compare", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = if Secretspec.get "DB_PASSWORD" == "hunter2" then "yes" else "no"
              }
          ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("`==` cannot inspect a secret"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn matching_a_secret_against_a_string_pattern_is_refused_rather_than_silently_missing() {
    let project = Project::declaring("pattern", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"describe : String -> String
describe value =
  case value of
    "hunter2" -> "guessed"
    _ -> "unknown"

main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = describe (Secretspec.get "DB_PASSWORD")
              }
          ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0].msg.contains("matched against a string pattern"),
        "{}",
        errors[0].msg
    );
}

#[test]
fn a_secret_misused_inside_a_higher_order_builtin_reports_rather_than_panics() {
    let project = Project::declaring("higherorder", declared_db(), "DB_PASSWORD=\"hunter2\"\n");
    let entry = project.write(
        "app.emet",
        r#"lengthOf : String -> String
lengthOf value = String.fromInt (String.length value)

main : List Scroll
main =
  [ scroll
      { name = "app"
      , glyphs =
          [ lineInFile
              { path = "/etc/app.env"
              , line = String.concat (List.map lengthOf [ Secretspec.get "DB_PASSWORD" ])
              }
          ]
      }
  ]
"#,
    );

    let errors = compile_file_all_with(&entry, project.options()).unwrap_err();

    assert!(
        errors[0]
            .msg
            .contains("`String.length` cannot inspect a secret"),
        "{}",
        errors[0].msg
    );
}
