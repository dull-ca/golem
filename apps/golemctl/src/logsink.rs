use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::poll::Event;

// The greppable on-disk record of one apply (ADR 0033 §3a). Every event line is
// appended to a combined `all.log` and to its unit_path's own slugged file, so a
// run can be read whole or one scroll at a time after the spinner clears. IO
// errors degrade to a single warning and never abort the apply — the files are a
// record, not a dependency of the reconcile.
pub struct LogSink {
    dir: PathBuf,
    all: File,
}

pub fn apply_dir(reconcile_id: u64) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("golemctl").join(format!("apply-{reconcile_id}"))
}

// A unit_path is one flat filename: segments joined by `-`, each character
// outside `[A-Za-z0-9._-]` replaced with `_`, suffixed `.log` (ADR 0033 §3a).
// `["<removes>"]` becomes `_removes_.log`; the empty (host-root) path becomes
// `host.log`.
pub fn unit_file_name(unit_path: &[String]) -> String {
    if unit_path.is_empty() {
        return "host.log".into();
    }
    let slug: String = unit_path
        .join("-")
        .chars()
        .map(|c| if is_slug_safe(c) { c } else { '_' })
        .collect();
    format!("{slug}.log")
}

fn is_slug_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

pub fn event_line(ev: &Event) -> String {
    format!(
        "{}  {}  {}  {}  {}",
        ev.at,
        ev.level,
        ev.unit_path.join("/"),
        ev.glyph_key,
        ev.message
    )
}

impl LogSink {
    pub fn create(reconcile_id: u64) -> Result<Self, std::io::Error> {
        let dir = apply_dir(reconcile_id);
        fs::create_dir_all(&dir)?;
        let all = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("all.log"))?;
        Ok(Self { dir, all })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn write_event(&mut self, ev: &Event) -> Result<(), std::io::Error> {
        let line = event_line(ev);
        writeln!(self.all, "{line}")?;
        let mut unit = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(unit_file_name(&ev.unit_path)))?;
        writeln!(unit, "{line}")?;
        Ok(())
    }
}

// Wraps a `LogSink` so a failed create or a failed append never aborts the
// apply: on the first IO error a single warning goes to stderr and persistence
// goes quiet for the rest of the run.
pub struct Persistence {
    sink: Option<LogSink>,
    warned: bool,
}

impl Persistence {
    pub fn open(reconcile_id: u64) -> Self {
        match LogSink::create(reconcile_id) {
            Ok(sink) => Self {
                sink: Some(sink),
                warned: false,
            },
            Err(e) => {
                eprintln!("golemctl: event log unavailable: {e}");
                Self {
                    sink: None,
                    warned: true,
                }
            }
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.sink.as_ref().map(|s| s.dir())
    }

    pub fn persist(&mut self, events: &[Event]) {
        let Some(sink) = self.sink.as_mut() else {
            return;
        };
        for ev in events {
            if let Err(e) = sink.write_event(ev) {
                if !self.warned {
                    eprintln!("golemctl: event log write failed: {e}");
                    self.warned = true;
                }
                self.sink = None;
                return;
            }
        }
    }
}
