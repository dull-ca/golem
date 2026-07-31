use std::io::IsTerminal;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PlanResponse {
    pub host: String,
    pub scroll_content_id: String,
    pub against_revision: Option<u64>,
    pub ops: Vec<PlannedOp>,
    #[serde(default)]
    pub reloads: Vec<PredictedReload>,
    pub summary: PlanSummary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlannedOp {
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub action: Action,
    #[serde(default)]
    pub old_cid: Option<String>,
    #[serde(default)]
    pub new_cid: Option<String>,
    pub describe: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Replace,
    Remove,
    Noop,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PredictedReload {
    pub unit: String,
    pub kind: String,
    pub triggered_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PlanSummary {
    pub install: usize,
    pub replace: usize,
    pub remove: usize,
    pub noop: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub detail: bool,
    pub color: bool,
    pub width: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            detail: false,
            color: false,
            width: DEFAULT_WIDTH,
        }
    }
}

pub const DEFAULT_WIDTH: usize = 100;
const VISIBLE_MEMBER_CAP: usize = 8;
const MARGIN: usize = 2;
const VERB_WIDTH: usize = 7;
const KIND_GAP: usize = 2;
const DETAIL_INDENT: usize = 6;
const SHORT_CID_CHARS: usize = 6;

const RESET: &str = "\u{1b}[0m";
const BOLD: &str = "\u{1b}[1m";
const DIM: &str = "\u{1b}[2m";
const GREEN: &str = "\u{1b}[32m";
const YELLOW: &str = "\u{1b}[33m";
const RED: &str = "\u{1b}[31m";
const CYAN: &str = "\u{1b}[36m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlyphKind {
    AptPackage,
    SystemdUnit,
    File,
    LineInFile,
    Unrecognized,
}

#[derive(Debug, Clone)]
struct Element {
    text: String,
    dim: bool,
}

#[derive(Debug, Clone)]
struct Step {
    mark: &'static str,
    verb: &'static str,
    accent: &'static str,
    count: usize,
    kind: String,
    members: Vec<Element>,
    one_member_per_line: bool,
    details: Vec<Vec<Element>>,
}

pub async fn run(bytes: Vec<u8>, addr: &str, json: bool, detail: bool) -> Result<()> {
    let body = post_plan(addr, bytes).await?;
    let options = RenderOptions {
        detail,
        color: color_is_welcome(),
        width: DEFAULT_WIDTH,
    };
    println!("{}", present(&body, json, &options)?);
    Ok(())
}

pub async fn post_plan(addr: &str, bytes: Vec<u8>) -> Result<String> {
    let url = format!("{}/plan", addr.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                bail!("{status}: {msg}");
            }
        }
        bail!("{status}: {text}");
    }
    Ok(text)
}

pub fn color_is_welcome() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub fn present(body: &str, json: bool, options: &RenderOptions) -> Result<String> {
    if json {
        return Ok(body.trim_end().to_string());
    }
    let plan: PlanResponse =
        serde_json::from_str(body).context("decode the plan response from golemd")?;
    Ok(render(&plan, options))
}

pub fn render(plan: &PlanResponse, options: &RenderOptions) -> String {
    let steps = steps_of(plan);
    let mut lines = vec![headline(plan, options), String::new()];
    lines.extend(step_lines(&steps, options));
    if !steps.is_empty() {
        lines.push(String::new());
    }
    lines.push(footer(&plan.summary, options));
    lines.join("\n")
}

fn headline(plan: &PlanResponse, options: &RenderOptions) -> String {
    let against = match plan.against_revision {
        Some(id) => format!("against revision {id}"),
        None => "against no prior revision".to_string(),
    };
    format!(
        "Plan for {} {} {} {} manifest {}",
        plan.host,
        paint("·", DIM, options.color),
        against,
        paint("·", DIM, options.color),
        short_cid(&plan.scroll_content_id),
    )
}

fn step_lines(steps: &[Step], options: &RenderOptions) -> Vec<String> {
    let count_width = steps
        .iter()
        .map(|s| s.count.to_string().chars().count())
        .max()
        .unwrap_or(1);
    let kind_width = steps
        .iter()
        .map(|s| s.kind.chars().count())
        .max()
        .unwrap_or(0);
    let verb_width = steps
        .iter()
        .map(|s| s.verb.chars().count())
        .max()
        .unwrap_or(0)
        .max(VERB_WIDTH);
    let member_column = MARGIN + 2 + verb_width + 1 + count_width + 1 + kind_width + KIND_GAP;
    let mut lines = Vec::new();
    for step in steps {
        let head = format!(
            "{}{} {} {:>count$} {}{}",
            " ".repeat(MARGIN),
            paint(step.mark, step.accent, options.color),
            paint(&pad(step.verb, verb_width), step.accent, options.color),
            step.count,
            paint(&step.kind, BOLD, options.color),
            " ".repeat(kind_width - step.kind.chars().count() + KIND_GAP),
            count = count_width,
        );
        if options.detail && !step.details.is_empty() {
            lines.push(head.trim_end().to_string());
            for detail in &step.details {
                let indent = MARGIN + DETAIL_INDENT;
                for line in wrap(detail, indent, options.width, false, options.color) {
                    lines.push(format!("{}{line}", " ".repeat(indent)));
                }
            }
            continue;
        }
        let wrapped = wrap(
            &step.members,
            member_column,
            options.width,
            step.one_member_per_line,
            options.color,
        );
        for (n, line) in wrapped.into_iter().enumerate() {
            if n == 0 {
                lines.push(format!("{head}{line}"));
            } else {
                lines.push(format!("{}{line}", " ".repeat(member_column)));
            }
        }
    }
    lines
}

fn footer(summary: &PlanSummary, options: &RenderOptions) -> String {
    let changes = summary.install + summary.replace + summary.remove;
    let mut segments = Vec::new();
    segments.push(match changes {
        0 => "no changes".to_string(),
        1 => "1 change".to_string(),
        n => format!("{n} changes"),
    });
    let mut by_action = Vec::new();
    if summary.install > 0 {
        by_action.push(format!("{} install", summary.install));
    }
    if summary.replace > 0 {
        by_action.push(format!("{} replace", summary.replace));
    }
    if summary.remove > 0 {
        by_action.push(format!("{} remove", summary.remove));
    }
    if !by_action.is_empty() {
        segments.push(by_action.join(", "));
    }
    if summary.noop > 0 {
        segments.push(format!("{} unchanged", summary.noop));
    }
    let text = format!("{}{}", " ".repeat(MARGIN), segments.join(" · "));
    paint(&text, DIM, options.color)
}

fn steps_of(plan: &PlanResponse) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut provenance: Vec<Vec<String>> = Vec::new();
    for op in &plan.ops {
        if op.action == Action::Noop {
            continue;
        }
        let kind = kind_of(&op.glyph_key);
        let (mark, verb, accent) = action_style(op.action);
        let unit = op.unit_path.join("/");
        let position = steps
            .iter()
            .position(|s| s.verb == verb && s.kind == kind_stem(kind));
        let index = match position {
            Some(index) => index,
            None => {
                steps.push(Step {
                    mark,
                    verb,
                    accent,
                    count: 0,
                    kind: kind_stem(kind).to_string(),
                    members: Vec::new(),
                    one_member_per_line: false,
                    details: Vec::new(),
                });
                provenance.push(Vec::new());
                steps.len() - 1
            }
        };
        let step = &mut steps[index];
        step.count += 1;
        step.members.push(Element {
            text: member_of(&op.glyph_key),
            dim: false,
        });
        step.details.push(vec![
            Element {
                text: member_of(&op.glyph_key),
                dim: false,
            },
            Element {
                text: cid_transition(op),
                dim: true,
            },
            Element {
                text: format!("({unit})"),
                dim: true,
            },
        ]);
        let units = &mut provenance[index];
        if !units.contains(&unit) {
            units.push(unit);
        }
    }
    for (step, units) in steps.iter_mut().zip(provenance) {
        step.kind = pluralize(&step.kind, step.count);
        cap_members(&mut step.members);
        step.members.push(Element {
            text: format!("({})", units.join(", ")),
            dim: true,
        });
    }
    steps.extend(reload_steps(&plan.reloads));
    steps
}

fn reload_steps(reloads: &[PredictedReload]) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    for reload in reloads {
        let verb = reload_verb(&reload.kind);
        let member = Element {
            text: format!(
                "{} ← {}",
                reload.unit,
                reload
                    .triggered_by
                    .iter()
                    .map(|key| member_of(key))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            dim: false,
        };
        match steps.iter_mut().find(|step| step.verb == verb) {
            Some(step) => {
                step.count += 1;
                step.members.push(member);
            }
            None => steps.push(Step {
                mark: "↻",
                verb,
                accent: CYAN,
                count: 1,
                kind: "unit".to_string(),
                members: vec![member],
                one_member_per_line: true,
                details: Vec::new(),
            }),
        }
    }
    for step in steps.iter_mut() {
        step.kind = pluralize("unit", step.count);
    }
    steps
}

fn reload_verb(kind: &str) -> &'static str {
    match kind {
        "restart" => "restart",
        _ => "reload-or-restart",
    }
}

fn cap_members(members: &mut Vec<Element>) {
    if members.len() <= VISIBLE_MEMBER_CAP {
        return;
    }
    let hidden = members.len() - VISIBLE_MEMBER_CAP;
    members.truncate(VISIBLE_MEMBER_CAP);
    members.push(Element {
        text: format!("… and {hidden} more"),
        dim: true,
    });
}

fn action_style(action: Action) -> (&'static str, &'static str, &'static str) {
    match action {
        Action::Install => ("+", "install", GREEN),
        Action::Replace => ("~", "replace", YELLOW),
        Action::Remove => ("-", "remove", RED),
        Action::Noop => ("·", "noop", DIM),
    }
}

fn kind_of(glyph_key: &str) -> GlyphKind {
    if glyph_key.starts_with("apt:") {
        GlyphKind::AptPackage
    } else if glyph_key.starts_with("systemd:") {
        GlyphKind::SystemdUnit
    } else if glyph_key.starts_with("fileline:") {
        GlyphKind::LineInFile
    } else if glyph_key.starts_with("file:") {
        GlyphKind::File
    } else {
        GlyphKind::Unrecognized
    }
}

fn kind_stem(kind: GlyphKind) -> &'static str {
    match kind {
        GlyphKind::AptPackage => "apt package",
        GlyphKind::SystemdUnit => "systemd unit",
        GlyphKind::File => "file",
        GlyphKind::LineInFile => "line-in-file",
        GlyphKind::Unrecognized => "glyph",
    }
}

fn pluralize(stem: &str, count: usize) -> String {
    if count == 1 {
        return stem.to_string();
    }
    match stem {
        "line-in-file" => "lines-in-file".to_string(),
        other => format!("{other}s"),
    }
}

fn member_of(glyph_key: &str) -> String {
    match kind_of(glyph_key) {
        GlyphKind::AptPackage => glyph_key.trim_start_matches("apt:").to_string(),
        GlyphKind::SystemdUnit => glyph_key.trim_start_matches("systemd:").to_string(),
        GlyphKind::File => glyph_key.trim_start_matches("file:").to_string(),
        GlyphKind::LineInFile => {
            let rest = glyph_key.trim_start_matches("fileline:");
            match rest.split_once(':') {
                Some((path, line)) => format!("{path}: \"{line}\""),
                None => rest.to_string(),
            }
        }
        GlyphKind::Unrecognized => glyph_key.to_string(),
    }
}

fn cid_transition(op: &PlannedOp) -> String {
    match (&op.old_cid, &op.new_cid) {
        (Some(old), Some(new)) => format!("{}→{}", short_cid(old), short_cid(new)),
        (None, Some(new)) => short_cid(new),
        (Some(old), None) => short_cid(old),
        (None, None) => String::new(),
    }
}

fn short_cid(cid: &str) -> String {
    if cid.chars().count() <= SHORT_CID_CHARS {
        return cid.to_string();
    }
    let head: String = cid.chars().take(SHORT_CID_CHARS).collect();
    format!("{head}…")
}

fn wrap(
    elements: &[Element],
    indent: usize,
    width: usize,
    one_per_line: bool,
    color: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut column = indent;
    for element in elements {
        let length = element.text.chars().count();
        let breaks = !current.is_empty() && (one_per_line || column + 1 + length > width);
        if breaks {
            lines.push(current);
            current = String::new();
            column = indent;
        }
        if !current.is_empty() {
            current.push(' ');
            column += 1;
        }
        current.push_str(&paint(
            &element.text,
            if element.dim { DIM } else { "" },
            color,
        ));
        column += length;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn pad(text: &str, width: usize) -> String {
    let length = text.chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(length)))
}

fn paint(text: &str, sgr: &str, color: bool) -> String {
    if !color || sgr.is_empty() {
        return text.to_string();
    }
    format!("{sgr}{text}{RESET}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(unit: &[&str], key: &str, action: Action) -> PlannedOp {
        PlannedOp {
            unit_path: unit.iter().map(|s| s.to_string()).collect(),
            glyph_key: key.to_string(),
            action,
            old_cid: Some("aaaaaaaaaaaaaaaa".into()),
            new_cid: Some("bbbbbbbbbbbbbbbb".into()),
            describe: format!("describe {key}"),
        }
    }

    fn mixed_plan() -> PlanResponse {
        let mut ops = vec![
            op(&["web", "base"], "apt:nginx", Action::Install),
            op(&["web", "base"], "apt:curl", Action::Install),
            op(&["web", "nginx"], "systemd:nginx.service", Action::Install),
            op(&["web", "nginx"], "apt:podman", Action::Noop),
            op(
                &["web", "nginx"],
                "file:/etc/systemd/system/nginx.service",
                Action::Replace,
            ),
            op(&["web", "base"], "file:/etc/motd", Action::Replace),
            op(
                &["web", "<removes>"],
                "fileline:/etc/hosts:10.0.0.3 oldhost",
                Action::Remove,
            ),
        ];
        ops.insert(2, op(&["web", "extra"], "apt:jq", Action::Install));
        PlanResponse {
            host: "web-01".into(),
            scroll_content_id: "3f9c1adeadbeef".into(),
            against_revision: Some(12),
            ops,
            reloads: vec![PredictedReload {
                unit: "nginx.service".into(),
                kind: "restart".into(),
                triggered_by: vec!["file:/etc/systemd/system/nginx.service".into()],
            }],
            summary: PlanSummary {
                install: 4,
                replace: 2,
                remove: 1,
                noop: 42,
            },
        }
    }

    #[test]
    fn the_collapsed_view_groups_by_action_and_kind_in_execution_order() {
        let rendered = render(&mixed_plan(), &RenderOptions::default());
        assert_eq!(
            rendered,
            [
                "Plan for web-01 · against revision 12 · manifest 3f9c1a…",
                "",
                "  + install 3 apt packages  nginx curl jq (web/base, web/extra)",
                "  + install 1 systemd unit  nginx.service (web/nginx)",
                "  ~ replace 2 files         /etc/systemd/system/nginx.service /etc/motd (web/nginx, web/base)",
                "  - remove  1 line-in-file  /etc/hosts: \"10.0.0.3 oldhost\" (web/<removes>)",
                "  ↻ restart 1 unit          nginx.service ← /etc/systemd/system/nginx.service",
                "",
                "  7 changes · 4 install, 2 replace, 1 remove · 42 unchanged",
            ]
            .join("\n")
        );
    }

    #[test]
    fn the_two_reload_kinds_render_as_their_own_verbs_and_keep_the_columns_aligned() {
        let mut plan = mixed_plan();
        plan.reloads = vec![
            PredictedReload {
                unit: "api.service".into(),
                kind: "restart".into(),
                triggered_by: vec!["file:/etc/systemd/system/api.service".into()],
            },
            PredictedReload {
                unit: "nginx.service".into(),
                kind: "reload-or-restart".into(),
                triggered_by: vec!["file:/etc/nginx/nginx.conf".into()],
            },
            PredictedReload {
                unit: "telegraf.service".into(),
                kind: "reload-or-restart".into(),
                triggered_by: vec!["file:/etc/telegraf/telegraf.conf".into()],
            },
        ];
        let rendered = render(&plan, &RenderOptions::default());
        assert_eq!(
            rendered,
            [
                "Plan for web-01 · against revision 12 · manifest 3f9c1a…",
                "",
                "  + install           3 apt packages  nginx curl jq (web/base, web/extra)",
                "  + install           1 systemd unit  nginx.service (web/nginx)",
                "  ~ replace           2 files         /etc/systemd/system/nginx.service /etc/motd",
                "                                      (web/nginx, web/base)",
                "  - remove            1 line-in-file  /etc/hosts: \"10.0.0.3 oldhost\" (web/<removes>)",
                "  ↻ restart           1 unit          api.service ← /etc/systemd/system/api.service",
                "  ↻ reload-or-restart 2 units         nginx.service ← /etc/nginx/nginx.conf",
                "                                      telegraf.service ← /etc/telegraf/telegraf.conf",
                "",
                "  7 changes · 4 install, 2 replace, 1 remove · 42 unchanged",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_plain_render_carries_no_escape_codes_and_a_colored_one_does() {
        let plain = render(&mixed_plan(), &RenderOptions::default());
        assert!(!plain.contains('\u{1b}'));
        let colored = render(
            &mixed_plan(),
            &RenderOptions {
                color: true,
                ..RenderOptions::default()
            },
        );
        assert!(colored.contains(GREEN));
        assert!(colored.contains(YELLOW));
        assert!(colored.contains(RED));
        assert!(colored.contains(CYAN));
        assert!(colored.contains(BOLD));
        assert!(colored.contains(DIM));
    }

    #[test]
    fn a_long_member_list_caps_and_wraps_under_the_member_column() {
        let names = [
            "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
            "juliett", "kilo",
        ];
        let plan = PlanResponse {
            host: "web-01".into(),
            scroll_content_id: "3f9c1adeadbeef".into(),
            against_revision: Some(12),
            ops: names
                .iter()
                .map(|n| op(&["web"], &format!("apt:{n}"), Action::Install))
                .collect(),
            reloads: vec![],
            summary: PlanSummary {
                install: names.len(),
                replace: 0,
                remove: 0,
                noop: 0,
            },
        };
        let rendered = render(
            &plan,
            &RenderOptions {
                width: 60,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            rendered,
            [
                "Plan for web-01 · against revision 12 · manifest 3f9c1a…",
                "",
                "  + install 11 apt packages  alpha bravo charlie delta echo",
                "                             foxtrot golf hotel … and 3 more",
                "                             (web)",
                "",
                "  11 changes · 11 install",
            ]
            .join("\n")
        );
    }

    #[test]
    fn detail_expands_every_glyph_with_its_cids_and_unit() {
        let rendered = render(
            &mixed_plan(),
            &RenderOptions {
                detail: true,
                ..RenderOptions::default()
            },
        );
        assert!(rendered.contains("      nginx aaaaaa…→bbbbbb… (web/base)"));
        assert!(rendered.contains("      /etc/motd aaaaaa…→bbbbbb… (web/base)"));
        assert!(
            !rendered.contains("apt:podman"),
            "an unchanged glyph stays a footer count even under --detail"
        );
        assert!(
            rendered.contains(
                "↻ restart 1 unit          nginx.service ← /etc/systemd/system/nginx.service"
            ),
            "the reload step has no per-glyph expansion, so it keeps its inline members"
        );
    }

    #[test]
    fn a_plan_with_no_changes_still_reports_its_unchanged_count() {
        let plan = PlanResponse {
            host: "web-01".into(),
            scroll_content_id: "3f9c1adeadbeef".into(),
            against_revision: None,
            ops: vec![op(&["web"], "apt:nginx", Action::Noop)],
            reloads: vec![],
            summary: PlanSummary {
                install: 0,
                replace: 0,
                remove: 0,
                noop: 1,
            },
        };
        let rendered = render(&plan, &RenderOptions::default());
        assert_eq!(
            rendered,
            [
                "Plan for web-01 · against no prior revision · manifest 3f9c1a…",
                "",
                "  no changes · 1 unchanged",
            ]
            .join("\n")
        );
    }

    #[test]
    fn json_passes_the_response_body_through_verbatim() {
        let body = "{\"host\":\"web-01\",\"ops\":[]}\n";
        let passed = present(body, true, &RenderOptions::default()).unwrap();
        assert_eq!(passed, "{\"host\":\"web-01\",\"ops\":[]}");
    }

    #[test]
    fn a_response_body_renders_through_present() {
        let body = serde_json::json!({
            "host": "web-01",
            "scroll_content_id": "3f9c1adeadbeef",
            "against_revision": 3,
            "ops": [
                { "unit_path": ["web"], "glyph_key": "apt:nginx", "action": "install",
                  "new_cid": "bbbbbbbbbbbb", "describe": "ensure apt package `nginx` installed" }
            ],
            "reloads": [],
            "summary": { "install": 1, "replace": 0, "remove": 0, "noop": 0 }
        })
        .to_string();
        let rendered = present(&body, false, &RenderOptions::default()).unwrap();
        assert!(rendered.contains("+ install 1 apt package  nginx (web)"));
    }
}
