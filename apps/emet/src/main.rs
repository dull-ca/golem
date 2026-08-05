use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use ariadne::{Color, Label, Report, ReportKind, Source};
use clap::{Parser, Subcommand, ValueEnum};

use emet::secrets::SecretOptions;
use emet::{compile_all, compile_file_all_with, Compiled, Error, Phase};
use scroll_format::{to_bytes, to_json, Manifest};

const EMET_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEMO: &str = r#"-- Elm/Haskell-style: top-level decls, optional signatures, HM inference.

webserver : String -> SystemdService
webserver unit = systemdService { unit = unit }

basePkg : String -> AptPackage
basePkg name = aptPackage { name = name }

-- no signature here: inferred as String -> SystemdService
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

    #[arg(long = "secret-key", env = "GOLEM_SECRET_KEY_FILE")]
    secret_key: Option<PathBuf>,

    #[arg(long = "secret-provider", env = "SECRETSPEC_PROVIDER")]
    secret_provider: Option<String>,

    #[arg(long = "secret-profile", env = "SECRETSPEC_PROFILE")]
    secret_profile: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Binary,
    Text,
    Json,
}

impl BuildArgs {
    fn secret_options(&self) -> SecretOptions {
        SecretOptions {
            key_file: self.secret_key.clone(),
            provider: self.secret_provider.clone(),
            profile: self.secret_profile.clone(),
        }
    }

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
            secret_key: None,
            secret_provider: None,
            secret_profile: None,
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
            match compile_file_all_with(p, args.secret_options()) {
                Ok(compiled) => emit(compiled, &args),
                Err(errors) => {
                    report_errors(&p.display().to_string(), &src, &errors);
                    ExitCode::from(1)
                }
            }
        }
        None => match compile_all(DEMO) {
            Ok(compiled) => emit(compiled, &args),
            Err(errors) => {
                report_errors("<demo>", DEMO, &errors);
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
    print!("{}", emet::render_text(compiled));
}

fn report_errors(entry_name: &str, entry_src: &str, errors: &[Error]) {
    let mut sources: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for e in errors {
        if let Some(path) = &e.file {
            let name = path.display().to_string();
            sources
                .entry(name)
                .or_insert_with(|| std::fs::read_to_string(path).unwrap_or_default());
        }
    }
    for e in errors {
        match &e.file {
            Some(path) => {
                let name = path.display().to_string();
                report_error(&name, &sources[&name], e);
            }
            None => report_error(entry_name, entry_src, e),
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
    let span = underline(&e.span, src);
    let mut label = Label::new((name, span.clone()))
        .with_message(&e.msg)
        .with_color(Color::Red);
    if let Some(note) = &e.note {
        label = label.with_message(format!("{}\n  note: {note}", e.msg));
    }
    Report::build(ReportKind::Error, name, span.start)
        .with_message(kind)
        .with_label(label)
        .finish()
        .eprint((name, Source::from(src)))
        .ok();
}

fn underline(span: &std::ops::Range<usize>, src: &str) -> std::ops::Range<usize> {
    let unlocated = span.start == 0 && span.end == 0;
    let reaches_past_this_source = span.end > src.len();
    if unlocated || reaches_past_this_source {
        0..src.len().min(1)
    } else {
        span.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::underline;

    #[test]
    fn a_span_inside_the_source_underlines_itself() {
        assert_eq!(underline(&(2..5), "abcdefg"), 2..5);
    }

    #[test]
    fn the_unlocated_span_underlines_the_first_character() {
        assert_eq!(underline(&(0..0), "abcdefg"), 0..1);
    }

    #[test]
    fn a_span_reaching_past_the_end_of_the_source_underlines_the_first_character() {
        assert_eq!(underline(&(120..140), "abcdefg"), 0..1);
        assert_eq!(underline(&(5..9), "abcdefg"), 0..1);
    }

    #[test]
    fn a_zero_width_span_at_the_end_of_input_is_left_alone() {
        assert_eq!(underline(&(7..7), "abcdefg"), 7..7);
    }

    #[test]
    fn an_empty_source_underlines_nothing_rather_than_panicking() {
        assert_eq!(underline(&(3..9), ""), 0..0);
    }
}
