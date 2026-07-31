//! The fleet inventory (ADR 0038): a TOML `[hosts]` table naming each golemd a
//! fleet verb may reach.
//!
//! ```text
//! [hosts]
//! scaly = "http://127.0.0.1:8807"
//!
//! [hosts.manta]
//! url = "http://127.0.0.1:8842"
//! ```
//!
//! Both value shapes carry the same one fact today. The table form is accepted
//! from the start so per-host connection options — a unix socket path, a
//! tunnel — can join it later without breaking files written now; ADR 0040
//! puts every such transport concern here rather than in golemd.
//!
//! A name is the join with the manifest: it labels output, it is what `--hosts`
//! selects, and a fleet apply matches it against the manifest's scroll names
//! ([`crate::fleet::Fanout`]). It tells the daemon nothing — each golemd's own
//! `--host` decides which scroll it enacts — so a name that matches no scroll
//! costs that host nothing but a reported skip.
//!
//! [`resolve`] fixes the search order (`--inventory`, then
//! `$GOLEMCTL_INVENTORY`, then `./fleet.toml`, then `./.fleet/inventory.toml`
//! — the file the VM harness's `fleet inventory` writes), and every error that
//! can send an operator hunting for the file repeats it verbatim
//! ([`RESOLUTION_CHAIN`]).
//! Hosts come back sorted by name whatever order the file lists them in, so
//! per-host output lands in the same order on every machine.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const INVENTORY_ENV: &str = "GOLEMCTL_INVENTORY";
pub const DEFAULT_INVENTORY_FILE: &str = "fleet.toml";
pub const HARNESS_INVENTORY_FILE: &str = ".fleet/inventory.toml";
pub const RESOLUTION_CHAIN: &str =
    "--inventory, then $GOLEMCTL_INVENTORY, then ./fleet.toml, then ./.fleet/inventory.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub addr: String,
}

#[derive(Debug, Clone)]
pub struct Inventory {
    path: PathBuf,
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
struct InventoryFile {
    // NOTE: the `BTreeMap` is what fixes host order — names sort, and the
    // file's own order never shows through. `toml::Value` rather than a typed
    // enum because the two value shapes are told apart in `addr_of`, where the
    // error can name both spellings.
    #[serde(default)]
    hosts: BTreeMap<String, toml::Value>,
}

pub fn resolve(flag: Option<PathBuf>) -> PathBuf {
    resolve_from(
        flag,
        std::env::var_os(INVENTORY_ENV).map(PathBuf::from),
        |path| path.exists(),
    )
}

/// The search order of [`RESOLUTION_CHAIN`], with `exists` injected so the
/// probing is testable. The harness fallback fires only when `./fleet.toml` is
/// absent, so a hand-written inventory always wins over the one
/// `fleet inventory` generates. With neither present this still returns
/// `./fleet.toml` rather than failing here — [`load`] is where the miss is
/// reported, and its error names the whole chain.
pub fn resolve_from(
    flag: Option<PathBuf>,
    from_env: Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(chosen) = flag.or(from_env) {
        return chosen;
    }
    let default = PathBuf::from(DEFAULT_INVENTORY_FILE);
    if exists(&default) {
        return default;
    }
    let harness = PathBuf::from(HARNESS_INVENTORY_FILE);
    if exists(&harness) {
        return harness;
    }
    default
}

pub fn load(path: &Path) -> Result<Inventory> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "no inventory at {} — golemctl looks at {RESOLUTION_CHAIN}",
            path.display()
        ),
        Err(e) => {
            return Err(e).with_context(|| format!("read inventory {}", path.display()));
        }
    };
    parse(path, &text)
}

pub fn parse(path: &Path, text: &str) -> Result<Inventory> {
    let file: InventoryFile =
        toml::from_str(text).with_context(|| format!("parse inventory {}", path.display()))?;
    if file.hosts.is_empty() {
        bail!(
            "inventory {} declares no hosts — add a [hosts] table, e.g. scaly = \"http://127.0.0.1:8807\" (golemctl looks at {RESOLUTION_CHAIN})",
            path.display()
        );
    }
    let mut targets = Vec::with_capacity(file.hosts.len());
    for (name, value) in &file.hosts {
        targets.push(Target {
            name: name.clone(),
            addr: addr_of(name, value).with_context(|| format!("inventory {}", path.display()))?,
        });
    }
    Ok(Inventory {
        path: path.to_path_buf(),
        targets,
    })
}

fn addr_of(name: &str, value: &toml::Value) -> Result<String> {
    match value {
        toml::Value::String(url) => Ok(url.clone()),
        toml::Value::Table(table) => match table.get("url").and_then(|u| u.as_str()) {
            Some(url) => Ok(url.to_string()),
            None => bail!(
                "host {name} has no url — write `{name} = \"http://…\"` or a [hosts.{name}] table with `url = \"http://…\"`"
            ),
        },
        _ => bail!("host {name} must be a url string or a table carrying a url"),
    }
}

impl Inventory {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    pub fn names(&self) -> Vec<String> {
        self.targets.iter().map(|t| t.name.clone()).collect()
    }

    pub fn select(&self, hosts: Option<&str>) -> Result<Vec<Target>> {
        let Some(hosts) = hosts else {
            return Ok(self.targets.clone());
        };
        let requested: Vec<&str> = hosts
            .split(',')
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .collect();
        if requested.is_empty() {
            bail!(
                "--hosts named no host — the inventory {} declares: {}",
                self.path.display(),
                self.names().join(", ")
            );
        }
        for name in &requested {
            if !self.targets.iter().any(|t| t.name == *name) {
                bail!(
                    "unknown host {name} — the inventory {} declares: {}",
                    self.path.display(),
                    self.names().join(", ")
                );
            }
        }
        Ok(self
            .targets
            .iter()
            .filter(|t| requested.contains(&t.name.as_str()))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory_of(text: &str) -> Inventory {
        parse(Path::new("fleet.toml"), text).unwrap()
    }

    #[test]
    fn the_flag_wins_over_the_environment_and_the_default() {
        let chosen = resolve_from(
            Some(PathBuf::from("/flag.toml")),
            Some(PathBuf::from("/env.toml")),
            |_| true,
        );
        assert_eq!(chosen, PathBuf::from("/flag.toml"));
    }

    #[test]
    fn the_environment_wins_over_the_default() {
        let chosen = resolve_from(None, Some(PathBuf::from("/env.toml")), |_| true);
        assert_eq!(chosen, PathBuf::from("/env.toml"));
    }

    #[test]
    fn the_default_is_fleet_toml_in_the_working_directory() {
        assert_eq!(
            resolve_from(None, None, |_| true),
            PathBuf::from(DEFAULT_INVENTORY_FILE)
        );
    }

    #[test]
    fn a_missing_default_falls_back_to_the_harness_inventory() {
        let chosen = resolve_from(None, None, |p| p == Path::new(HARNESS_INVENTORY_FILE));
        assert_eq!(chosen, PathBuf::from(HARNESS_INVENTORY_FILE));
    }

    #[test]
    fn a_flag_or_env_path_is_taken_verbatim_even_if_absent() {
        let chosen = resolve_from(None, Some(PathBuf::from("/env.toml")), |_| false);
        assert_eq!(chosen, PathBuf::from("/env.toml"));
    }

    #[test]
    fn with_neither_file_present_the_default_names_the_error() {
        assert_eq!(
            resolve_from(None, None, |_| false),
            PathBuf::from(DEFAULT_INVENTORY_FILE)
        );
    }

    #[test]
    fn both_the_string_and_the_table_value_shapes_carry_a_url() {
        let inventory = inventory_of(
            r#"
            [hosts]
            scaly = "http://127.0.0.1:8807"

            [hosts.manta]
            url = "http://127.0.0.1:8842"
            "#,
        );
        assert_eq!(
            inventory.targets(),
            [
                Target {
                    name: "manta".into(),
                    addr: "http://127.0.0.1:8842".into()
                },
                Target {
                    name: "scaly".into(),
                    addr: "http://127.0.0.1:8807".into()
                },
            ]
        );
    }

    #[test]
    fn hosts_come_back_ordered_by_name_whatever_the_file_order() {
        let inventory = inventory_of(
            r#"
            [hosts]
            zebra = "http://z"
            alpha = "http://a"
            manta = "http://m"
            "#,
        );
        assert_eq!(inventory.names(), ["alpha", "manta", "zebra"]);
    }

    #[test]
    fn a_table_host_without_a_url_names_both_spellings() {
        let err = parse(
            Path::new("fleet.toml"),
            r#"
            [hosts.scaly]
            port = 8807
            "#,
        )
        .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("scaly"), "{err}");
        assert!(err.contains("[hosts.scaly]"), "{err}");
        assert!(err.contains("url"), "{err}");
    }

    #[test]
    fn selecting_nothing_keeps_every_host_in_inventory_order() {
        let inventory = inventory_of(
            r#"
            [hosts]
            zebra = "http://z"
            alpha = "http://a"
            "#,
        );
        let selected = inventory.select(None).unwrap();
        assert_eq!(
            selected.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "zebra"]
        );
    }

    #[test]
    fn a_hosts_filter_keeps_the_named_subset_in_inventory_order() {
        let inventory = inventory_of(
            r#"
            [hosts]
            zebra = "http://z"
            alpha = "http://a"
            manta = "http://m"
            "#,
        );
        let selected = inventory.select(Some("zebra, alpha")).unwrap();
        assert_eq!(
            selected.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "zebra"]
        );
    }

    #[test]
    fn an_unknown_host_name_errors_listing_the_known_names() {
        let inventory = inventory_of(
            r#"
            [hosts]
            scaly = "http://s"
            manta = "http://m"
            "#,
        );
        let err = inventory
            .select(Some("scaly,otter"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("otter"), "{err}");
        assert!(err.contains("scaly"), "{err}");
        assert!(err.contains("manta"), "{err}");
    }

    #[test]
    fn an_empty_hosts_table_errors_naming_the_resolution_chain() {
        let err = parse(Path::new("fleet.toml"), "[hosts]\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("fleet.toml"), "{err}");
        assert!(err.contains(RESOLUTION_CHAIN), "{err}");
    }

    #[test]
    fn a_missing_inventory_errors_naming_the_resolution_chain() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nowhere.toml");
        let err = load(&missing).unwrap_err().to_string();
        assert!(err.contains("nowhere.toml"), "{err}");
        assert!(err.contains(RESOLUTION_CHAIN), "{err}");
    }
}
