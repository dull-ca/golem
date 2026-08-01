//! The apply loop: fire the manifest, then poll until settled (ADR 0033 §3).
//! [`run`] picks the surface — the live unit tree on a TTY, plain lines
//! otherwise. `--json` and a non-terminal stdout both take the plain path (a
//! pipe or CI gets deterministic lines, no spinner); `--reattach` skips the
//! POST and resumes the newest attempt via `/reconciles/latest`.
//!
//! Exit-code contract: `settled` → 0, any other terminal `outcome`
//! (`partial`, `rolled_back`) → nonzero. A partial or rolled-back reconcile is
//! a *result*, not a transport error (ADR 0029 §5), so the report still prints;
//! the nonzero code lets a caller (fleet, CI) branch on it.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use iocraft::prelude::*;
use tokio::sync::Notify;

use crate::conn::Conn;
use crate::logsink::Persistence;
use crate::model::ApplyModel;
use crate::poll::{Event, Progress};
use crate::view::UnitTree;

pub fn plain_line(ev: &Event) -> String {
    format!(
        "[{}] {}  {}: {}",
        ev.level,
        ev.unit_path.join(" / "),
        ev.glyph_key,
        ev.message
    )
}

pub fn should_stop(p: &Progress) -> bool {
    p.phase.is_terminal()
}

fn first_line_of(message: &str) -> String {
    let mut lines = message.lines().filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next() else {
        return String::new();
    };
    if lines.next().is_some() {
        format!("{first}…")
    } else {
        first.to_string()
    }
}

pub fn summarize_report(report: &serde_json::Value) -> Vec<String> {
    let outcome = report
        .get("outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown");
    let revision = report
        .get("revision")
        .and_then(|r| r.get("id"))
        .and_then(|id| id.as_u64());

    let mut headline = match revision {
        Some(id) => format!("apply {outcome} — revision {id}"),
        None => format!("apply {outcome}"),
    };

    let units = report
        .get("units")
        .and_then(|u| u.as_array())
        .cloned()
        .unwrap_or_default();

    let mut unit_segments = Vec::new();
    let mut detail_lines = Vec::new();
    for unit in &units {
        let failures = unit
            .get("failures")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        if failures.is_empty() {
            continue;
        }
        let unit_path = unit
            .get("unit_path")
            .and_then(|p| p.as_array())
            .map(|segs| {
                segs.iter()
                    .filter_map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default();
        let disposition = match unit.get("outcome").and_then(|o| o.as_str()) {
            Some("rolled_back") => "rolled back",
            _ => "kept",
        };
        let noun = if failures.len() == 1 {
            "glyph"
        } else {
            "glyphs"
        };
        unit_segments.push(format!(
            "{unit_path}: {} {noun} failed ({disposition})",
            failures.len()
        ));

        for failure in &failures {
            let glyph_key = failure
                .get("glyph_key")
                .and_then(|k| k.as_str())
                .unwrap_or("?");
            let message = failure
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("");
            let message = first_line_of(message);
            let class = failure.get("class").and_then(|c| c.as_str());
            let mut line = format!("  {unit_path} / {glyph_key}: {message}");
            if let Some(class) = class {
                line.push_str(&format!(" ({class})"));
            }
            detail_lines.push(line);
        }
    }

    if !unit_segments.is_empty() {
        headline.push_str(" — ");
        headline.push_str(&unit_segments.join("; "));
    }

    let mut lines = vec![headline];
    lines.extend(detail_lines);
    lines
}

// `settled` and a settle with no report both exit 0; every other terminal
// outcome (`partial`, `rolled_back`) exits 1. See the module contract.
fn exit_code(report: Option<&serde_json::Value>) -> i32 {
    match report
        .and_then(|r| r.get("outcome"))
        .and_then(|o| o.as_str())
    {
        Some("settled") => 0,
        None => 0,
        _ => 1,
    }
}

pub async fn run(bytes: Vec<u8>, conn: Conn, json: bool, reattach: bool) -> Result<()> {
    let id = if reattach {
        conn.get_latest(0).await?.reconcile_id
    } else {
        conn.post_manifest(bytes).await?.reconcile_id
    };
    let code = if !json && std::io::stdout().is_terminal() {
        run_tui(conn, id).await?
    } else {
        run_plain(&conn, id, json).await?
    };
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

async fn run_plain(conn: &Conn, id: u64, json: bool) -> Result<i32> {
    let mut persistence = Persistence::open(id);
    if let Some(dir) = persistence.dir() {
        let line = format!("logs: {}/", dir.display());
        if json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    let mut cursor = 0u64;
    loop {
        let p = conn.get_progress(id, cursor).await?;
        cursor = p.cursor;
        persistence.persist(&p.events);
        for ev in &p.events {
            let line = plain_line(ev);
            if json {
                eprintln!("{line}");
            } else {
                println!("{line}");
            }
        }
        if should_stop(&p) {
            if let Some(report) = &p.report {
                if json {
                    println!("{report}");
                } else {
                    for line in summarize_report(report) {
                        println!("{line}");
                    }
                }
            }
            return Ok(exit_code(p.report.as_ref()));
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

// The shared handle the mounted view and the poll task both hold: the folded
// model behind a lock, a `done` flag the view watches to exit, an `error` slot
// the poll task fills on a transport failure (so `run_tui` can propagate it
// after unmount instead of the loop silently reading as a clean settle), and a
// `Notify` the poll task rings after each fold to wake a fresh frame —
// devenv's model-behind-a-lock + notify pattern (`devenv-tui::app`), minimally
// adapted.
#[derive(Clone)]
struct Live {
    model: Arc<Mutex<ApplyModel>>,
    done: Arc<AtomicBool>,
    error: Arc<Mutex<Option<anyhow::Error>>>,
    notify: Arc<Notify>,
}

// The mounted live loop: iocraft's inline `render_loop` on stderr hosting the
// `UnitTree`, whose spinner marks self-animate on their own ~80ms timer while a
// separate tokio task polls golemd every ~1s and folds each `Progress` into the
// shared model. The network poll runs on the real tokio runtime — not inside an
// iocraft hook, which iocraft's own executor would not drive to completion — and
// rings `notify` after each fold; the view's `use_future` waits on that (or a
// short timeout) and bumps a redraw `State` so the new data re-reads the lock.
// The spinner ticks independently in between. `render_loop` is inline (no
// alternate screen, no full-screen clear) — it line-diffs each frame, preserving
// scrollback — and takes its width from the terminal, so nothing here hardcodes
// a column count. The poll task sets `done` on the terminal phase and rings
// `notify`; the view then calls `system.exit()`, so the loop unmounts on its own
// frame boundary and the final report prints to stdout after unmount.
// Smoke-verified against a fake-reconciler golemd (spinner frames change between
// samples while a glyph is in flight); the fold, `render_to_string`,
// `plain_line`, `should_stop`, and `exit_code` stay the unit-tested surface.
async fn run_tui(conn: Conn, id: u64) -> Result<i32> {
    let mut model = ApplyModel::new();
    let persistence = Persistence::open(id);
    model.log_dir = persistence.dir().map(|d| d.to_path_buf());
    let log_dir = model.log_dir.clone();

    let live = Live {
        model: Arc::new(Mutex::new(model)),
        done: Arc::new(AtomicBool::new(false)),
        error: Arc::new(Mutex::new(None)),
        notify: Arc::new(Notify::new()),
    };

    let poller = tokio::spawn(poll_into(
        live.clone(),
        conn,
        id,
        Arc::new(Mutex::new(persistence)),
    ));

    let mut element = element! {
        ContextProvider(value: Context::owned(live.clone())) {
            ApplyView
        }
    };
    element.render_loop().output(Output::Stderr).await?;
    let _ = poller.await;

    if let Some(err) = live.error.lock().ok().and_then(|mut e| e.take()) {
        return Err(err);
    }

    let report = live.model.lock().ok().and_then(|m| m.report.clone());
    if let Some(report) = &report {
        for line in summarize_report(report) {
            println!("{line}");
        }
    }
    // Reprint the log path on stdout after the report so it survives the tree
    // scrolling out of view in a long session (ADR 0033 §3b).
    if let Some(dir) = &log_dir {
        println!("logs: {}/", dir.display());
    }
    Ok(exit_code(report.as_ref()))
}

async fn poll_into(live: Live, conn: Conn, id: u64, persistence: Arc<Mutex<Persistence>>) {
    let mut cursor = 0u64;
    loop {
        let p = match conn.get_progress(id, cursor).await {
            Ok(p) => p,
            Err(err) => {
                if let Ok(mut slot) = live.error.lock() {
                    *slot = Some(err);
                }
                live.done.store(true, Ordering::Release);
                live.notify.notify_waiters();
                break;
            }
        };
        cursor = p.cursor;
        let terminal = should_stop(&p);
        if let Ok(mut sink) = persistence.lock() {
            sink.persist(&p.events);
        }
        if let Ok(mut m) = live.model.lock() {
            m.apply_progress(p);
        }
        if terminal {
            live.done.store(true, Ordering::Release);
        }
        live.notify.notify_waiters();
        if terminal {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

#[component]
fn ApplyView(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let live = hooks.use_context::<Live>().clone();
    let mut redraw = hooks.use_state(|| 0u64);

    let notify = live.notify.clone();
    hooks.use_future(async move {
        loop {
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
            let Some(v) = redraw.try_get() else { break };
            redraw.set(v.wrapping_add(1));
        }
    });

    let mut system = hooks.use_context_mut::<SystemContext>();
    if live.done.load(Ordering::Acquire) {
        system.exit();
    }

    element! {
        ContextProvider(value: Context::owned(live.model.clone())) {
            UnitTree
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poll::{Event, Phase, Progress};

    fn ev() -> Event {
        Event {
            seq: 1,
            at: "2026-07-26T00:00:00Z".into(),
            level: "warn".into(),
            kind: crate::poll::EventKind::Lifecycle,
            unit_path: vec!["scaly".into(), "canary".into()],
            glyph_key: "systemd:canary.service".into(),
            message: "enact failed (round 1)".into(),
        }
    }

    fn prog(phase: Phase) -> Progress {
        Progress {
            reconcile_id: 1,
            phase,
            units: vec![],
            events: vec![],
            cursor: 0,
            report: None,
        }
    }

    #[test]
    fn plain_line_names_level_unit_and_message() {
        let line = plain_line(&ev());
        assert!(line.contains("warn"));
        assert!(line.contains("scaly / canary"));
        assert!(line.contains("systemd:canary.service"));
        assert!(line.contains("enact failed"));
    }

    #[test]
    fn should_stop_only_on_terminal_phase() {
        assert!(!should_stop(&prog(Phase::Enacting)));
        assert!(should_stop(&prog(Phase::Settled)));
        assert!(should_stop(&prog(Phase::RolledBack)));
    }

    #[test]
    fn exit_code_is_nonzero_for_partial_outcomes() {
        assert_eq!(exit_code(None), 0);
        assert_eq!(
            exit_code(Some(&serde_json::json!({ "outcome": "settled" }))),
            0
        );
        assert_eq!(
            exit_code(Some(&serde_json::json!({ "outcome": "partial" }))),
            1
        );
        assert_eq!(
            exit_code(Some(&serde_json::json!({ "outcome": "rolled_back" }))),
            1
        );
    }

    #[test]
    fn summarize_report_names_outcome_and_revision_when_settled() {
        let report = serde_json::json!({
            "outcome": "settled",
            "revision": { "id": 3 },
            "units": []
        });
        let lines = summarize_report(&report);
        assert_eq!(lines, vec!["apply settled — revision 3"]);
    }

    #[test]
    fn summarize_report_names_the_kept_unit_and_glyph_count_when_partial() {
        let report = serde_json::json!({
            "outcome": "partial",
            "revision": { "id": 2 },
            "units": [
                {
                    "unit_path": ["scaly", "canary"],
                    "outcome": "partial",
                    "glyphs": [],
                    "failures": [
                        {
                            "glyph_key": "systemd:canary.service",
                            "unit_path": ["scaly", "canary"],
                            "phase": "enact",
                            "class": "retries-exhausted",
                            "attempts": 5,
                            "message": "unit not found",
                            "rolled_back": false
                        }
                    ]
                }
            ]
        });
        let lines = summarize_report(&report);
        assert_eq!(
            lines[0],
            "apply partial — revision 2 — scaly / canary: 1 glyph failed (kept)"
        );
    }

    #[test]
    fn summarize_report_names_the_rolled_back_unit_when_rolled_back() {
        let report = serde_json::json!({
            "outcome": "rolled_back",
            "revision": { "id": 5 },
            "units": [
                {
                    "unit_path": ["scaly", "canary"],
                    "outcome": "rolled_back",
                    "glyphs": [],
                    "failures": [
                        {
                            "glyph_key": "apt:podman",
                            "unit_path": ["scaly", "canary"],
                            "phase": "enact",
                            "class": "fatal",
                            "attempts": 1,
                            "message": "mirror down",
                            "rolled_back": true
                        },
                        {
                            "glyph_key": "systemd:x.service",
                            "unit_path": ["scaly", "canary"],
                            "phase": "enact",
                            "class": "retries-exhausted",
                            "attempts": 5,
                            "message": "unit not found",
                            "rolled_back": true
                        }
                    ]
                }
            ]
        });
        let lines = summarize_report(&report);
        assert_eq!(
            lines[0],
            "apply rolled_back — revision 5 — scaly / canary: 2 glyphs failed (rolled back)"
        );
    }

    #[test]
    fn summarize_report_appends_a_failure_detail_line_with_unit_path_and_class() {
        let report = serde_json::json!({
            "outcome": "partial",
            "revision": { "id": 2 },
            "units": [
                {
                    "unit_path": ["scaly", "canary"],
                    "outcome": "partial",
                    "glyphs": [],
                    "failures": [
                        {
                            "glyph_key": "systemd:canary.service",
                            "unit_path": ["scaly", "canary"],
                            "phase": "enact",
                            "class": "retries-exhausted",
                            "attempts": 5,
                            "message": "unit not found",
                            "rolled_back": false
                        }
                    ]
                }
            ]
        });
        let lines = summarize_report(&report);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("scaly / canary"));
        assert!(lines[1].contains("systemd:canary.service"));
        assert!(lines[1].contains("unit not found"));
        assert!(lines[1].contains("retries-exhausted"));
    }

    #[test]
    fn summarize_report_collapses_a_multi_line_failure_message_to_one_line() {
        let report = serde_json::json!({
            "outcome": "partial",
            "revision": { "id": 7 },
            "units": [
                {
                    "unit_path": ["scaly", "canary"],
                    "outcome": "partial",
                    "glyphs": [],
                    "failures": [
                        {
                            "glyph_key": "systemd:fishnet-canary.service",
                            "unit_path": ["scaly", "canary"],
                            "phase": "enact",
                            "class": "retries-exhausted",
                            "attempts": 5,
                            "message": "systemctl start fishnet-canary.service: Job for fishnet-canary.service failed because the control process exited with error code.\nSee \"systemctl status fishnet-canary.service\" and \"journalctl -xeu fishnet-canary.service\" for details.\n",
                            "rolled_back": false
                        }
                    ]
                }
            ]
        });
        let lines = summarize_report(&report);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].lines().count(), 1);
        assert_eq!(
            lines[1],
            "  scaly / canary / systemd:fishnet-canary.service: systemctl start fishnet-canary.service: Job for fishnet-canary.service failed because the control process exited with error code.… (retries-exhausted)"
        );
    }

    // A transport failure (golemd crashing mid-apply, an unreachable port) must
    // surface as an error on `live`, not read as a silent settle: `run_tui`
    // reads this slot after unmount and turns it into the process's `Err`.
    #[tokio::test]
    async fn poll_into_carries_a_transport_error_instead_of_a_silent_done() {
        let live = Live {
            model: Arc::new(Mutex::new(ApplyModel::new())),
            done: Arc::new(AtomicBool::new(false)),
            error: Arc::new(Mutex::new(None)),
            notify: Arc::new(Notify::new()),
        };
        // Port 0 refuses to connect immediately — a stand-in for golemd vanishing
        // mid-poll, no live server required.
        let persistence = Arc::new(Mutex::new(Persistence::open(0)));
        let target = crate::inventory::Target {
            name: "unreachable".into(),
            addr: "http://127.0.0.1:0".into(),
        };
        let conn = Conn::open(&target, &crate::conn::AuthSource::None)
            .await
            .unwrap();

        poll_into(live.clone(), conn, 1, persistence).await;

        assert!(live.done.load(Ordering::Acquire));
        let err = live.error.lock().unwrap().take();
        assert!(
            err.is_some(),
            "expected a transport error, got a silent done"
        );
    }
}
