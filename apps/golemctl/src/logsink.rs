use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::poll::{Event, EventKind};

// The greppable on-disk record of one apply (ADR 0033 §3a). Every event line is
// appended to a combined `all.log` and to its unit_path's own slugged file, so a
// run can be read whole or one scroll at a time after the spinner clears. IO
// errors degrade to a single warning and never abort the apply — the files are a
// record, not a dependency of the reconcile.
//
// The files are the after-the-fact record devenv-tui drops on exit, so they are
// left on disk deliberately: no rotation, no cleanup — $TMPDIR is the lifecycle,
// and one apply's log is small.
pub struct LogSink {
    dir: PathBuf,
    all: File,
}

fn apply_root() -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("golemctl")
}

pub fn apply_dir(reconcile_id: u64) -> PathBuf {
    apply_root().join(format!("apply-{reconcile_id}"))
}

// One host's dir in a fleet fan-out (ADR 0038). A reconcile id is per-daemon,
// so hosts applying concurrently routinely draw the same one — the slugged host
// name is the only thing keeping their transcripts apart.
pub fn host_apply_dir(host: &str, reconcile_id: u64) -> PathBuf {
    apply_root().join(format!("apply-{}-{reconcile_id}", slug_of(host)))
}

// A unit_path is one flat filename: segments joined by `-`, each character
// outside `[A-Za-z0-9._-]` replaced with `_` (`slug_of`), suffixed `.log`
// (ADR 0033 §3a). `["<removes>"]` becomes `_removes_.log`; the empty
// (host-root) path becomes `host.log`.
pub fn unit_file_name(unit_path: &[String]) -> String {
    if unit_path.is_empty() {
        return "host.log".into();
    }
    format!("{}.log", slug_of(&unit_path.join("-")))
}

fn slug_of(text: &str) -> String {
    text.chars()
        .map(|c| if is_slug_safe(c) { c } else { '_' })
        .collect()
}

fn is_slug_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn kind_tag(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Lifecycle => "lifecycle",
        EventKind::Cmd => "cmd",
    }
}

pub fn event_line(ev: &Event) -> String {
    // The `kind` column (ADR 0033 §3a) so a `grep cmd`/`grep lifecycle` (or an
    // awk on the column) separates the interleaved streams after the fact — one
    // ordered transcript with a filter column, not two files.
    format!(
        "{}  {}  {}  {}  {}  {}",
        ev.at,
        ev.level,
        kind_tag(ev.kind),
        ev.unit_path.join("/"),
        ev.glyph_key,
        ev.message
    )
}

impl LogSink {
    pub fn create(reconcile_id: u64) -> Result<Self, std::io::Error> {
        Self::create_at(apply_dir(reconcile_id))
    }

    pub fn create_at(dir: PathBuf) -> Result<Self, std::io::Error> {
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
// goes quiet for the rest of the run. `open` derives the dir from a reconcile
// id; `open_at` takes one the caller chose, which is how a fleet run gets a dir
// per host (`host_apply_dir`).
pub struct Persistence {
    sink: Option<LogSink>,
    warned: bool,
}

impl Persistence {
    pub fn open(reconcile_id: u64) -> Self {
        Self::from_sink(LogSink::create(reconcile_id))
    }

    pub fn open_at(dir: PathBuf) -> Self {
        Self::from_sink(LogSink::create_at(dir))
    }

    fn from_sink(sink: Result<LogSink, std::io::Error>) -> Self {
        match sink {
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
