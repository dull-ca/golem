use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ariadne::{Color, Label, Report, ReportKind, Source};
use clap::{Parser, Subcommand, ValueEnum};

use emet::{compile, compile_file, Compiled, Error, Phase};
use scroll_format::{to_bytes, to_json, Manifest};

const EMET_VERSION: &str = env!("CARGO_PKG_VERSION");

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

#[derive(Parser)]
#[command(name = "emetc", about = "Compile an emet program to a scroll manifest")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Build(BuildArgs),
}

#[derive(clap::Args)]
struct BuildArgs {
    file: Option<PathBuf>,

    #[arg(short = 'o', long = "out")]
    out: Option<PathBuf>,

    #[arg(long = "format", value_enum, default_value_t = OutputFormat::Binary)]
    format: OutputFormat,

    #[arg(long = "text", conflicts_with_all = ["human", "json"])]
    text: bool,

    #[arg(long = "human", conflicts_with_all = ["text", "json"])]
    human: bool,

    #[arg(long = "json", conflicts_with_all = ["text", "human"])]
    json: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Binary,
    Text,
    Json,
}

impl BuildArgs {
    fn resolved_format(&self) -> OutputFormat {
        if self.text || self.human {
            OutputFormat::Text
        } else if self.json {
            OutputFormat::Json
        } else {
            self.format
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Build(args)) => run_build(args),
        None => run_build(BuildArgs {
            file: None,
            out: None,
            format: OutputFormat::Text,
            text: false,
            human: false,
            json: false,
        }),
    }
}

fn run_build(args: BuildArgs) -> ExitCode {
    match &args.file {
        Some(p) => {
            let src = match std::fs::read_to_string(p) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("cannot read {}: {e}", p.display());
                    return ExitCode::from(2);
                }
            };
            match compile_file(p) {
                Ok(compiled) => emit(compiled, &args),
                Err(e) => {
                    report_error(&p.display().to_string(), &src, &e);
                    ExitCode::from(1)
                }
            }
        }
        None => match compile(DEMO) {
            Ok(compiled) => emit(compiled, &args),
            Err(e) => {
                report_error("<demo>", DEMO, &e);
                ExitCode::from(1)
            }
        },
    }
}

fn emit(compiled: Compiled, args: &BuildArgs) -> ExitCode {
    match args.resolved_format() {
        OutputFormat::Text => {
            print_text(&compiled);
            ExitCode::SUCCESS
        }
        OutputFormat::Json => {
            let manifest = Manifest::from_scrolls(compiled.scrolls, EMET_VERSION);
            println!("{}", to_json(&manifest));
            ExitCode::SUCCESS
        }
        OutputFormat::Binary => {
            let manifest = Manifest::from_scrolls(compiled.scrolls, EMET_VERSION);
            write_binary(&manifest, args.out.as_deref())
        }
    }
}

fn write_binary(manifest: &Manifest, out: Option<&std::path::Path>) -> ExitCode {
    let bytes = to_bytes(manifest);
    match out {
        Some(path) => match std::fs::write(path, &bytes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cannot write {}: {e}", path.display());
                ExitCode::from(2)
            }
        },
        None => match std::io::stdout().write_all(&bytes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cannot write manifest to stdout: {e}");
                ExitCode::from(2)
            }
        },
    }
}

fn print_text(compiled: &Compiled) {
    println!("main : {}", compiled.main_ty);
    println!("planned scrolls ({}):", compiled.scrolls.len());
    for s in &compiled.scrolls {
        println!("  scroll `{}` ({} glyphs):", s.name, s.glyphs.len());
        for g in &s.glyphs {
            println!("    * {}", g.describe());
        }
    }
}

fn report_error(name: &str, src: &str, e: &Error) {
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
    let mut label = Label::new((name, span))
        .with_message(&e.msg)
        .with_color(Color::Red);
    if let Some(note) = &e.note {
        label = label.with_message(format!("{}\n  note: {note}", e.msg));
    }
    Report::build(ReportKind::Error, name, e.span.start)
        .with_message(kind)
        .with_label(label)
        .finish()
        .eprint((name, Source::from(src)))
        .ok();
}
