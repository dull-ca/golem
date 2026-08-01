//! Fan a verb out over an inventory of daemons, concurrently (ADR 0038).
//! golemd, the wire format, and the trust model are untouched: every daemon
//! already selects its own scroll from the shared manifest, so a fleet verb is
//! N single-host verbs driven from one process — one manifest compiled once,
//! each host's `Progress` folded into its own [`ApplyModel`] by the same
//! POST-then-poll loop [`crate::apply`] runs.
//!
//! **Per-host isolation.** Each target runs in its own task. A transport
//! failure, a 409 from a daemon already reconciling, or a rolled-back unit
//! stops that host alone; every other host runs to its terminal phase, and
//! every host is reported.
//!
//! **Outcome taxonomy** ([`HostOutcome`]):
//! - `Settled` — the daemon reported `settled` (or settled carrying no report).
//! - `Unsettled` — a terminal report of `partial` or `rolled_back`. A result,
//!   not a transport error (ADR 0029 §5), so the report still prints.
//! - `Skipped` — the manifest names no scroll for this host; see below.
//! - `Error` — nothing was learned: the POST failed, a poll failed, or the
//!   host's task died.
//!
//! **Exit codes.** `fleet apply` exits 0 iff every host is `Settled` or
//! `Skipped`, 1 otherwise ([`fleet_exit_code`]) — the same 0-means-settled
//! contract single-host `apply` keeps, aggregated. `fleet plan` exits 0 unless
//! a host errored: a diff is not a failure. `fleet status` always exits 0,
//! unreachable hosts included — a status is an observation, not an assertion.
//!
//! **Absence is silence.** [`Fanout`] decodes the manifest before any host is
//! contacted, and a target it names no scroll for is `Skipped`: never POSTed
//! to, never counted against the exit code. golemd resolves a host absent from
//! a manifest to the *empty* scroll, so fanning a partial manifest out to the
//! whole inventory would decommission every host it fails to name.
//! Decommissioning takes an explicitly authored empty scroll instead.
//! (Single-host `apply` keeps its meaning — naming one daemon is an explicit
//! order.) golemd must adopt the same rule before peer gossip ships (ADR 0039).
//!
//! **Trust.** Fan-out reaches every daemon directly from the operator's
//! machine and sends no credentials; reachability and authenticity of a golemd
//! port are the infra layer's to establish, not golem's (ADR 0040).
//!
//! Surfaces mirror [`crate::apply`]: a TTY gets one live tree, a branch per
//! host over that host's reused unit tree ([`fleet_lines`]); `--json` and a
//! non-terminal stdout take the plain path — host-prefixed lines, then either
//! per-host summaries or one `{"hosts": {…}}` aggregate.

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context as _, Result};
use iocraft::prelude::*;
use tokio::sync::Notify;

use crate::apply;
use crate::conn::{AuthSource, Conn};
use crate::inventory::Target;
use crate::logsink::{host_apply_dir, Persistence};
use crate::model::ApplyModel;
use crate::plan::{self, paint, RenderOptions, BOLD, DIM, GREEN, RED};
use crate::view::{self, Emphasis, Line};

const POLL_INTERVAL: Duration = Duration::from_millis(1000);
const FRAME_INTERVAL: Duration = Duration::from_millis(200);
const CONTENT_ID_PREFIX_CHARS: usize = 12;

pub const SKIPPED_NOTE: &str = "skipped — no scroll in manifest";

// The targets paired with the scroll names the manifest carries, read once and
// consulted before any host is contacted — the "absence is silence" gate of the
// module contract. Decoding up front also means undecodable bytes fail while
// every daemon is still untouched.
#[derive(Debug, Clone)]
pub struct Fanout {
    targets: Vec<Target>,
    scroll_names: BTreeSet<String>,
}

impl Fanout {
    pub fn read(manifest_bytes: &[u8], targets: Vec<Target>) -> Result<Self> {
        let manifest = scroll_format::from_bytes(manifest_bytes)
            .map_err(|e| anyhow!("{e}"))
            .context("decode the manifest before fanning it out")?;
        Ok(Fanout {
            targets,
            scroll_names: manifest
                .scrolls
                .iter()
                .map(|addressed| addressed.scroll.name.clone())
                .collect(),
        })
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    pub fn carries_a_scroll_for(&self, target: &Target) -> bool {
        self.scroll_names.contains(&target.name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostOutcome {
    Settled { report: Option<serde_json::Value> },
    Unsettled { report: serde_json::Value },
    Skipped,
    Error { message: String },
}

impl HostOutcome {
    pub fn report(&self) -> Option<&serde_json::Value> {
        match self {
            HostOutcome::Settled { report } => report.as_ref(),
            HostOutcome::Unsettled { report } => Some(report),
            HostOutcome::Skipped | HostOutcome::Error { .. } => None,
        }
    }

    pub fn is_settled(&self) -> bool {
        matches!(self, HostOutcome::Settled { .. })
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, HostOutcome::Skipped)
    }
}

// A terminal poll carrying no report reads as settled — the same reading
// single-host `apply::exit_code` gives it, so one host's exit contract and the
// fleet's stay in step.
pub fn outcome_of(report: Option<serde_json::Value>) -> HostOutcome {
    let Some(report) = report else {
        return HostOutcome::Settled { report: None };
    };
    match outcome_name(&report) {
        "settled" => HostOutcome::Settled {
            report: Some(report),
        },
        _ => HostOutcome::Unsettled { report },
    }
}

fn outcome_name(report: &serde_json::Value) -> &str {
    report
        .get("outcome")
        .and_then(|o| o.as_str())
        .unwrap_or("unknown")
}

fn transport_failure(err: anyhow::Error) -> HostOutcome {
    HostOutcome::Error {
        message: format!("{err:#}"),
    }
}

pub fn prefixed(host: &str, line: &str) -> String {
    format!("[{host}] {line}")
}

pub fn fleet_exit_code(results: &[(Target, HostOutcome)]) -> i32 {
    if results
        .iter()
        .all(|(_, outcome)| outcome.is_settled() || outcome.is_skipped())
    {
        0
    } else {
        1
    }
}

const BODY_INDENT: &str = "  ";

fn host_heading(target: &Target, color: bool) -> String {
    format!(
        "{}  {}",
        paint(&target.name, BOLD, color),
        paint(&target.addr, DIM, color)
    )
}

fn skipped_note(color: bool) -> String {
    format!("{BODY_INDENT}{}", paint(SKIPPED_NOTE, DIM, color))
}

fn error_note(message: &str, color: bool) -> String {
    format!(
        "{BODY_INDENT}{} {}",
        paint("error:", RED, color),
        concise_error(message)
    )
}

/// Collapse an anyhow `{:#}` chain to its two informative ends — the outermost
/// context and the root cause — for a per-host line an operator scans. A fan-out
/// prints one of these per failing host, and the middle of the chain is the same
/// plumbing on every one of them. `--json` carries the chain verbatim, so nothing
/// is lost, only shortened.
fn concise_error(message: &str) -> String {
    let segments: Vec<&str> = message.split(": ").collect();
    match (segments.first(), segments.last()) {
        (Some(context), Some(cause)) if segments.len() > 2 => format!("{context} — {cause}"),
        _ => message.to_string(),
    }
}

fn under_heading(line: &str) -> String {
    format!("{BODY_INDENT}{line}")
}

pub fn summary_lines(results: &[(Target, HostOutcome)], color: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for (target, outcome) in results {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(host_heading(target, color));
        match outcome {
            HostOutcome::Skipped => lines.push(skipped_note(color)),
            HostOutcome::Error { message } => lines.push(error_note(message, color)),
            _ => match outcome.report() {
                Some(report) => lines.extend(
                    apply::summarize_report(report)
                        .iter()
                        .map(|line| under_heading(line)),
                ),
                None => lines.push(under_heading("apply settled")),
            },
        }
    }
    lines
}

pub fn apply_json(results: &[(Target, HostOutcome)]) -> serde_json::Value {
    let mut hosts = serde_json::Map::new();
    for (target, outcome) in results {
        let entry = match outcome {
            HostOutcome::Skipped => serde_json::json!({ "skipped": true }),
            HostOutcome::Error { message } => serde_json::json!({ "error": message }),
            HostOutcome::Settled { report } => serde_json::json!({
                "outcome": "settled",
                "report": report,
            }),
            HostOutcome::Unsettled { report } => serde_json::json!({
                "outcome": outcome_name(report),
                "report": report,
            }),
        };
        hosts.insert(target.name.clone(), entry);
    }
    serde_json::json!({ "hosts": hosts })
}

pub async fn run_apply(bytes: Vec<u8>, targets: Vec<Target>, json: bool) -> Result<()> {
    let fanout = Fanout::read(&bytes, targets)?;
    let auth = crate::conn::resolve_auth(None)?;
    let results = if !json && std::io::stdout().is_terminal() {
        apply_live(bytes, &fanout, &auth).await?
    } else {
        apply_plain(bytes, &fanout, &auth, json).await
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&apply_json(&results))?);
    } else {
        for line in summary_lines(&results, plan::color_is_welcome()) {
            println!("{line}");
        }
    }
    let code = fleet_exit_code(&results);
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

pub async fn apply_plain(
    bytes: Vec<u8>,
    fanout: &Fanout,
    auth: &AuthSource,
    json: bool,
) -> Vec<(Target, HostOutcome)> {
    let mut tasks = Vec::with_capacity(fanout.targets().len());
    for target in fanout.targets() {
        if !fanout.carries_a_scroll_for(target) {
            emit(json, &prefixed(&target.name, SKIPPED_NOTE));
            tasks.push((target.clone(), HostTask::Skipped));
            continue;
        }
        let target = target.clone();
        let bytes = bytes.clone();
        let auth = auth.clone();
        tasks.push((
            target.clone(),
            HostTask::Running(tokio::spawn(async move {
                apply_host_plain(&target, bytes, &auth, json).await
            })),
        ));
    }
    join_host_tasks(tasks).await
}

enum HostTask {
    Running(tokio::task::JoinHandle<HostOutcome>),
    Skipped,
}

async fn join_host_tasks(tasks: Vec<(Target, HostTask)>) -> Vec<(Target, HostOutcome)> {
    let mut results = Vec::with_capacity(tasks.len());
    for (target, task) in tasks {
        let outcome = match task {
            HostTask::Skipped => HostOutcome::Skipped,
            HostTask::Running(handle) => handle.await.unwrap_or_else(|e| HostOutcome::Error {
                message: format!("the host task ended abnormally: {e}"),
            }),
        };
        results.push((target, outcome));
    }
    results
}

async fn apply_host_plain(
    target: &Target,
    bytes: Vec<u8>,
    auth: &AuthSource,
    json: bool,
) -> HostOutcome {
    let conn = match Conn::open(target, auth).await {
        Ok(conn) => conn,
        Err(err) => return transport_failure(err),
    };
    let id = match conn.post_manifest(bytes).await {
        Ok(accepted) => accepted.reconcile_id,
        Err(err) => return transport_failure(err),
    };
    let mut persistence = Persistence::open_at(host_apply_dir(&target.name, id));
    if let Some(dir) = persistence.dir() {
        emit(
            json,
            &prefixed(&target.name, &format!("logs: {}/", dir.display())),
        );
    }
    let mut cursor = 0u64;
    loop {
        let progress = match conn.get_progress(id, cursor).await {
            Ok(progress) => progress,
            Err(err) => return transport_failure(err),
        };
        cursor = progress.cursor;
        persistence.persist(&progress.events);
        for event in &progress.events {
            emit(json, &prefixed(&target.name, &apply::plain_line(event)));
        }
        if progress.phase.is_terminal() {
            return outcome_of(progress.report);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// In `--json` mode stdout carries exactly the final aggregate object, so the
// running commentary goes to stderr instead — the split single-host `apply
// --json` already makes, kept so a fleet run pipes as cleanly.
fn emit(json: bool, line: &str) {
    if json {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostStatus {
    InFlight,
    Settled,
    RolledBack,
    Failed,
    Skipped,
}

impl HostStatus {
    pub fn of(outcome: &HostOutcome) -> Self {
        match outcome {
            HostOutcome::Settled { .. } => HostStatus::Settled,
            HostOutcome::Unsettled { report } => match outcome_name(report) {
                "rolled_back" => HostStatus::RolledBack,
                _ => HostStatus::Failed,
            },
            HostOutcome::Skipped => HostStatus::Skipped,
            HostOutcome::Error { .. } => HostStatus::Failed,
        }
    }
}

pub fn host_mark(status: HostStatus) -> &'static str {
    match status {
        HostStatus::InFlight => view::SPINNER_FRAMES[0],
        HostStatus::Settled => view::CHECKMARK,
        HostStatus::RolledBack => view::ROLLED_BACK,
        HostStatus::Failed => view::XMARK,
        HostStatus::Skipped => view::UNCHANGED,
    }
}

#[derive(Debug, Clone)]
pub struct HostProgress {
    pub name: String,
    pub model: ApplyModel,
    pub status: HostStatus,
    pub note: Option<String>,
}

impl HostProgress {
    pub fn in_flight(target: &Target) -> Self {
        HostProgress {
            name: target.name.clone(),
            model: ApplyModel::new(),
            status: HostStatus::InFlight,
            note: None,
        }
    }

    pub fn skipped(target: &Target) -> Self {
        HostProgress {
            name: target.name.clone(),
            model: ApplyModel::new(),
            status: HostStatus::Skipped,
            note: Some(SKIPPED_NOTE.to_string()),
        }
    }
}

pub fn fleet_lines(hosts: &[HostProgress]) -> Vec<Line> {
    let mut lines = Vec::new();
    for host in hosts {
        let in_flight = host.status == HostStatus::InFlight;
        lines.push(Line::Branch {
            depth: 0,
            label: host.name.clone(),
            active: in_flight,
            mark: host_mark(host.status),
            settled: false,
            emphasis: if in_flight {
                Emphasis::Primary
            } else {
                Emphasis::Done
            },
        });
        if let Some(note) = &host.note {
            lines.push(Line::Plain {
                text: format!("  {note}"),
            });
        }
        lines.extend(view::lines(&host.model).into_iter().map(under_host));
    }
    lines
}

// Push one host's unit-tree lines a level in, under its heading. `Line::Plain`
// carries no depth — it is the `logs:` header and the skip note — so it is
// indented in its text instead.
fn under_host(line: Line) -> Line {
    match line {
        Line::Branch {
            depth,
            label,
            active,
            mark,
            settled,
            emphasis,
        } => Line::Branch {
            depth: depth + 1,
            label,
            active,
            mark,
            settled,
            emphasis,
        },
        Line::Glyph {
            depth,
            row,
            settled,
            emphasis,
        } => Line::Glyph {
            depth: depth + 1,
            row,
            settled,
            emphasis,
        },
        Line::CmdTail { depth, text } => Line::CmdTail {
            depth: depth + 1,
            text,
        },
        Line::Log { depth, text, host } => Line::Log {
            depth: depth + 1,
            text,
            host,
        },
        Line::Plain { text } => Line::Plain {
            text: format!("  {text}"),
        },
    }
}

// The shared handle the mounted view and every host task hold: apply.rs's
// model-behind-a-lock + `Notify` pattern widened to one `HostProgress` per
// target, and without its error slot — a transport failure here is that host's
// `Error` outcome, never the process's `Err`, so the remaining hosts keep
// drawing and keep their own results.
#[derive(Clone)]
struct FleetLive {
    hosts: Arc<Mutex<Vec<HostProgress>>>,
    done: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl FleetLive {
    fn settle(&self, index: usize, outcome: &HostOutcome) {
        if let Ok(mut hosts) = self.hosts.lock() {
            hosts[index].status = HostStatus::of(outcome);
            if let HostOutcome::Error { message } = outcome {
                hosts[index].note = Some(message.clone());
            }
        }
        self.notify.notify_waiters();
    }
}

pub async fn apply_live(
    bytes: Vec<u8>,
    fanout: &Fanout,
    auth: &AuthSource,
) -> Result<Vec<(Target, HostOutcome)>> {
    let live = FleetLive {
        hosts: Arc::new(Mutex::new(
            fanout
                .targets()
                .iter()
                .map(|target| {
                    if fanout.carries_a_scroll_for(target) {
                        HostProgress::in_flight(target)
                    } else {
                        HostProgress::skipped(target)
                    }
                })
                .collect(),
        )),
        done: Arc::new(AtomicBool::new(false)),
        notify: Arc::new(Notify::new()),
    };

    let mut tasks = Vec::with_capacity(fanout.targets().len());
    for (index, target) in fanout.targets().iter().enumerate() {
        if !fanout.carries_a_scroll_for(target) {
            tasks.push((target.clone(), HostTask::Skipped));
            continue;
        }
        let live = live.clone();
        let target = target.clone();
        let bytes = bytes.clone();
        let auth = auth.clone();
        tasks.push((
            target.clone(),
            HostTask::Running(tokio::spawn(async move {
                apply_host_live(live, index, target, bytes, auth).await
            })),
        ));
    }

    // Joining the host tasks from a task of its own is what lets the view keep
    // drawing: `render_loop` owns this task until `done` flips, and `done` only
    // flips once every host has reached a terminal outcome.
    let watcher = tokio::spawn({
        let live = live.clone();
        async move {
            let results = join_host_tasks(tasks).await;
            live.done.store(true, Ordering::Release);
            live.notify.notify_waiters();
            results
        }
    });

    let mut element = element! {
        ContextProvider(value: Context::owned(live.clone())) {
            FleetView
        }
    };
    element.render_loop().output(Output::Stderr).await?;
    watcher.await.context("await the fleet apply tasks")
}

async fn apply_host_live(
    live: FleetLive,
    index: usize,
    target: Target,
    bytes: Vec<u8>,
    auth: AuthSource,
) -> HostOutcome {
    let conn = match Conn::open(&target, &auth).await {
        Ok(conn) => conn,
        Err(err) => {
            let outcome = transport_failure(err);
            live.settle(index, &outcome);
            return outcome;
        }
    };
    let id = match conn.post_manifest(bytes).await {
        Ok(accepted) => accepted.reconcile_id,
        Err(err) => {
            let outcome = transport_failure(err);
            live.settle(index, &outcome);
            return outcome;
        }
    };
    let mut persistence = Persistence::open_at(host_apply_dir(&target.name, id));
    let log_dir = persistence.dir().map(|d| d.to_path_buf());
    if let Ok(mut hosts) = live.hosts.lock() {
        hosts[index].model.log_dir = log_dir;
    }
    live.notify.notify_waiters();

    let mut cursor = 0u64;
    loop {
        let progress = match conn.get_progress(id, cursor).await {
            Ok(progress) => progress,
            Err(err) => {
                let outcome = transport_failure(err);
                live.settle(index, &outcome);
                return outcome;
            }
        };
        cursor = progress.cursor;
        let terminal = progress.phase.is_terminal();
        let report = progress.report.clone();
        persistence.persist(&progress.events);
        if let Ok(mut hosts) = live.hosts.lock() {
            hosts[index].model.apply_progress(progress);
        }
        if terminal {
            let outcome = outcome_of(report);
            live.settle(index, &outcome);
            return outcome;
        }
        live.notify.notify_waiters();
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[component]
fn FleetView(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let live = hooks.use_context::<FleetLive>().clone();
    let mut redraw = hooks.use_state(|| 0u64);

    let notify = live.notify.clone();
    hooks.use_future(async move {
        loop {
            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(FRAME_INTERVAL) => {}
            }
            let Some(v) = redraw.try_get() else { break };
            redraw.set(v.wrapping_add(1));
        }
    });

    let (raw_width, raw_height) = hooks.use_terminal_size();

    let mut system = hooks.use_context_mut::<SystemContext>();
    if live.done.load(Ordering::Acquire) {
        system.exit();
    }

    let (width, height) = view::resolve_terminal_size(raw_width, raw_height);
    let budget = (height as usize).saturating_sub(1);
    let rows: Vec<AnyElement<'static>> = match live.hosts.lock() {
        Ok(hosts) => view::fit(fleet_lines(&hosts), budget)
            .iter()
            .map(view::animated_line)
            .collect(),
        Err(_) => Vec::new(),
    };

    element! {
        View(width: width, flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostPlan {
    Report(String),
    Skipped,
    Error(String),
}

pub async fn run_plan(
    bytes: Vec<u8>,
    targets: Vec<Target>,
    json: bool,
    detail: bool,
) -> Result<()> {
    let fanout = Fanout::read(&bytes, targets)?;
    let auth = crate::conn::resolve_auth(None)?;
    let results = gather_plans(bytes, &fanout, &auth).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan_json(&results))?);
    } else {
        let options = RenderOptions {
            detail,
            color: plan::color_is_welcome(),
            width: plan::DEFAULT_WIDTH,
            nested: true,
        };
        for line in plan_lines(&results, &options) {
            println!("{line}");
        }
    }
    if results
        .iter()
        .any(|(_, plan)| matches!(plan, HostPlan::Error(_)))
    {
        std::process::exit(1);
    }
    Ok(())
}

pub async fn gather_plans(
    bytes: Vec<u8>,
    fanout: &Fanout,
    auth: &AuthSource,
) -> Vec<(Target, HostPlan)> {
    let mut tasks = Vec::with_capacity(fanout.targets().len());
    for target in fanout.targets() {
        if !fanout.carries_a_scroll_for(target) {
            tasks.push((target.clone(), None));
            continue;
        }
        let target = target.clone();
        let bytes = bytes.clone();
        let auth = auth.clone();
        tasks.push((
            target.clone(),
            Some(tokio::spawn(async move {
                let conn = Conn::open(&target, &auth).await?;
                conn.post_plan(bytes).await
            })),
        ));
    }
    let mut results = Vec::with_capacity(tasks.len());
    for (target, task) in tasks {
        let plan = match task {
            None => HostPlan::Skipped,
            Some(task) => match task.await {
                Ok(Ok(body)) => HostPlan::Report(body),
                Ok(Err(err)) => HostPlan::Error(format!("{err:#}")),
                Err(err) => HostPlan::Error(format!("the host task ended abnormally: {err}")),
            },
        };
        results.push((target, plan));
    }
    results
}

pub fn plan_lines(results: &[(Target, HostPlan)], options: &RenderOptions) -> Vec<String> {
    let sectioned = RenderOptions {
        nested: true,
        ..*options
    };
    let mut lines = Vec::new();
    for (target, plan) in results {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(host_heading(target, options.color));
        match plan {
            HostPlan::Report(body) => match plan::present(body, false, &sectioned) {
                Ok(text) => lines.extend(text.lines().map(|line| line.to_string())),
                Err(err) => lines.push(error_note(&format!("{err:#}"), options.color)),
            },
            HostPlan::Skipped => lines.push(skipped_note(options.color)),
            HostPlan::Error(message) => lines.push(error_note(message, options.color)),
        }
    }
    lines
}

pub fn plan_json(results: &[(Target, HostPlan)]) -> serde_json::Value {
    let mut hosts = serde_json::Map::new();
    for (target, plan) in results {
        let entry = match plan {
            HostPlan::Report(body) => serde_json::from_str(body)
                .unwrap_or_else(|_| serde_json::json!({ "error": "unreadable plan response" })),
            HostPlan::Skipped => serde_json::json!({ "skipped": true }),
            HostPlan::Error(message) => serde_json::json!({ "error": message }),
        };
        hosts.insert(target.name.clone(), entry);
    }
    serde_json::json!({ "hosts": hosts })
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostReading {
    pub host: Option<String>,
    pub latest_revision: Option<u64>,
    pub content_id: Option<String>,
}

pub async fn run_status(targets: Vec<Target>, json: bool) -> Result<()> {
    let auth = crate::conn::resolve_auth(None)?;
    let readings = gather_status(&targets, &auth).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&status_json(&readings))?);
    } else {
        for line in status_lines(&readings, plan::color_is_welcome()) {
            println!("{line}");
        }
    }
    Ok(())
}

pub async fn gather_status(
    targets: &[Target],
    auth: &AuthSource,
) -> Vec<(Target, Result<HostReading, String>)> {
    let mut tasks = Vec::with_capacity(targets.len());
    for target in targets {
        let target = target.clone();
        let auth = auth.clone();
        tasks.push((
            target.clone(),
            tokio::spawn(async move { read_host(&target, &auth).await }),
        ));
    }
    let mut readings = Vec::with_capacity(tasks.len());
    for (target, task) in tasks {
        let reading = match task.await {
            Ok(Ok(reading)) => Ok(reading),
            Ok(Err(err)) => Err(format!("{err:#}")),
            Err(err) => Err(format!("the host task ended abnormally: {err}")),
        };
        readings.push((target, reading));
    }
    readings
}

async fn read_host(target: &Target, auth: &AuthSource) -> Result<HostReading> {
    let conn = Conn::open(target, auth).await?;
    let (status, state) = tokio::try_join!(conn.get_json("status"), conn.get_json("state"))?;
    Ok(HostReading {
        host: status
            .get("host")
            .and_then(|h| h.as_str())
            .map(|h| h.to_string()),
        latest_revision: status.get("latest_revision").and_then(|r| r.as_u64()),
        content_id: state
            .get("content_id")
            .and_then(|c| c.as_str())
            .map(|c| c.to_string()),
    })
}

pub const NOTHING_APPLIED: &str = "nothing applied";

pub fn status_lines(
    readings: &[(Target, Result<HostReading, String>)],
    color: bool,
) -> Vec<String> {
    let name_width = readings
        .iter()
        .map(|(target, _)| target.name.chars().count())
        .max()
        .unwrap_or(0);
    let revision_width = readings
        .iter()
        .filter_map(|(_, reading)| reading.as_ref().ok())
        .map(|reading| revision_cell(reading).chars().count())
        .max()
        .unwrap_or(0);
    readings
        .iter()
        .map(|(target, reading)| status_line(target, reading, name_width, revision_width, color))
        .collect()
}

fn status_line(
    target: &Target,
    reading: &Result<HostReading, String>,
    name_width: usize,
    revision_width: usize,
    color: bool,
) -> String {
    let (mark, accent) = status_mark(reading);
    let mut line = format!(
        "{} {}{}",
        paint(mark, accent, color),
        paint(&target.name, BOLD, color),
        padding(&target.name, name_width),
    );
    match reading {
        Ok(reading) => {
            let revision = revision_cell(reading);
            line.push_str(&format!(
                "  {revision}{}  ",
                padding(&revision, revision_width)
            ));
            match &reading.content_id {
                Some(content_id) => line.push_str(&short_content_id(content_id)),
                None => line.push_str(&paint(NOTHING_APPLIED, DIM, color)),
            }
            if let Some(daemon) = misnamed_daemon(target, reading) {
                line.push_str(&paint(&format!("  (daemon: {daemon})"), DIM, color));
            }
        }
        Err(message) => line.push_str(&format!("  unreachable: {}", concise_error(message))),
    }
    line
}

fn status_mark(reading: &Result<HostReading, String>) -> (&'static str, &'static str) {
    match reading {
        Ok(reading) if reading.content_id.is_some() => (view::CHECKMARK, GREEN),
        Ok(_) => (view::UNCHANGED, DIM),
        Err(_) => (view::XMARK, RED),
    }
}

fn revision_cell(reading: &HostReading) -> String {
    match reading.latest_revision {
        Some(id) => format!("rev {id}"),
        None => "rev none".to_string(),
    }
}

fn misnamed_daemon<'a>(target: &Target, reading: &'a HostReading) -> Option<&'a str> {
    reading
        .host
        .as_deref()
        .filter(|daemon| *daemon != target.name)
}

fn padding(text: &str, width: usize) -> String {
    " ".repeat(width.saturating_sub(text.chars().count()))
}

fn short_content_id(content_id: &str) -> String {
    content_id.chars().take(CONTENT_ID_PREFIX_CHARS).collect()
}

pub fn status_json(readings: &[(Target, Result<HostReading, String>)]) -> serde_json::Value {
    let mut hosts = serde_json::Map::new();
    for (target, reading) in readings {
        let entry = match reading {
            Ok(reading) => serde_json::json!({
                "addr": target.addr,
                "host": reading.host,
                "latest_revision": reading.latest_revision,
                "content_id": reading.content_id,
            }),
            Err(message) => serde_json::json!({
                "addr": target.addr,
                "error": message,
            }),
        };
        hosts.insert(target.name.clone(), entry);
    }
    serde_json::json!({ "hosts": hosts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled() -> HostOutcome {
        outcome_of(Some(serde_json::json!({
            "outcome": "settled",
            "revision": { "id": 1 },
            "units": []
        })))
    }

    #[test]
    fn a_transport_error_chain_collapses_to_context_and_root_cause() {
        let chain = "GET http://127.0.0.1:8899/status: error sending request for url (http://127.0.0.1:8899/status): client error (Connect): tcp connect error: Connection refused (os error 111)";
        assert_eq!(
            concise_error(chain),
            "GET http://127.0.0.1:8899/status — Connection refused (os error 111)"
        );
        assert_eq!(concise_error("plain message"), "plain message");
        assert_eq!(concise_error("context: cause"), "context: cause");
    }

    fn rolled_back() -> HostOutcome {
        outcome_of(Some(serde_json::json!({
            "outcome": "rolled_back",
            "revision": { "id": 2 },
            "units": []
        })))
    }

    fn unreachable() -> HostOutcome {
        HostOutcome::Error {
            message: "connection refused".into(),
        }
    }

    fn target(name: &str) -> Target {
        Target {
            name: name.into(),
            addr: format!("http://{name}:8807"),
        }
    }

    fn manifest_naming(hosts: &[&str]) -> Vec<u8> {
        let scrolls = hosts
            .iter()
            .map(|host| scroll_format::Scroll {
                name: host.to_string(),
                policy: None,
                notifies: vec![],
                contents: scroll_format::Contents::Glyphs(vec![scroll_format::Glyph::AptPackage {
                    name: "nginx".into(),
                }]),
            })
            .collect();
        scroll_format::to_bytes(&scroll_format::Manifest::from_scrolls(scrolls, "test"))
    }

    #[test]
    fn a_report_without_an_outcome_field_still_reads_as_settled() {
        assert_eq!(outcome_of(None), HostOutcome::Settled { report: None });
    }

    #[test]
    fn a_partial_report_is_unsettled_not_settled() {
        let outcome = outcome_of(Some(serde_json::json!({ "outcome": "partial" })));
        assert!(!outcome.is_settled());
        assert!(matches!(outcome, HostOutcome::Unsettled { .. }));
    }

    #[test]
    fn the_fleet_exits_zero_only_when_every_host_settled() {
        assert_eq!(
            fleet_exit_code(&[(target("a"), settled()), (target("b"), settled())]),
            0
        );
        assert_eq!(
            fleet_exit_code(&[(target("a"), settled()), (target("b"), rolled_back())]),
            1
        );
        assert_eq!(
            fleet_exit_code(&[(target("a"), settled()), (target("b"), unreachable())]),
            1
        );
    }

    #[test]
    fn a_skipped_host_never_costs_the_fleet_its_zero_exit() {
        assert_eq!(
            fleet_exit_code(&[
                (target("a"), settled()),
                (target("b"), HostOutcome::Skipped),
                (target("c"), HostOutcome::Skipped),
            ]),
            0
        );
        assert_eq!(fleet_exit_code(&[(target("b"), HostOutcome::Skipped)]), 0);
        assert_eq!(
            fleet_exit_code(&[
                (target("b"), HostOutcome::Skipped),
                (target("c"), unreachable()),
            ]),
            1
        );
    }

    #[test]
    fn a_host_the_manifest_names_no_scroll_for_is_skipped() {
        let fanout = Fanout::read(
            &manifest_naming(&["scaly", "manta"]),
            vec![target("scaly"), target("manta"), target("otter")],
        )
        .unwrap();
        assert!(fanout.carries_a_scroll_for(&target("scaly")));
        assert!(fanout.carries_a_scroll_for(&target("manta")));
        assert!(!fanout.carries_a_scroll_for(&target("otter")));
        assert_eq!(fanout.targets().len(), 3);
    }

    #[test]
    fn undecodable_manifest_bytes_fail_before_any_host_is_contacted() {
        let err = Fanout::read(b"not a manifest at all", vec![target("scaly")]).unwrap_err();
        assert!(format!("{err:#}").contains("decode the manifest"));
    }

    #[test]
    fn a_skipped_host_reports_why_in_the_summary_and_the_aggregate() {
        let results = [
            (target("scaly"), settled()),
            (target("otter"), HostOutcome::Skipped),
        ];
        let lines = summary_lines(&results, false);
        assert_eq!(lines[0], "scaly  http://scaly:8807");
        assert_eq!(lines[1], "  apply settled — revision 1");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "otter  http://otter:8807");
        assert_eq!(lines[4], "  skipped — no scroll in manifest");

        let aggregate = apply_json(&results);
        assert_eq!(aggregate["hosts"]["otter"]["skipped"], true);
        assert!(aggregate["hosts"]["otter"]["outcome"].is_null());
        assert!(aggregate["hosts"]["otter"]["error"].is_null());
    }

    #[test]
    fn a_skipped_host_shows_no_phantom_plan() {
        let results = vec![
            (target("otter"), HostPlan::Skipped),
            (
                target("scaly"),
                HostPlan::Report(
                    serde_json::json!({
                        "host": "scaly",
                        "scroll_content_id": "abcdef0123456789",
                        "against_revision": null,
                        "ops": [],
                        "reloads": [],
                        "summary": { "install": 0, "replace": 0, "remove": 0, "noop": 0 }
                    })
                    .to_string(),
                ),
            ),
        ];
        let lines = plan_lines(&results, &RenderOptions::default());
        assert_eq!(lines[0], "otter  http://otter:8807");
        assert_eq!(lines[1], "  skipped — no scroll in manifest");
        assert!(!lines.iter().any(|line| line.contains("Plan for")));

        let aggregate = plan_json(&results);
        assert_eq!(aggregate["hosts"]["otter"]["skipped"], true);
        assert_eq!(aggregate["hosts"]["scaly"]["host"], "scaly");
    }

    #[test]
    fn a_skipped_hosts_heading_carries_the_quiet_mark() {
        assert_eq!(
            host_mark(HostStatus::of(&HostOutcome::Skipped)),
            view::UNCHANGED
        );
        let lines = fleet_lines(&[HostProgress::skipped(&target("otter"))]);
        assert!(matches!(&lines[0], Line::Branch { active: false, .. }));
        assert!(
            matches!(&lines[1], Line::Plain { text } if text.contains(SKIPPED_NOTE)),
            "the skipped host says why under its heading"
        );
    }

    #[test]
    fn one_hosts_failure_never_masks_another_hosts_settle_in_the_summary() {
        let lines = summary_lines(
            &[
                (target("scaly"), settled()),
                (target("manta"), unreachable()),
            ],
            false,
        );
        assert_eq!(lines[0], "scaly  http://scaly:8807");
        assert_eq!(lines[1], "  apply settled — revision 1");
        assert_eq!(lines[3], "manta  http://manta:8807");
        assert_eq!(lines[4], "  error: connection refused");
    }

    #[test]
    fn a_summary_paints_the_heading_and_the_error_only_when_color_is_welcome() {
        let results = [
            (target("scaly"), settled()),
            (target("manta"), unreachable()),
        ];
        let painted = summary_lines(&results, true);
        assert_eq!(
            painted[0],
            "\u{1b}[1mscaly\u{1b}[0m  \u{1b}[2mhttp://scaly:8807\u{1b}[0m"
        );
        assert_eq!(painted[4], "  \u{1b}[31merror:\u{1b}[0m connection refused");
        assert!(summary_lines(&results, false)
            .iter()
            .all(|line| !line.contains('\u{1b}')));
    }

    #[test]
    fn a_fleet_plan_section_indents_a_headline_that_no_longer_repeats_the_host() {
        let results = vec![(
            target("scaly"),
            HostPlan::Report(
                serde_json::json!({
                    "host": "scaly",
                    "scroll_content_id": "abcdef0123456789",
                    "against_revision": 1,
                    "ops": [],
                    "reloads": [],
                    "summary": { "install": 0, "replace": 0, "remove": 0, "noop": 0 }
                })
                .to_string(),
            ),
        )];
        let lines = plan_lines(&results, &RenderOptions::default());
        assert_eq!(
            lines,
            [
                "scaly  http://scaly:8807",
                "  against revision 1 · manifest abcdef…",
                "  no changes",
            ]
        );
    }

    #[test]
    fn a_fleet_plan_paints_its_heading_only_when_color_is_welcome() {
        let results = vec![(
            target("manta"),
            HostPlan::Error("connection refused".to_string()),
        )];
        let painted = plan_lines(
            &results,
            &RenderOptions {
                color: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            painted[0],
            "\u{1b}[1mmanta\u{1b}[0m  \u{1b}[2mhttp://manta:8807\u{1b}[0m"
        );
        assert_eq!(painted[1], "  \u{1b}[31merror:\u{1b}[0m connection refused");
        assert!(plan_lines(&results, &RenderOptions::default())
            .iter()
            .all(|line| !line.contains('\u{1b}')));
    }

    #[test]
    fn every_plain_line_carries_its_hosts_name() {
        assert_eq!(
            prefixed("scaly", "apt:nginx installed"),
            "[scaly] apt:nginx installed"
        );
    }

    #[test]
    fn the_json_aggregate_keys_each_host_by_name_with_outcome_or_error() {
        let aggregate = apply_json(&[
            (target("scaly"), settled()),
            (target("manta"), rolled_back()),
            (target("otter"), unreachable()),
        ]);
        assert_eq!(aggregate["hosts"]["scaly"]["outcome"], "settled");
        assert_eq!(aggregate["hosts"]["scaly"]["report"]["revision"]["id"], 1);
        assert_eq!(aggregate["hosts"]["manta"]["outcome"], "rolled_back");
        assert_eq!(aggregate["hosts"]["otter"]["error"], "connection refused");
        assert!(aggregate["hosts"]["otter"]["outcome"].is_null());
        assert_eq!(aggregate.as_object().unwrap().len(), 1);
    }

    #[test]
    fn a_hosts_heading_carries_the_mark_its_outcome_earned() {
        assert_eq!(host_mark(HostStatus::of(&settled())), view::CHECKMARK);
        assert_eq!(host_mark(HostStatus::of(&rolled_back())), view::ROLLED_BACK);
        assert_eq!(host_mark(HostStatus::of(&unreachable())), view::XMARK);
        assert_eq!(host_mark(HostStatus::InFlight), view::SPINNER_FRAMES[0]);
    }

    #[test]
    fn each_hosts_tree_sits_one_level_under_its_heading() {
        let mut host = HostProgress::in_flight(&target("scaly"));
        host.model.apply_progress(crate::poll::Progress {
            reconcile_id: 1,
            phase: crate::poll::Phase::Enacting,
            units: vec![crate::poll::UnitProgress {
                unit_path: vec!["scaly".into(), "web".into()],
                glyphs: vec![crate::poll::GlyphProgress {
                    glyph_key: "apt:nginx".into(),
                    action: "install".into(),
                    state: crate::poll::GlyphState::InProgress,
                    rounds: 1,
                    next_retry_in_ms: None,
                    shared: false,
                    owner: None,
                }],
            }],
            events: vec![],
            cursor: 0,
            report: None,
        });

        let lines = fleet_lines(&[host]);
        match &lines[0] {
            Line::Branch {
                depth,
                label,
                active,
                ..
            } => {
                assert_eq!(*depth, 0);
                assert_eq!(label, "scaly");
                assert!(active);
            }
            _ => panic!("the first line is the host heading"),
        }
        let depths: Vec<usize> = lines[1..]
            .iter()
            .filter_map(|line| match line {
                Line::Branch { depth, .. } | Line::Glyph { depth, .. } => Some(*depth),
                _ => None,
            })
            .collect();
        assert!(depths.iter().all(|d| *d >= 1), "{depths:?}");
    }

    #[test]
    fn a_plan_error_renders_under_its_hosts_heading_without_hiding_the_others() {
        let results = vec![
            (
                target("manta"),
                HostPlan::Error("connection refused".to_string()),
            ),
            (
                target("scaly"),
                HostPlan::Report(
                    serde_json::json!({
                        "host": "scaly",
                        "scroll_content_id": "abcdef0123456789",
                        "against_revision": null,
                        "ops": [],
                        "reloads": [],
                        "summary": { "install": 0, "replace": 0, "remove": 0, "noop": 0 }
                    })
                    .to_string(),
                ),
            ),
        ];
        let lines = plan_lines(&results, &RenderOptions::default());
        assert_eq!(lines[0], "manta  http://manta:8807");
        assert_eq!(lines[1], "  error: connection refused");
        assert!(lines.iter().any(|line| line == "scaly  http://scaly:8807"));
        assert!(lines
            .iter()
            .any(|line| line == "  against no prior revision · manifest abcdef…"));

        let aggregate = plan_json(&results);
        assert_eq!(aggregate["hosts"]["manta"]["error"], "connection refused");
        assert_eq!(aggregate["hosts"]["scaly"]["host"], "scaly");
    }

    fn applied(host: &str, revision: u64) -> Result<HostReading, String> {
        Ok(HostReading {
            host: Some(host.into()),
            latest_revision: Some(revision),
            content_id: Some("0123456789abcdef0123".into()),
        })
    }

    #[test]
    fn each_status_mark_says_which_of_the_three_states_the_host_is_in() {
        let readings = [
            (target("scaly"), applied("scaly", 2)),
            (
                target("orbit"),
                Ok(HostReading {
                    host: Some("orbit".into()),
                    latest_revision: Some(1),
                    content_id: None,
                }),
            ),
            (target("zulip"), Err("connection refused".to_string())),
        ];
        assert_eq!(
            status_lines(&readings, false),
            [
                "✓ scaly  rev 2  0123456789ab",
                "· orbit  rev 1  nothing applied",
                "✗ zulip  unreachable: connection refused",
            ]
        );
    }

    #[test]
    fn the_name_and_revision_columns_align_to_the_widest_of_the_selected_hosts() {
        let readings = [
            (target("scaly"), applied("scaly", 9)),
            (target("longhorn"), applied("longhorn", 10)),
            (target("zulip"), Err("connection refused".to_string())),
        ];
        assert_eq!(
            status_lines(&readings, false),
            [
                "✓ scaly     rev 9   0123456789ab",
                "✓ longhorn  rev 10  0123456789ab",
                "✗ zulip     unreachable: connection refused",
            ]
        );
    }

    #[test]
    fn a_status_line_paints_its_mark_and_name_only_when_color_is_welcome() {
        let readings = [
            (target("scaly"), applied("scaly", 2)),
            (
                target("orbit"),
                Ok(HostReading {
                    host: Some("orbit".into()),
                    latest_revision: Some(1),
                    content_id: None,
                }),
            ),
            (target("zulip"), Err("connection refused".to_string())),
        ];
        let painted = status_lines(&readings, true);
        assert_eq!(
            painted[0],
            "\u{1b}[32m✓\u{1b}[0m \u{1b}[1mscaly\u{1b}[0m  rev 2  0123456789ab"
        );
        assert_eq!(
            painted[1],
            "\u{1b}[2m·\u{1b}[0m \u{1b}[1morbit\u{1b}[0m  rev 1  \u{1b}[2mnothing applied\u{1b}[0m"
        );
        assert_eq!(
            painted[2],
            "\u{1b}[31m✗\u{1b}[0m \u{1b}[1mzulip\u{1b}[0m  unreachable: connection refused"
        );
        assert!(status_lines(&readings, false)
            .iter()
            .all(|line| !line.contains('\u{1b}')));
    }

    #[test]
    fn a_daemon_answering_to_another_name_than_the_inventorys_says_so() {
        let readings = [(target("scaly"), applied("scaly-01", 2))];
        assert_eq!(
            status_lines(&readings, false),
            ["✓ scaly  rev 2  0123456789ab  (daemon: scaly-01)"]
        );
        assert!(
            status_lines(&[(target("scaly"), applied("scaly", 2))], false)[0]
                .ends_with("0123456789ab")
        );
    }

    #[test]
    fn an_unreachable_host_says_so_on_its_own_status_line() {
        let readings = [(target("manta"), Err("connection refused".to_string()))];
        assert_eq!(
            status_lines(&readings, false),
            ["✗ manta  unreachable: connection refused"]
        );
        let aggregate = status_json(&readings);
        assert_eq!(aggregate["hosts"]["manta"]["error"], "connection refused");
        assert_eq!(aggregate["hosts"]["manta"]["addr"], "http://manta:8807");
    }
}
