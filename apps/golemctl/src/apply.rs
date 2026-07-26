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
use std::time::Duration;

use anyhow::Result;

use crate::model::ApplyModel;
use crate::poll::{get_latest, get_progress, post_manifest, Event, Progress};
use crate::view;

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

pub fn print_report(report: &serde_json::Value) {
    match serde_json::to_string_pretty(report) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{report}"),
    }
}

// `settled` and a settle with no report both exit 0; every other terminal
// outcome (`partial`, `rolled_back`) exits 1. See the module contract.
fn exit_code(report: Option<&serde_json::Value>) -> i32 {
    match report.and_then(|r| r.get("outcome")).and_then(|o| o.as_str()) {
        Some("settled") => 0,
        None => 0,
        _ => 1,
    }
}

pub async fn run(bytes: Vec<u8>, addr: &str, json: bool, reattach: bool) -> Result<()> {
    let id = if reattach {
        get_latest(addr, 0).await?.reconcile_id
    } else {
        post_manifest(addr, bytes).await?.reconcile_id
    };
    let code = if !json && std::io::stdout().is_terminal() {
        run_tui(addr, id).await?
    } else {
        run_plain(addr, id, json).await?
    };
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

async fn run_plain(addr: &str, id: u64, json: bool) -> Result<i32> {
    let mut cursor = 0u64;
    loop {
        let p = get_progress(addr, id, cursor).await?;
        cursor = p.cursor;
        for ev in &p.events {
            println!("{}", plain_line(ev));
        }
        if should_stop(&p) {
            if let Some(report) = &p.report {
                if json {
                    println!("{report}");
                } else {
                    print_report(report);
                }
            }
            return Ok(exit_code(p.report.as_ref()));
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

// The string-redraw baseline: fold each poll into the model and re-render the
// pure view to stderr, clearing between frames. Animation therefore advances at
// poll cadence, not smoothly — the self-animating iocraft components exist
// (see `view::Spinner`) but are not yet mounted here. This path and `--reattach`
// are smoke-verified only (a live render loop needs a pty harness the plan does
// not build); the tested surface is the fold, `render_to_string`, `plain_line`,
// `should_stop`, and `exit_code`.
async fn run_tui(addr: &str, id: u64) -> Result<i32> {
    let mut model = ApplyModel::new();
    let mut cursor = 0u64;
    loop {
        let p = get_progress(addr, id, cursor).await?;
        cursor = p.cursor;
        let terminal = should_stop(&p);
        let report = p.report.clone();
        model.apply_progress(p);
        eprint!("\x1b[2J\x1b[H");
        eprintln!("{}", view::render_to_string(&model, 100));
        if terminal {
            if let Some(report) = &report {
                print_report(report);
            }
            return Ok(exit_code(report.as_ref()));
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
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
        assert_eq!(exit_code(Some(&serde_json::json!({ "outcome": "settled" }))), 0);
        assert_eq!(exit_code(Some(&serde_json::json!({ "outcome": "partial" }))), 1);
        assert_eq!(
            exit_code(Some(&serde_json::json!({ "outcome": "rolled_back" }))),
            1
        );
    }
}
