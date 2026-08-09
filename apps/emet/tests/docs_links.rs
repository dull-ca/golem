mod docs_gate;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use docs_gate::{
    fail_with, markdown_pages_under, relative_to_repo, repo_root, MARKDOWN_EXTENSIONS,
};

// Prose that is maintained, and therefore held to its links (ADR 0054).
const LINKED_PAGE_ROOTS: [&str; 6] = [
    "README.md",
    "QUICKSTART.md",
    "TUTORIAL-fleet.md",
    "CLAUDE.md",
    "docs",
    "sites/website/src/content/docs",
];
// Dated implementation plans, kept as written for the record. Nobody revisits
// one after the work lands, so holding them to a live link is a standing
// failure nobody can fix without rewriting history.
const UNMAINTAINED_PAGE_ROOTS: [&str; 1] = ["docs/superpowers"];
// ADRs and design docs still have their links checked; what they are exempt
// from is `every_mentioned_repository_path_exists`. They describe the tree as
// it stood on the day of the decision, and a path that has since moved is a
// true statement about the past — correcting it would edit an accepted record.
const DATED_RECORD_ROOTS: [&str; 2] = ["docs/adr", "docs/design"];
const SITE_CONTENT_ROOT: &str = "sites/website/src/content/docs";
const SITE_INDEX_STEM: &str = "index";
// `origin` moved from Codeberg to GitHub (ADR 0035). A link left behind is not
// a broken link — it resolves, to a copy of the project that stopped moving —
// so no reachability check would ever object. Naming the host is what turns a
// silently wrong destination into a failure.
const RETIRED_LINK_HOSTS: [&str; 1] = ["codeberg.org"];
const FENCE_DELIMITER: &str = "```";
const FRONTMATTER_DELIMITER: &str = "---";
const MENTIONED_PATH_EXTENSIONS: [&str; 14] = [
    "rs", "emet", "toml", "nix", "sh", "md", "mdx", "json", "py", "conf", "service", "yml", "ts",
    "mjs",
];
const GLOB_CHARACTERS: [char; 8] = ['*', '{', '}', '?', '<', '>', '[', ']'];
const BUILD_OUTPUT_DIRECTORIES: [&str; 4] = ["target", "result", "node_modules", "dist"];
const LINK_OPENERS: [(&str, char); 3] = [("](", ')'), ("href=\"", '"'), ("href='", '\'')];

struct DocumentLine {
    number: usize,
    text: String,
    in_frontmatter: bool,
}

struct MentionedLink {
    line: usize,
    target: String,
}

fn is_under(path: &Path, roots: &[&str]) -> bool {
    roots
        .iter()
        .any(|root| path.starts_with(repo_root().join(root)))
}

fn linked_pages() -> Vec<PathBuf> {
    let mut pages = Vec::new();
    for root in LINKED_PAGE_ROOTS {
        pages.extend(markdown_pages_under(&repo_root().join(root)));
    }
    pages.retain(|page| !is_under(page, &UNMAINTAINED_PAGE_ROOTS));
    assert!(
        !pages.is_empty(),
        "no documentation pages found under {LINKED_PAGE_ROOTS:?} — the documentation trees are \
         empty or unreachable"
    );
    pages
}

fn pages_mentioning_current_paths() -> Vec<PathBuf> {
    let mut pages = linked_pages();
    pages.retain(|page| !is_under(page, &DATED_RECORD_ROOTS));
    pages
}

// Prose only: fenced code is a fence gate's problem, and a path inside one is
// usually illustrative rather than real. Frontmatter stays in because a
// Starlight page's `description` is prose a reader sees, but each of its lines
// is flagged so a YAML comment is never counted as a markdown heading.
fn readable_lines(page: &Path) -> Vec<DocumentLine> {
    let text = std::fs::read_to_string(page)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", page.display()));

    let mut lines = Vec::new();
    let mut in_fence = false;
    let mut in_frontmatter = text.starts_with(FRONTMATTER_DELIMITER);
    let mut frontmatter_opened = false;

    for (index, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();

        if in_frontmatter {
            if trimmed == FRONTMATTER_DELIMITER {
                if frontmatter_opened {
                    in_frontmatter = false;
                    continue;
                }
                frontmatter_opened = true;
                continue;
            }
            if !trimmed.starts_with('#') {
                lines.push(DocumentLine {
                    number: index + 1,
                    text: raw.to_string(),
                    in_frontmatter: true,
                });
            }
            continue;
        }

        if trimmed.starts_with(FENCE_DELIMITER) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        lines.push(DocumentLine {
            number: index + 1,
            text: raw.to_string(),
            in_frontmatter: false,
        });
    }
    lines
}

fn without_inline_code(line: &str) -> String {
    let mut kept = String::new();
    let mut inside = false;
    for character in line.chars() {
        if character == '`' {
            inside = !inside;
            continue;
        }
        if !inside {
            kept.push(character);
        }
    }
    kept
}

fn inline_code_spans(line: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    for character in line.chars() {
        if character == '`' {
            if inside {
                spans.push(std::mem::take(&mut current));
            }
            inside = !inside;
            continue;
        }
        if inside {
            current.push(character);
        }
    }
    spans
}

fn link_targets_in(line: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = line;

    while let Some((at, opener, closer)) = LINK_OPENERS
        .iter()
        .filter_map(|(opener, closer)| rest.find(opener).map(|at| (at, *opener, *closer)))
        .min_by_key(|(at, _, _)| *at)
    {
        let start = at + opener.len();
        let end = rest[start..]
            .find(closer)
            .map(|offset| start + offset)
            .unwrap_or(rest.len());
        let target = rest[start..end].split_whitespace().next().unwrap_or("");
        if !target.is_empty() {
            targets.push(target.to_string());
        }
        rest = rest[end..].strip_prefix(closer).unwrap_or_default();
    }
    targets
}

fn links_in(page: &Path) -> Vec<MentionedLink> {
    readable_lines(page)
        .into_iter()
        .flat_map(|line| {
            link_targets_in(&without_inline_code(&line.text))
                .into_iter()
                .map(move |target| MentionedLink {
                    line: line.number,
                    target,
                })
        })
        .collect()
}

// The anchor Astro will publish for a heading, and therefore a reimplementation
// that has to agree with `@astrojs/markdown-remark`'s `rehype-collect-headings`
// exactly: it slugs the heading's text through `github-slugger` and then trims
// one trailing hyphen. Agreeing "closely" is the failure mode to avoid — every
// disagreement is either an anchor this check calls broken while the site
// serves it, or one it passes that a reader lands on nowhere. Duplicates are
// disambiguated by the caller, which mirrors the same package's counter
// (`heading`, `heading-1`, `heading-2`). Upgrading Astro can move all of this.
fn slug(heading: &str) -> String {
    let mut plain = String::new();
    let mut skipping_link_target = false;
    let mut characters = heading.chars().peekable();

    while let Some(character) = characters.next() {
        if skipping_link_target {
            skipping_link_target = character != ')';
            continue;
        }
        match character {
            '`' | '[' | '*' => continue,
            ']' if characters.peek() == Some(&'(') => {
                characters.next();
                skipping_link_target = true;
            }
            _ => plain.push(character),
        }
    }

    let separated: String = plain
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|character| match character {
            ' ' => Some('-'),
            '-' | '_' => Some(character),
            _ if character.is_alphanumeric() => Some(character),
            _ => None,
        })
        .collect();
    separated
        .strip_suffix('-')
        .map(str::to_string)
        .unwrap_or(separated)
}

fn heading_slugs(page: &Path) -> HashSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut slugs = HashSet::new();

    for line in readable_lines(page) {
        if line.in_frontmatter {
            continue;
        }
        let heading = line
            .text
            .strip_prefix('#')
            .map(|rest| rest.trim_start_matches('#').trim());
        let Some(heading) = heading else { continue };

        let base = slug(heading);
        if base.is_empty() {
            continue;
        }
        let occurrence = seen.entry(base.clone()).or_default();
        slugs.insert(if *occurrence == 0 {
            base.clone()
        } else {
            format!("{base}-{occurrence}")
        });
        *occurrence += 1;
    }
    slugs
}

fn site_routes() -> BTreeMap<String, PathBuf> {
    let root = repo_root().join(SITE_CONTENT_ROOT);
    let mut routes = BTreeMap::new();

    for page in markdown_pages_under(&root) {
        let relative = page
            .strip_prefix(&root)
            .expect("a site page lives under the content root")
            .with_extension("");
        let route = if relative.file_stem().and_then(|s| s.to_str()) == Some(SITE_INDEX_STEM) {
            match relative.parent().map(Path::to_path_buf) {
                Some(parent) if parent.as_os_str().is_empty() => "/".to_string(),
                Some(parent) => format!("/{}/", parent.display()),
                None => "/".to_string(),
            }
        } else {
            format!("/{}/", relative.display())
        };
        routes.insert(route, page);
    }
    routes
}

fn resolved_against(page: &Path, target: &str) -> PathBuf {
    let mut resolved = page.parent().expect("a page has a parent").to_path_buf();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                resolved.pop();
            }
            _ => resolved.push(segment),
        }
    }
    resolved
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|extension| MARKDOWN_EXTENSIONS.contains(&extension))
}

#[test]
fn every_documentation_link_resolves() {
    let routes = site_routes();
    let site_root = repo_root().join(SITE_CONTENT_ROOT);
    let mut failures = Vec::new();

    for page in linked_pages() {
        let name = relative_to_repo(&page);
        let own_headings = heading_slugs(&page);

        for link in links_in(&page) {
            let at = format!("{name}:{}", link.line);
            let target = link.target.as_str();

            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }

            let (destination, anchor) = match target.split_once('#') {
                Some((destination, anchor)) => (destination, Some(anchor)),
                None => (target, None),
            };

            if destination.is_empty() {
                let anchor = anchor.unwrap_or_default();
                if !own_headings.contains(anchor) {
                    failures.push(format!("{at}: `{target}` names no heading on this page"));
                }
                continue;
            }

            if destination.starts_with('/') {
                // A leading slash means a published route only on a page the
                // site publishes. Everywhere else it is an absolute filesystem
                // path (`/etc/golem/`), which the route table would reject for
                // the wrong reason.
                if !page.starts_with(&site_root) {
                    continue;
                }
                let Some(route_page) = routes.get(destination) else {
                    failures.push(format!(
                        "{at}: `{target}` is not a page this site publishes"
                    ));
                    continue;
                };
                if let Some(anchor) = anchor {
                    if !heading_slugs(route_page).contains(anchor) {
                        failures.push(format!(
                            "{at}: `{target}` names no heading in {}",
                            relative_to_repo(route_page)
                        ));
                    }
                }
                continue;
            }

            let resolved = resolved_against(&page, destination);
            if !resolved.exists() {
                failures.push(format!(
                    "{at}: `{target}` points at {}, which does not exist",
                    relative_to_repo(&resolved)
                ));
                continue;
            }
            if let Some(anchor) = anchor {
                if is_markdown(&resolved) && !heading_slugs(&resolved).contains(anchor) {
                    failures.push(format!(
                        "{at}: `{target}` names no heading in {}",
                        relative_to_repo(&resolved)
                    ));
                }
            }
        }
    }

    fail_with("documentation link(s) do not resolve", failures);
}

#[test]
fn no_documentation_link_points_at_a_retired_host() {
    let mut failures = Vec::new();

    for page in linked_pages() {
        let name = relative_to_repo(&page);
        for link in links_in(&page) {
            if let Some(host) = RETIRED_LINK_HOSTS
                .iter()
                .find(|host| link.target.contains(*host))
            {
                failures.push(format!(
                    "{name}:{}: `{}` points at {host}, which this project no longer publishes to",
                    link.line, link.target
                ));
            }
        }
    }

    fail_with("documentation link(s) point at a retired host", failures);
}

#[test]
fn every_mentioned_repository_path_exists() {
    let root = repo_root();
    let top_level: HashSet<String> = std::fs::read_dir(&root)
        .expect("the repository root is readable")
        .map(|entry| entry.expect("readable directory entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| !BUILD_OUTPUT_DIRECTORIES.contains(&name.as_str()))
        .collect();
    let mut failures = Vec::new();

    for page in pages_mentioning_current_paths() {
        let name = relative_to_repo(&page);
        for line in readable_lines(&page) {
            if line.in_frontmatter {
                continue;
            }
            for span in inline_code_spans(&line.text) {
                let mentioned = span.trim();
                if !looks_like_a_repository_path(mentioned, &top_level) {
                    continue;
                }
                if !root.join(mentioned).exists() {
                    failures.push(format!(
                        "{name}:{}: `{mentioned}` names a file this repository does not have",
                        line.number
                    ));
                }
            }
        }
    }

    fail_with("mentioned repository path(s) do not exist", failures);
}

// Backticks in prose hold anything — a shell command, a type name, a glob, a
// path on the host golem manages — and only some of it is a file in this
// repository. Every filter here narrows toward the unambiguous case, so the
// check under-reports on purpose: a mention it declines to recognize is a
// missed stale path, while one it recognizes wrongly is a failure an author
// cannot fix without rewording correct prose.
fn looks_like_a_repository_path(mentioned: &str, top_level: &HashSet<String>) -> bool {
    let Some((first_segment, _)) = mentioned.split_once('/') else {
        return false;
    };
    if mentioned.starts_with('/') || mentioned.starts_with('.') {
        return false;
    }
    if mentioned.contains(char::is_whitespace) {
        return false;
    }
    if mentioned.contains(GLOB_CHARACTERS) {
        return false;
    }
    if !top_level.contains(first_segment) {
        return false;
    }
    let last_segment = mentioned.rsplit('/').next().unwrap_or_default();
    last_segment
        .rsplit_once('.')
        .is_some_and(|(_, extension)| MENTIONED_PATH_EXTENSIONS.contains(&extension))
}
