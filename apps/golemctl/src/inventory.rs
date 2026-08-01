//! The fleet inventory (ADR 0038): a TOML `[hosts]` table naming each golemd a
//! fleet verb may reach.
//!
//! ```text
//! [hosts]
//! scaly = "http://127.0.0.1:8807"
//!
//! [hosts.manta]
//! url = "http://127.0.0.1:8842"
//!
//! [hosts.orbit]
//! ssh        = "golem@10.0.0.5"          # the ssh destination, required
//! ssh_port   = 2222                      # ssh's own port; default is ssh's
//! remote_port = 7474                     # golemd's loopback port on that host
//! ssh_args   = ["-i", "/keys/id_ed25519"]  # extra flags for the ssh command
//! token_file = "/keys/golem-token"       # this host's bearer secret
//! ```
//!
//! A bare string and a table carrying `url` say the same thing: dial this
//! address directly. A table carrying `ssh` says the daemon is loopback-bound
//! and golemctl must open its own forward to reach it (ADR 0042,
//! [`crate::tunnel`]) — the deployed shape, since a routable golemd port
//! publishes root-equivalent control of its host. `url` and `ssh` in one table
//! is an error: a host is reached one way. `token_file` is orthogonal to both
//! and overrides the ambient `GOLEM_AUTH_TOKEN*` for that host alone
//! ([`crate::conn::resolve_auth`]).
//!
//! A single-host verb's positional address takes the same two shapes:
//! `http://host:port`, or [`SSH_ADDR_FORM`] — `ssh://[user@]host[:port]`, whose
//! port is *ssh's*, the daemon's staying [`DEFAULT_REMOTE_PORT`]. The richer
//! ssh fields have no spelling there; a host needing them belongs in an
//! inventory. ADR 0040 puts every such transport concern here rather than in
//! golemd.
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

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

pub const INVENTORY_ENV: &str = "GOLEMCTL_INVENTORY";
pub const DEFAULT_INVENTORY_FILE: &str = "fleet.toml";
pub const HARNESS_INVENTORY_FILE: &str = ".fleet/inventory.toml";
pub const RESOLUTION_CHAIN: &str =
    "--inventory, then $GOLEMCTL_INVENTORY, then ./fleet.toml, then ./.fleet/inventory.toml";

pub const DEFAULT_REMOTE_PORT: u16 = 7474;
pub const SSH_SCHEME: &str = "ssh://";
pub const SSH_ADDR_FORM: &str = "ssh://[user@]host[:port]";

/// How a daemon is reached: dialed directly, or through a forward golemctl
/// opens over ssh. `Display` renders each back in the spelling it was written
/// in, which is what per-host headings and `--json` show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Http {
        url: String,
    },
    Ssh {
        destination: String,
        ssh_port: Option<u16>,
        remote_port: u16,
        ssh_args: Vec<String>,
    },
}

impl Endpoint {
    /// Read a verb's positional address. Only the `ssh://` prefix is
    /// interpreted; anything else is carried through untouched as a base URL,
    /// so golemctl never has to keep up with what the HTTP client will accept.
    pub fn parse(addr: &str) -> Result<Endpoint> {
        let Some(rest) = addr.strip_prefix(SSH_SCHEME) else {
            return Ok(Endpoint::Http {
                url: addr.to_string(),
            });
        };
        let rest = rest.trim_end_matches('/');
        if rest.is_empty() {
            bail!("the ssh target {addr} names no host — write {SSH_ADDR_FORM}");
        }
        let (destination, ssh_port) = split_ssh_port(rest)?;
        Ok(Endpoint::Ssh {
            destination,
            ssh_port,
            remote_port: DEFAULT_REMOTE_PORT,
            ssh_args: Vec::new(),
        })
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Endpoint::Http { url } => write!(f, "{url}"),
            Endpoint::Ssh {
                destination,
                ssh_port,
                ..
            } => match ssh_port {
                Some(port) => write!(f, "{SSH_SCHEME}{destination}:{port}"),
                None => write!(f, "{SSH_SCHEME}{destination}"),
            },
        }
    }
}

// NOTE: the port colon is searched only after the last `@`, so a colon inside
// the user part is not read as a port. A trailing `:something` that is not a
// port is an error rather than part of the host name — silently dialing port 22
// on `ssh://scaly:2222x` would be worse than refusing it.
fn split_ssh_port(rest: &str) -> Result<(String, Option<u16>)> {
    let host_at = rest.rfind('@').map(|at| at + 1).unwrap_or(0);
    let Some(colon) = rest[host_at..].rfind(':').map(|at| host_at + at) else {
        return Ok((rest.to_string(), None));
    };
    let port = rest[colon + 1..].parse::<u16>().map_err(|_| {
        anyhow!("the ssh target {SSH_SCHEME}{rest} names no readable port — write {SSH_ADDR_FORM}")
    })?;
    Ok((rest[..colon].to_string(), Some(port)))
}

/// One host a verb may act on: the name that joins it to a scroll, how to reach
/// it, and — when it holds a secret of its own — where that secret is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub endpoint: Endpoint,
    pub token_file: Option<PathBuf>,
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
    // enum because the string and table shapes are told apart in `target_of`,
    // where the error can name every spelling a host accepts.
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
        targets
            .push(target_of(name, value).with_context(|| format!("inventory {}", path.display()))?);
    }
    Ok(Inventory {
        path: path.to_path_buf(),
        targets,
    })
}

#[derive(Debug, Deserialize)]
struct HostTable {
    url: Option<String>,
    ssh: Option<String>,
    ssh_port: Option<u16>,
    remote_port: Option<u16>,
    #[serde(default)]
    ssh_args: Vec<String>,
    token_file: Option<PathBuf>,
}

fn target_of(name: &str, value: &toml::Value) -> Result<Target> {
    match value {
        toml::Value::String(url) => Ok(Target {
            name: name.to_string(),
            endpoint: Endpoint::Http { url: url.clone() },
            token_file: None,
        }),
        toml::Value::Table(_) => {
            let host: HostTable = value
                .clone()
                .try_into()
                .with_context(|| format!("host {name}"))?;
            Ok(Target {
                name: name.to_string(),
                endpoint: endpoint_of(name, &host)?,
                token_file: host.token_file,
            })
        }
        _ => bail!(
            "host {name} must be a url string or a table carrying a url or an ssh destination"
        ),
    }
}

fn endpoint_of(name: &str, host: &HostTable) -> Result<Endpoint> {
    match (&host.url, &host.ssh) {
        (Some(_), Some(_)) => bail!(
            "host {name} is reached two ways at once — a [hosts.{name}] table carries `url` or `ssh`, never both"
        ),
        (Some(url), None) => Ok(Endpoint::Http { url: url.clone() }),
        (None, Some(destination)) => Ok(Endpoint::Ssh {
            destination: destination.clone(),
            ssh_port: host.ssh_port,
            remote_port: host.remote_port.unwrap_or(DEFAULT_REMOTE_PORT),
            ssh_args: host.ssh_args.clone(),
        }),
        (None, None) => bail!(
            "host {name} says how to reach it neither way — write `{name} = \"http://…\"`, or a [hosts.{name}] table with `url = \"http://…\"` or `ssh = \"user@host\"`"
        ),
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
                    endpoint: Endpoint::Http {
                        url: "http://127.0.0.1:8842".into()
                    },
                    token_file: None,
                },
                Target {
                    name: "scaly".into(),
                    endpoint: Endpoint::Http {
                        url: "http://127.0.0.1:8807".into()
                    },
                    token_file: None,
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
        assert!(err.contains("ssh"), "{err}");
    }

    #[test]
    fn an_ssh_host_carries_its_destination_ports_and_extra_args() {
        let inventory = inventory_of(
            r#"
            [hosts.scaly]
            ssh = "golem@10.0.0.5"
            ssh_port = 2222
            remote_port = 7000
            ssh_args = ["-i", "/keys/id_ed25519"]
            token_file = "/keys/golem-token"
            "#,
        );
        assert_eq!(
            inventory.targets(),
            [Target {
                name: "scaly".into(),
                endpoint: Endpoint::Ssh {
                    destination: "golem@10.0.0.5".into(),
                    ssh_port: Some(2222),
                    remote_port: 7000,
                    ssh_args: vec!["-i".into(), "/keys/id_ed25519".into()],
                },
                token_file: Some(PathBuf::from("/keys/golem-token")),
            }]
        );
    }

    #[test]
    fn an_ssh_host_naming_only_a_destination_takes_the_loopback_defaults() {
        let inventory = inventory_of(
            r#"
            [hosts.scaly]
            ssh = "golem@10.0.0.5"
            "#,
        );
        assert_eq!(
            inventory.targets(),
            [Target {
                name: "scaly".into(),
                endpoint: Endpoint::Ssh {
                    destination: "golem@10.0.0.5".into(),
                    ssh_port: None,
                    remote_port: DEFAULT_REMOTE_PORT,
                    ssh_args: vec![],
                },
                token_file: None,
            }]
        );
    }

    #[test]
    fn a_host_reached_both_ways_at_once_is_an_error_naming_both_spellings() {
        let err = parse(
            Path::new("fleet.toml"),
            r#"
            [hosts.scaly]
            url = "http://127.0.0.1:8807"
            ssh = "golem@10.0.0.5"
            "#,
        )
        .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("scaly"), "{err}");
        assert!(err.contains("url"), "{err}");
        assert!(err.contains("ssh"), "{err}");
    }

    #[test]
    fn an_http_host_carries_no_per_host_token_file_unless_it_says_so() {
        let inventory = inventory_of(
            r#"
            [hosts.scaly]
            url = "http://127.0.0.1:8807"
            token_file = "/keys/golem-token"
            "#,
        );
        assert_eq!(
            inventory.targets()[0].token_file,
            Some(PathBuf::from("/keys/golem-token"))
        );
        assert_eq!(
            inventory_of("[hosts]\nscaly = \"http://s\"\n").targets()[0].token_file,
            None
        );
    }

    #[test]
    fn an_ssh_addr_parses_its_user_host_and_ssh_port() {
        assert_eq!(
            Endpoint::parse("ssh://golem@10.0.0.5:2222").unwrap(),
            Endpoint::Ssh {
                destination: "golem@10.0.0.5".into(),
                ssh_port: Some(2222),
                remote_port: DEFAULT_REMOTE_PORT,
                ssh_args: vec![],
            }
        );
        assert_eq!(
            Endpoint::parse("ssh://scaly").unwrap(),
            Endpoint::Ssh {
                destination: "scaly".into(),
                ssh_port: None,
                remote_port: DEFAULT_REMOTE_PORT,
                ssh_args: vec![],
            }
        );
    }

    #[test]
    fn an_addr_without_the_ssh_scheme_stays_the_http_address_it_always_was() {
        assert_eq!(
            Endpoint::parse("http://127.0.0.1:8807").unwrap(),
            Endpoint::Http {
                url: "http://127.0.0.1:8807".into()
            }
        );
    }

    #[test]
    fn an_ssh_addr_with_an_unreadable_port_says_how_to_write_one() {
        let err = Endpoint::parse("ssh://scaly:none").unwrap_err().to_string();
        assert!(err.contains("ssh://[user@]host[:port]"), "{err}");
        let err = Endpoint::parse("ssh://").unwrap_err().to_string();
        assert!(err.contains("ssh://[user@]host[:port]"), "{err}");
    }

    #[test]
    fn an_endpoint_shows_itself_the_way_it_was_written() {
        assert_eq!(
            Endpoint::parse("ssh://golem@10.0.0.5:2222")
                .unwrap()
                .to_string(),
            "ssh://golem@10.0.0.5:2222"
        );
        assert_eq!(
            Endpoint::parse("ssh://scaly").unwrap().to_string(),
            "ssh://scaly"
        );
        assert_eq!(
            Endpoint::parse("http://127.0.0.1:8807")
                .unwrap()
                .to_string(),
            "http://127.0.0.1:8807"
        );
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
