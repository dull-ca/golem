//! emet CLI: parse/typecheck/evaluate a program and print the glyph plan,
//! rendering errors with ariadne.

use std::process::ExitCode;

use ariadne::{Color, Label, Report, ReportKind, Source};

use emet::{compile, Phase};

const DEMO: &str = r#"-- Elm/Haskell-style: top-level decls, optional signatures, HM inference.

webserver : Str -> SystemdService
webserver unit = systemdService { unit = unit }

basePkg : Str -> AptPackage
basePkg name = aptPackage { name = name }

-- no signature here: inferred as Str -> SystemdService
enable name =
  let unit = name
  in systemdService { unit = unit }

main : List Scroll
main =
  [ scroll
      { name = "web"
      , glyphs =
          [ basePkg "nginx"
          , webserver "nginx.service"
          , enable "redis.service"
          ]
      }
  ]
"#;

fn main() -> ExitCode {
    let path = std::env::args().nth(1);
    let (name, src) = match &path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => (p.clone(), s),
            Err(e) => {
                eprintln!("cannot read {p}: {e}");
                return ExitCode::from(2);
            }
        },
        None => ("<demo>".to_string(), DEMO.to_string()),
    };

    match compile(&src) {
        Ok(c) => {
            println!("main : {}", c.main_ty);
            println!("planned scrolls ({}):", c.scrolls.len());
            for s in &c.scrolls {
                println!("  scroll `{}` ({} glyphs):", s.name, s.glyphs.len());
                for g in &s.glyphs {
                    println!("    * {}", g.describe());
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let kind = match e.phase {
                Phase::Lex => "lex error",
                Phase::Parse => "parse error",
                Phase::Type => "type error",
                Phase::Analyze => "analysis error",
            };
            let span = if e.span.start == e.span.end && e.span.start == 0 {
                0..src.len().min(1)
            } else {
                e.span.clone()
            };
            let mut label = Label::new((name.as_str(), span))
                .with_message(&e.msg)
                .with_color(Color::Red);
            if let Some(note) = &e.note {
                label = label.with_message(format!("{}\n  note: {note}", e.msg));
            }
            Report::build(ReportKind::Error, name.as_str(), e.span.start)
                .with_message(kind)
                .with_label(label)
                .finish()
                .eprint((name.as_str(), Source::from(&src)))
                .ok();
            ExitCode::from(1)
        }
    }
}
