use std::fs;
use std::sync::Mutex;

use golemctl::logsink::{unit_file_name, LogSink, Persistence};
use golemctl::poll::{Event, EventKind};

// TMPDIR is process-global; tests that set it must not run concurrently.
static TMPDIR_LOCK: Mutex<()> = Mutex::new(());

fn event(unit: &[&str], key: &str, level: &str, msg: &str) -> Event {
    tagged(unit, key, level, EventKind::Lifecycle, msg)
}

fn cmd_event(unit: &[&str], key: &str, level: &str, msg: &str) -> Event {
    tagged(unit, key, level, EventKind::Cmd, msg)
}

fn tagged(unit: &[&str], key: &str, level: &str, kind: EventKind, msg: &str) -> Event {
    Event {
        seq: 1,
        at: "2026-07-26T14:03:11Z".into(),
        level: level.into(),
        kind,
        unit_path: unit.iter().map(|s| s.to_string()).collect(),
        glyph_key: key.into(),
        message: msg.into(),
    }
}

// Holds the TMPDIR lock for the whole test, creates a private temp dir under the
// *real* system temp (captured before TMPDIR is overridden — `tempfile::tempdir`
// reads TMPDIR itself), and points TMPDIR at it. Serializing all of this behind
// the lock keeps concurrent tests from nesting their temp dirs inside each other.
struct TmpdirGuard {
    dir: tempfile::TempDir,
    prev: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TmpdirGuard {
    fn new() -> Self {
        let lock = TMPDIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::Builder::new()
            .tempdir_in(std::env::temp_dir())
            .unwrap();
        let prev = std::env::var_os("TMPDIR");
        std::env::set_var("TMPDIR", dir.path());
        Self {
            dir,
            prev,
            _lock: lock,
        }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

impl Drop for TmpdirGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("TMPDIR", v),
            None => std::env::remove_var("TMPDIR"),
        }
    }
}

#[test]
fn a_unit_path_slugs_to_one_flat_filename() {
    assert_eq!(
        unit_file_name(&["scaly".into(), "fishnet-a".into()]),
        "scaly-fishnet-a.log"
    );
}

#[test]
fn the_removes_group_slugs_the_angle_brackets() {
    assert_eq!(unit_file_name(&["<removes>".into()]), "_removes_.log");
}

#[test]
fn the_host_root_path_is_host_log() {
    assert_eq!(unit_file_name(&[]), "host.log");
}

#[test]
fn events_land_in_all_log_and_their_units_slug_file() {
    let guard = TmpdirGuard::new();

    let mut sink = LogSink::create(42).unwrap();
    sink.write_event(&event(&["scaly", "a"], "apt:podman", "info", "install apt:podman"))
        .unwrap();
    sink.write_event(&event(&["<removes>"], "apt:old", "info", "remove apt:old"))
        .unwrap();
    sink.write_event(&event(&["scaly", "a"], "apt:podman", "warn", "dpkg lock held"))
        .unwrap();

    let dir = guard.path().join("golemctl").join("apply-42");
    let all = fs::read_to_string(dir.join("all.log")).unwrap();
    assert_eq!(all.lines().count(), 3);
    assert!(all.contains("install apt:podman"));
    assert!(all.contains("remove apt:old"));

    let unit = fs::read_to_string(dir.join("scaly-a.log")).unwrap();
    assert_eq!(unit.lines().count(), 2);
    assert!(unit.contains("install apt:podman"));
    assert!(unit.contains("dpkg lock held"));
    assert!(!unit.contains("remove apt:old"));

    let removes = fs::read_to_string(dir.join("_removes_.log")).unwrap();
    assert_eq!(removes.lines().count(), 1);
    assert!(removes.contains("remove apt:old"));
}

#[test]
fn an_event_line_carries_timestamp_level_glyph_and_message() {
    let guard = TmpdirGuard::new();
    let mut sink = LogSink::create(7).unwrap();
    sink.write_event(&event(
        &["scaly", "a"],
        "apt:podman",
        "warn",
        "enact failed (round 1): dpkg lock held; retrying in 2s",
    ))
    .unwrap();
    let line = fs::read_to_string(guard.path().join("golemctl").join("apply-7").join("all.log"))
        .unwrap();
    assert!(line.contains("2026-07-26T14:03:11Z"));
    assert!(line.contains("warn"));
    assert!(line.contains("apt:podman"));
    assert!(line.contains("enact failed (round 1)"));
}

#[test]
fn both_kinds_interleave_in_the_unit_file_with_a_kind_column() {
    let guard = TmpdirGuard::new();
    let mut sink = LogSink::create(11).unwrap();
    sink.write_event(&cmd_event(
        &["scaly", "a"],
        "apt:podman",
        "info",
        "Unpacking podman (4.3.1) ...",
    ))
    .unwrap();
    sink.write_event(&event(
        &["scaly", "a"],
        "apt:podman",
        "warn",
        "enact failed (round 1): dpkg lock held; retrying in 2s",
    ))
    .unwrap();

    let dir = guard.path().join("golemctl").join("apply-11");
    let unit = fs::read_to_string(dir.join("scaly-a.log")).unwrap();
    let unit_lines: Vec<&str> = unit.lines().collect();
    assert_eq!(unit_lines.len(), 2, "both kinds land in the unit file");
    assert!(unit_lines[0].contains("cmd"));
    assert!(unit_lines[0].contains("Unpacking podman"));
    assert!(unit_lines[1].contains("lifecycle"));
    assert!(unit_lines[1].contains("enact failed"));

    let all = fs::read_to_string(dir.join("all.log")).unwrap();
    assert!(all.contains("  cmd  "), "all.log carries the kind column");
    assert!(all.contains("  lifecycle  "));
}

#[test]
fn persistence_degrades_when_the_dir_cannot_be_created() {
    let guard = TmpdirGuard::new();
    let blocker = guard.path().join("golemctl");
    fs::write(&blocker, b"not a directory").unwrap();

    let mut persistence = Persistence::open(99);
    assert!(persistence.dir().is_none());
    persistence.persist(&[event(&["scaly", "a"], "apt:podman", "info", "install")]);
}
