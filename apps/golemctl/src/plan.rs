//! The dry-run view: `POST /plan`, then the collapsed render (ADR 0036). One
//! line per (action × glyph kind) in first-occurrence execution order, its
//! members and contributing units listed, the coalesced reload steps last;
//! unchanged glyphs appear only as a footer count.
//!
//! `plan` always exits 0 — a diff is not an error, and a diff-signalling exit
//! code waits until something needs it. Color follows the same policy as the
//! apply view (stdout is a tty and `NO_COLOR` is unset), but the output is
//! composed as plain strings rather than through `view.rs`'s iocraft canvas: a
//! plan is static and must be byte-stable for snapshots and pipes, and the canvas
//! pads every row to its width.

use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::conn::Conn;

#[derive(Debug, Clone, Deserialize)]
pub struct PlanResponse {
    pub host: String,
    pub scroll_content_id: String,
    pub against_revision: Option<u64>,
    pub ops: Vec<PlannedOp>,
    #[serde(default)]
    pub reloads: Vec<PredictedReload>,
    pub summary: PlanSummary,
    #[serde(default)]
    pub reality: Option<Reality>,
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
    #[serde(default)]
    pub observed: Option<Observed>,
    #[serde(default)]
    pub unobservable: Option<Unobservable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    Realized,
    Divergent,
    Absent,
    Unknown,
    #[serde(other)]
    Unrecognized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unobservable {
    Sealed,
    Unreadable,
    NotModelled,
    #[serde(other)]
    Unrecognized,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Reality {
    #[serde(default)]
    pub realized: usize,
    #[serde(default)]
    pub divergent: usize,
    #[serde(default)]
    pub absent: usize,
    #[serde(default)]
    pub unknown: usize,
    #[serde(default)]
    pub already_gone: usize,
    #[serde(default)]
    pub still_present: usize,
    #[serde(default)]
    pub host_already_matches: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
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
    pub nested: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            detail: false,
            color: false,
            width: DEFAULT_WIDTH,
            nested: false,
        }
    }
}

pub const DEFAULT_WIDTH: usize = 100;
/// How many members a step lists before eliding the rest — a reading aid only;
/// `--detail` and `--json` always carry the complete list.
const VISIBLE_MEMBER_CAP: usize = 8;
const MARGIN: usize = 2;
const VERB_WIDTH: usize = 7;
const KIND_GAP: usize = 2;
const DETAIL_INDENT: usize = 6;
const SHORT_CID_CHARS: usize = 6;

const RESET: &str = "\u{1b}[0m";
pub const BOLD: &str = "\u{1b}[1m";
pub const DIM: &str = "\u{1b}[2m";
pub const GREEN: &str = "\u{1b}[32m";
const YELLOW: &str = "\u{1b}[33m";
pub const RED: &str = "\u{1b}[31m";
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
struct Span {
    text: String,
    dim: bool,
}

#[derive(Debug, Clone)]
struct Element {
    spans: Vec<Span>,
}

impl Element {
    fn bright(text: impl Into<String>) -> Self {
        Element {
            spans: vec![Span {
                text: text.into(),
                dim: false,
            }],
        }
    }

    fn dim(text: impl Into<String>) -> Self {
        Element {
            spans: vec![Span {
                text: text.into(),
                dim: true,
            }],
        }
    }

    fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    fn width(&self) -> usize {
        self.spans
            .iter()
            .map(|span| span.text.chars().count())
            .sum()
    }

    fn painted(&self, color: bool) -> String {
        self.spans
            .iter()
            .map(|span| paint(&span.text, if span.dim { DIM } else { "" }, color))
            .collect()
    }
}

#[derive(Debug, Clone)]
struct Member {
    element: Element,
    occurrences: usize,
}

impl Member {
    fn marked(self) -> Element {
        let Member {
            mut element,
            occurrences,
        } = self;
        if occurrences > 1 {
            element.spans.push(Span {
                text: format!(" ×{occurrences}"),
                dim: true,
            });
        }
        element
    }
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

pub async fn run(
    bytes: Vec<u8>,
    conn: &Conn,
    json: bool,
    detail: bool,
    against_host: bool,
) -> Result<()> {
    let body = conn.post_plan(bytes, against_host).await?;
    let options = RenderOptions {
        detail,
        color: color_is_welcome(),
        width: DEFAULT_WIDTH,
        nested: false,
    };
    println!("{}", present(&body, json, &options)?);
    Ok(())
}

pub fn color_is_welcome() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// The one seam between the response and the terminal, so tests drive the whole
/// presentation from a body. `--json` re-emits the raw body rather than
/// re-serializing the parsed struct, so fields a newer golemd added survive the
/// round trip untouched.
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
    let mut blocks = vec![vec![headline(plan, options)]];
    if !steps.is_empty() {
        blocks.push(step_lines(&steps, options));
    }
    blocks.push(vec![footer(&distinct_summary(&plan.ops), options)]);
    stacked(blocks, options)
}

fn stacked(blocks: Vec<Vec<String>>, options: &RenderOptions) -> String {
    let between = if options.nested { "\n" } else { "\n\n" };
    blocks
        .into_iter()
        .map(|block| block.join("\n"))
        .collect::<Vec<_>>()
        .join(between)
}

fn headline(plan: &PlanResponse, options: &RenderOptions) -> String {
    let against = match plan.against_revision {
        Some(id) => format!("against revision {id}"),
        None => "against no prior revision".to_string(),
    };
    let separator = paint("·", DIM, options.color);
    let manifest = short_cid(&plan.scroll_content_id);
    if options.nested {
        return format!(
            "{}{against} {separator} manifest {manifest}",
            " ".repeat(MARGIN)
        );
    }
    format!(
        "Plan for {} {separator} {against} {separator} manifest {manifest}",
        plan.host,
    )
}

/// Lay the steps out into aligned columns. Every width is measured on the
/// *unstyled* text of an element's spans and the accents painted afterwards,
/// because `{:<width}` over an ANSI-wrapped string counts the escape bytes and
/// silently breaks the alignment that continuation lines depend on.
///
/// The verb column is computed rather than fixed, floored at [`VERB_WIDTH`], so
/// `reload-or-restart` widens it without disturbing the member column — and a
/// plan carrying no such step still renders exactly as it did before.
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

/// Distinct (action, glyph key) pairs — what execution dedups (ADR 0034), not ops.
fn distinct_summary(ops: &[PlannedOp]) -> PlanSummary {
    let mut summary = PlanSummary {
        install: 0,
        replace: 0,
        remove: 0,
        noop: 0,
    };
    let mut seen: HashSet<(Action, &str)> = HashSet::new();
    for op in ops {
        if !seen.insert((op.action, op.glyph_key.as_str())) {
            continue;
        }
        match op.action {
            Action::Install => summary.install += 1,
            Action::Replace => summary.replace += 1,
            Action::Remove => summary.remove += 1,
            Action::Noop => summary.noop += 1,
        }
    }
    summary
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

#[derive(Debug, Clone)]
struct GlyphDraft {
    key: String,
    element: Element,
    cids: String,
    units: Vec<String>,
    occurrences: usize,
}

#[derive(Debug, Clone)]
struct StepDraft {
    mark: &'static str,
    verb: &'static str,
    accent: &'static str,
    stem: &'static str,
    glyphs: Vec<GlyphDraft>,
    units: Vec<String>,
}

impl StepDraft {
    fn absorb(&mut self, op: &PlannedOp, unit: String) {
        match self.glyphs.iter_mut().find(|g| g.key == op.glyph_key) {
            Some(glyph) => {
                glyph.occurrences += 1;
                if !glyph.units.contains(&unit) {
                    glyph.units.push(unit.clone());
                }
            }
            None => self.glyphs.push(GlyphDraft {
                key: op.glyph_key.clone(),
                element: member_element(&op.glyph_key),
                cids: cid_transition(op),
                units: vec![unit.clone()],
                occurrences: 1,
            }),
        }
        if !self.units.contains(&unit) {
            self.units.push(unit);
        }
    }

    fn into_step(self) -> Step {
        let details = self
            .glyphs
            .iter()
            .map(|glyph| {
                vec![
                    glyph.element.clone(),
                    Element::dim(glyph.cids.clone()),
                    Element::dim(format!("({})", glyph.units.join(", "))),
                ]
            })
            .collect();
        let count = self.glyphs.len();
        let mut distinct: Vec<Member> = self
            .glyphs
            .into_iter()
            .map(|glyph| Member {
                element: glyph.element,
                occurrences: glyph.occurrences,
            })
            .collect();
        collapse_numeric_siblings(&mut distinct);
        let mut members: Vec<Element> = distinct.into_iter().map(Member::marked).collect();
        cap_members(&mut members);
        members.push(provenance_element(&self.units));
        Step {
            mark: self.mark,
            verb: self.verb,
            accent: self.accent,
            count,
            kind: pluralize(self.stem, count),
            members,
            one_member_per_line: false,
            details,
        }
    }
}

fn steps_of(plan: &PlanResponse) -> Vec<Step> {
    let mut drafts: Vec<StepDraft> = Vec::new();
    for op in &plan.ops {
        if op.action == Action::Noop {
            continue;
        }
        let (mark, verb, accent) = action_style(op.action);
        let stem = kind_stem(kind_of(&op.glyph_key));
        let unit = op.unit_path.join("/");
        let position = drafts.iter().position(|d| d.verb == verb && d.stem == stem);
        let index = match position {
            Some(index) => index,
            None => {
                drafts.push(StepDraft {
                    mark,
                    verb,
                    accent,
                    stem,
                    glyphs: Vec::new(),
                    units: Vec::new(),
                });
                drafts.len() - 1
            }
        };
        drafts[index].absorb(op, unit);
    }
    let mut steps: Vec<Step> = drafts.into_iter().map(StepDraft::into_step).collect();
    steps.extend(reload_steps(&plan.reloads));
    steps
}

fn provenance_element(units: &[String]) -> Element {
    let mut entries: Vec<Member> = units
        .iter()
        .map(|unit| Member {
            element: Element::bright(unit.clone()),
            occurrences: 1,
        })
        .collect();
    collapse_numeric_siblings(&mut entries);
    let joined = entries
        .iter()
        .map(|entry| entry.element.text())
        .collect::<Vec<_>>()
        .join(", ");
    Element::dim(format!("({joined})"))
}

fn collapse_numeric_siblings(members: &mut Vec<Member>) {
    let mut group_of: HashMap<(String, String, String), usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, member) in members.iter().enumerate() {
        if member.occurrences != 1 {
            continue;
        }
        let Some(last) = member.element.spans.last() else {
            continue;
        };
        let Some((prefix, _, suffix)) = split_last_digit_run(&last.text) else {
            continue;
        };
        let leading: String = member.element.spans[..member.element.spans.len() - 1]
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        let key = (leading, prefix.to_string(), suffix.to_string());
        match group_of.get(&key) {
            Some(&group) => groups[group].push(index),
            None => {
                group_of.insert(key, groups.len());
                groups.push(vec![index]);
            }
        }
    }
    let mut absorbed = vec![false; members.len()];
    for group in groups.iter().filter(|group| group.len() >= 2) {
        let mut numbers: Vec<String> = group
            .iter()
            .map(|&index| digit_run_of(&members[index]).to_string())
            .collect();
        numbers.sort_by(|a, b| numeric_order(a).cmp(&numeric_order(b)));
        let head = &members[group[0]].element;
        let last = head.spans[head.spans.len() - 1].text.clone();
        let (prefix, _, suffix) = split_last_digit_run(&last).unwrap_or_default();
        let collapsed = format!("{prefix}{{{}}}{suffix}", numbers.join(","));
        let head = &mut members[group[0]].element;
        let end = head.spans.len() - 1;
        head.spans[end].text = collapsed;
        for &index in &group[1..] {
            absorbed[index] = true;
        }
    }
    let mut kept = Vec::with_capacity(members.len());
    for (index, member) in members.drain(..).enumerate() {
        if !absorbed[index] {
            kept.push(member);
        }
    }
    *members = kept;
}

fn digit_run_of(member: &Member) -> &str {
    member
        .element
        .spans
        .last()
        .and_then(|span| split_last_digit_run(&span.text))
        .map(|(_, digits, _)| digits)
        .unwrap_or_default()
}

fn split_last_digit_run(text: &str) -> Option<(&str, &str, &str)> {
    let bytes = text.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |last| last + 1);
    Some((&text[..start], &text[start..end], &text[end..]))
}

fn numeric_order(digits: &str) -> (usize, &str) {
    let significant = digits.trim_start_matches('0');
    (significant.len(), significant)
}

/// One step per reload *kind*, reusing the verb-×-kind grouping the ops above
/// use. The distinction rides in the verb text rather than a dim annotation so
/// the line stays scannable next to `install` / `replace` / `remove`.
fn reload_steps(reloads: &[PredictedReload]) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    for reload in reloads {
        let verb = reload_verb(&reload.kind);
        let member = reload_member(reload);
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

fn reload_member(reload: &PredictedReload) -> Element {
    let mut spans = vec![Span {
        text: format!("{} ← ", reload.unit),
        dim: false,
    }];
    for (position, key) in reload.triggered_by.iter().enumerate() {
        if position > 0 {
            spans.push(Span {
                text: ", ".to_string(),
                dim: false,
            });
        }
        spans.extend(member_element(key).spans);
    }
    Element { spans }
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
    members.push(Element::dim(format!("… and {hidden} more")));
}

fn action_style(action: Action) -> (&'static str, &'static str, &'static str) {
    match action {
        Action::Install => ("+", "install", GREEN),
        Action::Replace => ("~", "replace", YELLOW),
        Action::Remove => ("-", "remove", RED),
        Action::Noop => ("·", "noop", DIM),
    }
}

// NOTE: this and `member_of` parse `Glyph::key()`'s namespaced form, which is
// explicitly not part of the wire contract — hence the tolerant `Unrecognized`
// arm, which renders the raw key rather than failing on a key shape a newer
// golemd invented.
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

fn member_element(glyph_key: &str) -> Element {
    match kind_of(glyph_key) {
        GlyphKind::File => path_element(glyph_key.trim_start_matches("file:")),
        _ => Element::bright(member_of(glyph_key)),
    }
}

fn path_element(path: &str) -> Element {
    match path.rfind('/') {
        Some(cut) if cut + 1 < path.len() => Element {
            spans: vec![
                Span {
                    text: path[..=cut].to_string(),
                    dim: true,
                },
                Span {
                    text: path[cut + 1..].to_string(),
                    dim: false,
                },
            ],
        },
        _ => Element::bright(path),
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
        let length = element.width();
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
        current.push_str(&element.painted(color));
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

pub fn paint(text: &str, sgr: &str, color: bool) -> String {
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
            observed: None,
            unobservable: None,
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
            reality: None,
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
                "  7 changes · 4 install, 2 replace, 1 remove · 1 unchanged",
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
                "  7 changes · 4 install, 2 replace, 1 remove · 1 unchanged",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_nested_render_drops_the_host_indents_its_headline_and_closes_the_gaps() {
        let rendered = render(
            &mixed_plan(),
            &RenderOptions {
                nested: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            rendered,
            [
                "  against revision 12 · manifest 3f9c1a…",
                "  + install 3 apt packages  nginx curl jq (web/base, web/extra)",
                "  + install 1 systemd unit  nginx.service (web/nginx)",
                "  ~ replace 2 files         /etc/systemd/system/nginx.service /etc/motd (web/nginx, web/base)",
                "  - remove  1 line-in-file  /etc/hosts: \"10.0.0.3 oldhost\" (web/<removes>)",
                "  ↻ restart 1 unit          nginx.service ← /etc/systemd/system/nginx.service",
                "  7 changes · 4 install, 2 replace, 1 remove · 1 unchanged",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_nested_render_with_nothing_to_do_is_two_lines() {
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
            reality: None,
        };
        let rendered = render(
            &plan,
            &RenderOptions {
                nested: true,
                ..RenderOptions::default()
            },
        );
        assert_eq!(
            rendered,
            [
                "  against no prior revision · manifest 3f9c1a…",
                "  no changes · 1 unchanged",
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
            reality: None,
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
            reality: None,
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

    fn install_only(ops: Vec<PlannedOp>) -> PlanResponse {
        PlanResponse {
            host: "web-01".into(),
            scroll_content_id: "3f9c1adeadbeef".into(),
            against_revision: None,
            ops,
            reloads: vec![],
            summary: PlanSummary {
                install: 0,
                replace: 0,
                remove: 0,
                noop: 0,
            },
            reality: None,
        }
    }

    #[test]
    fn a_glyph_shared_by_several_units_renders_once_with_a_dedup_marker() {
        let plan = install_only(vec![
            op(&["farm", "one"], "apt:podman", Action::Install),
            op(&["farm", "two"], "apt:podman", Action::Install),
            op(&["farm", "three"], "apt:podman", Action::Install),
            op(&["farm", "base"], "apt:htop", Action::Install),
        ]);
        let rendered = render(&plan, &RenderOptions::default());
        assert!(
            rendered.contains(
                "+ install 2 apt packages  podman ×3 htop (farm/one, farm/two, farm/three, farm/base)"
            ),
            "{rendered}"
        );
        assert!(rendered.contains("  2 changes · 2 install"), "{rendered}");
    }

    #[test]
    fn the_footer_counts_distinct_glyphs_rather_than_the_server_op_counts() {
        let mut plan = mixed_plan();
        plan.ops
            .push(op(&["web", "spare"], "apt:nginx", Action::Install));
        plan.ops
            .push(op(&["web", "spare"], "apt:podman", Action::Noop));
        let rendered = render(&plan, &RenderOptions::default());
        assert!(
            rendered.contains("  7 changes · 4 install, 2 replace, 1 remove · 1 unchanged"),
            "{rendered}"
        );
    }

    #[test]
    fn numbered_siblings_collapse_into_braces_and_keep_their_zero_padding() {
        let plan = install_only(vec![
            op(&["web"], "systemd:worker-010.service", Action::Install),
            op(&["web"], "systemd:worker-002.service", Action::Install),
            op(&["web"], "systemd:worker-001.service", Action::Install),
            op(&["web"], "systemd:solo-7.service", Action::Install),
            op(&["web"], "systemd:nginx.service", Action::Install),
        ]);
        let rendered = render(&plan, &RenderOptions::default());
        assert!(
            rendered.contains(
                "+ install 5 systemd units  worker-{001,002,010}.service solo-7.service nginx.service"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn a_deduped_member_never_collapses_into_a_brace_group() {
        let plan = install_only(vec![
            op(&["farm", "one"], "apt:node-1", Action::Install),
            op(&["farm", "two"], "apt:node-1", Action::Install),
            op(&["farm", "one"], "apt:node-2", Action::Install),
        ]);
        let rendered = render(&plan, &RenderOptions::default());
        assert!(
            rendered.contains("+ install 2 apt packages  node-1 ×2 node-2 (farm/one, farm/two)"),
            "{rendered}"
        );
    }

    #[test]
    fn a_file_member_dims_its_directory_and_leaves_the_basename_bright() {
        let plan = install_only(vec![op(
            &["web"],
            "file:/etc/containers/systemd/fishnet-canary.container",
            Action::Install,
        )]);
        let colored = render(
            &plan,
            &RenderOptions {
                color: true,
                ..RenderOptions::default()
            },
        );
        assert!(
            colored.contains(&format!(
                "{DIM}/etc/containers/systemd/{RESET}fishnet-canary.container"
            )),
            "{colored:?}"
        );
        let plain = render(&plan, &RenderOptions::default());
        assert!(!plain.contains('\u{1b}'));
        assert!(plain.contains("/etc/containers/systemd/fishnet-canary.container"));
    }

    #[test]
    fn a_reload_trigger_path_is_two_toned_too() {
        let mut plan = install_only(vec![]);
        plan.reloads = vec![PredictedReload {
            unit: "nginx.service".into(),
            kind: "restart".into(),
            triggered_by: vec!["file:/etc/nginx/nginx.conf".into()],
        }];
        let colored = render(
            &plan,
            &RenderOptions {
                color: true,
                ..RenderOptions::default()
            },
        );
        assert!(
            colored.contains(&format!(
                "nginx.service ← {DIM}/etc/nginx/{RESET}nginx.conf"
            )),
            "{colored:?}"
        );
    }

    #[test]
    fn the_provenance_list_collapses_numbered_units_too() {
        let plan = install_only(vec![
            op(
                &["scaly", "fishnet-move", "client-1"],
                "apt:podman",
                Action::Install,
            ),
            op(
                &["scaly", "fishnet-move", "client-2"],
                "apt:podman",
                Action::Install,
            ),
            op(&["scaly", "solo"], "apt:podman", Action::Install),
        ]);
        let rendered = render(&plan, &RenderOptions::default());
        assert!(
            rendered.contains("(scaly/fishnet-move/client-{1,2}, scaly/solo)"),
            "{rendered}"
        );
    }

    fn planned(unit_path: &[&str], glyph_key: &str) -> serde_json::Value {
        serde_json::json!({
            "unit_path": unit_path,
            "glyph_key": glyph_key,
            "action": "install",
            "new_cid": "bbbbbbbbbbbb",
            "describe": format!("ensure {glyph_key}"),
        })
    }

    fn podman_farm(
        flavour: &str,
        unit_path: &[&str],
    ) -> (Vec<serde_json::Value>, serde_json::Value) {
        let file = format!("/etc/containers/systemd/fishnet-{flavour}.container");
        let service = format!("fishnet-{flavour}.service");
        let ops = vec![
            planned(unit_path, "apt:podman"),
            planned(unit_path, &format!("file:{file}")),
            planned(unit_path, &format!("systemd:{service}")),
        ];
        let reload = serde_json::json!({
            "unit": service,
            "kind": "restart",
            "triggered_by": [format!("file:{file}")],
        });
        (ops, reload)
    }

    fn fishnet_body() -> String {
        let mut ops = Vec::new();
        let mut reloads = Vec::new();
        let mut farms: Vec<(String, Vec<String>)> = Vec::new();
        for n in 1..=3 {
            farms.push((
                format!("move-{n}"),
                vec![
                    "scaly".to_string(),
                    "fishnet-move".to_string(),
                    format!("client-{n}"),
                ],
            ));
        }
        for n in 1..=2 {
            farms.push((
                format!("analysis-{n}"),
                vec![
                    "scaly".to_string(),
                    "fishnet-analysis".to_string(),
                    format!("client-{n}"),
                ],
            ));
        }
        for (flavour, unit_path) in &farms {
            let path: Vec<&str> = unit_path.iter().map(String::as_str).collect();
            let (farm_ops, reload) = podman_farm(flavour, &path);
            ops.extend(farm_ops);
            reloads.push(reload);
        }
        ops.push(planned(&["scaly", "base"], "apt:htop"));
        ops.push(planned(&["scaly", "base"], "file:/etc/motd.d/farm"));
        let (canary_ops, canary_reload) = podman_farm("canary", &["scaly", "canary"]);
        ops.extend(canary_ops);
        reloads.push(canary_reload);
        serde_json::json!({
            "host": "scaly-01",
            "scroll_content_id": "c0ffee1234",
            "against_revision": null,
            "ops": ops,
            "reloads": reloads,
            "summary": { "install": 20, "replace": 0, "remove": 0, "noop": 0 },
        })
        .to_string()
    }

    #[test]
    fn the_fishnet_farm_renders_deduped_collapsed_and_distinct_counted() {
        let rendered = present(&fishnet_body(), false, &RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            [
                "Plan for scaly-01 · against no prior revision · manifest c0ffee…",
                "",
                "  + install 2 apt packages   podman ×6 htop",
                "                             (scaly/fishnet-move/client-{1,2,3}, scaly/fishnet-analysis/client-{1,2}, scaly/base, scaly/canary)",
                "  + install 7 files          /etc/containers/systemd/fishnet-move-{1,2,3}.container",
                "                             /etc/containers/systemd/fishnet-analysis-{1,2}.container",
                "                             /etc/motd.d/farm /etc/containers/systemd/fishnet-canary.container",
                "                             (scaly/fishnet-move/client-{1,2,3}, scaly/fishnet-analysis/client-{1,2}, scaly/base, scaly/canary)",
                "  + install 6 systemd units  fishnet-move-{1,2,3}.service fishnet-analysis-{1,2}.service",
                "                             fishnet-canary.service",
                "                             (scaly/fishnet-move/client-{1,2,3}, scaly/fishnet-analysis/client-{1,2}, scaly/canary)",
                "  ↻ restart 6 units          fishnet-move-1.service ← /etc/containers/systemd/fishnet-move-1.container",
                "                             fishnet-move-2.service ← /etc/containers/systemd/fishnet-move-2.container",
                "                             fishnet-move-3.service ← /etc/containers/systemd/fishnet-move-3.container",
                "                             fishnet-analysis-1.service ← /etc/containers/systemd/fishnet-analysis-1.container",
                "                             fishnet-analysis-2.service ← /etc/containers/systemd/fishnet-analysis-2.container",
                "                             fishnet-canary.service ← /etc/containers/systemd/fishnet-canary.container",
                "",
                "  15 changes · 15 install",
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

    #[test]
    fn a_response_without_reality_fields_still_parses() {
        let body = r#"{"host":"web-01","scroll_content_id":"3f9c1a",
                       "against_revision":12,"ops":[],"reloads":[],
                       "summary":{"install":0,"replace":0,"remove":0,"noop":0}}"#;
        let parsed: PlanResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.reality.is_none());
    }

    #[test]
    fn an_op_carrying_an_observation_parses_it() {
        let body = serde_json::json!({
            "host": "web-01",
            "scroll_content_id": "3f9c1a",
            "against_revision": 12,
            "ops": [{
                "unit_path": ["web"],
                "glyph_key": "apt:nginx",
                "action": "install",
                "new_cid": "bbbbbbbbbbbb",
                "describe": "ensure apt package `nginx` installed",
                "observed": "realized"
            }],
            "reloads": [],
            "summary": { "install": 1, "replace": 0, "remove": 0, "noop": 0 }
        })
        .to_string();
        let parsed: PlanResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.ops[0].observed, Some(Observed::Realized));
        assert_eq!(parsed.ops[0].unobservable, None);
    }

    #[test]
    fn an_unrecognized_observation_degrades_to_unrecognized_not_an_error() {
        let body = serde_json::json!({
            "host": "web-01",
            "scroll_content_id": "3f9c1a",
            "against_revision": 12,
            "ops": [{
                "unit_path": ["web"],
                "glyph_key": "apt:nginx",
                "action": "install",
                "new_cid": "bbbbbbbbbbbb",
                "describe": "ensure apt package `nginx` installed",
                "observed": "quantum-superposed",
                "unobservable": "not-yet-invented"
            }],
            "reloads": [],
            "summary": { "install": 1, "replace": 0, "remove": 0, "noop": 0 }
        })
        .to_string();
        let parsed: PlanResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.ops[0].observed, Some(Observed::Unrecognized));
        assert_eq!(parsed.ops[0].unobservable, Some(Unobservable::Unrecognized));
    }

    #[test]
    fn a_reality_block_parses_all_seven_counters() {
        let body = serde_json::json!({
            "host": "web-01",
            "scroll_content_id": "3f9c1a",
            "against_revision": 12,
            "ops": [],
            "reloads": [],
            "summary": { "install": 0, "replace": 0, "remove": 0, "noop": 0 },
            "reality": {
                "realized": 1,
                "divergent": 2,
                "absent": 3,
                "unknown": 4,
                "already_gone": 5,
                "still_present": 6,
                "host_already_matches": false
            }
        })
        .to_string();
        let parsed: PlanResponse = serde_json::from_str(&body).unwrap();
        let reality = parsed.reality.unwrap();
        assert_eq!(reality.realized, 1);
        assert_eq!(reality.divergent, 2);
        assert_eq!(reality.absent, 3);
        assert_eq!(reality.unknown, 4);
        assert_eq!(reality.already_gone, 5);
        assert_eq!(reality.still_present, 6);
        assert!(!reality.host_already_matches);
    }

    #[test]
    fn json_mode_passes_the_reality_fields_through_verbatim() {
        let body = serde_json::json!({
            "host": "web-01",
            "scroll_content_id": "3f9c1a",
            "against_revision": 12,
            "ops": [{
                "unit_path": ["web"],
                "glyph_key": "apt:nginx",
                "action": "install",
                "new_cid": "bbbbbbbbbbbb",
                "describe": "ensure apt package `nginx` installed",
                "observed": "divergent"
            }],
            "reloads": [],
            "summary": { "install": 1, "replace": 0, "remove": 0, "noop": 0 },
            "reality": {
                "realized": 0,
                "divergent": 1,
                "absent": 0,
                "unknown": 0,
                "already_gone": 0,
                "still_present": 0,
                "host_already_matches": false
            }
        })
        .to_string();
        let passed = present(&body, true, &RenderOptions::default()).unwrap();
        assert_eq!(passed, body);
    }
}
