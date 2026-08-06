use emet::compile_file;
use emet::ir::{Entry, Glyph};

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quadlet")
}

fn compiled() -> emet::Compiled {
    compile_file(&fixtures_dir().join("WorkloadEntry.emet")).expect("WorkloadEntry.emet compiles")
}

fn compiled_image_refs() -> emet::Compiled {
    compile_file(&fixtures_dir().join("ImageRefEntry.emet")).expect("ImageRefEntry.emet compiles")
}

fn image_line(c: &emet::Compiled, host: &str) -> String {
    let unit = file_contents(
        scroll(c, host),
        &format!("/etc/containers/systemd/{host}.container"),
    );
    unit.lines()
        .find(|l| l.starts_with("Image="))
        .unwrap_or_else(|| panic!("no Image= line in {host}.container"))
        .to_string()
}

fn scroll<'a>(c: &'a emet::Compiled, name: &str) -> &'a emet::ir::Scroll {
    c.scrolls
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no scroll named {name}"))
}

fn keys(s: &emet::ir::Scroll) -> Vec<String> {
    s.glyphs().iter().map(|g| g.key()).collect()
}

fn file_contents<'a>(s: &'a emet::ir::Scroll, path: &str) -> &'a str {
    s.glyphs()
        .iter()
        .find_map(|g| match g {
            Glyph::Filesystem {
                path: p,
                entry: Entry::File { contents, .. },
            } if p == path => contents.plain(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no file glyph at {path}"))
}

#[test]
fn workload_lowers_to_podman_apt_container_volume_service_directory_dropin_and_base() {
    let c = compiled();
    let s = scroll(&c, "registry");
    assert_eq!(
        keys(s),
        vec![
            "apt:podman".to_string(),
            "file:/srv/registry/uploads".to_string(),
            "apt:aardvark-dns".to_string(),
            "file:/etc/containers/systemd/golem-registry-net.network".to_string(),
            "file:/etc/containers/systemd/golem-registry-data.volume".to_string(),
            "file:/etc/containers/systemd/registry.container".to_string(),
            "systemd:registry.service".to_string(),
            "apt:nftables".to_string(),
            "file:/etc/nftables.d".to_string(),
            "file:/etc/golem-nftables.conf".to_string(),
            "file:/etc/nftables.d/00-base.nft".to_string(),
            "file:/etc/systemd/system/golem-nftables.service".to_string(),
            "systemd:golem-nftables.service".to_string(),
            "file:/etc/nftables.d/registry-5000.nft".to_string(),
        ]
    );
}

#[test]
fn host_mount_source_becomes_a_directory_glyph() {
    let c = compiled();
    let s = scroll(&c, "registry");
    let entry = s
        .glyphs()
        .iter()
        .find_map(|g| match g {
            Glyph::Filesystem { path, entry } if path == "/srv/registry/uploads" => Some(entry),
            _ => None,
        })
        .expect("directory glyph for the host mount source");
    assert!(
        matches!(entry, Entry::Directory { .. }),
        "host mount source is a directory"
    );
}

#[test]
fn container_unit_file_renders_every_typed_key() {
    let c = compiled();
    let unit = file_contents(
        scroll(&c, "registry"),
        "/etc/containers/systemd/registry.container",
    );
    assert!(
        unit.contains("Image=docker.io/library/registry:2"),
        "got:\n{unit}"
    );
    assert!(unit.contains("ContainerName=registry"), "got:\n{unit}");
    assert!(unit.contains("PublishPort=5000:5000/tcp"), "got:\n{unit}");
    assert!(
        unit.contains("Volume=golem-registry-data.volume:/var/lib/registry:Z"),
        "got:\n{unit}"
    );
    assert!(
        unit.contains("Volume=/srv/registry/uploads:/uploads:ro"),
        "got:\n{unit}"
    );
    assert!(
        unit.contains("Environment=REGISTRY_LOG_LEVEL=info"),
        "got:\n{unit}"
    );
    assert!(
        unit.contains("Network=golem-registry-net.network"),
        "got:\n{unit}"
    );
    assert!(unit.contains("Label=golem.role=registry"), "got:\n{unit}");
    assert!(unit.contains("Restart=always"), "got:\n{unit}");
    assert!(
        unit.contains("WantedBy=multi-user.target default.target"),
        "got:\n{unit}"
    );
}

#[test]
fn volume_unit_file_renders_the_dot_volume_quadlet() {
    let c = compiled();
    let vol = file_contents(
        scroll(&c, "registry"),
        "/etc/containers/systemd/golem-registry-data.volume",
    );
    assert!(vol.contains("[Volume]"), "got:\n{vol}");
    assert!(vol.contains("Driver=local"), "got:\n{vol}");
}

#[test]
fn only_a_value_systemd_would_split_is_quoted() {
    let c = compiled();
    let unit = file_contents(
        scroll(&c, "registry"),
        "/etc/containers/systemd/registry.container",
    );
    assert!(
        unit.contains("Environment=REGISTRY_NOTE=\"two words\""),
        "systemd splits an unquoted value on whitespace; got:\n{unit}"
    );
    assert!(
        unit.contains("Environment=REGISTRY_LOG_LEVEL=info\n"),
        "a value with nothing to quote is written verbatim; got:\n{unit}"
    );
}

#[test]
fn a_managed_network_writes_a_dot_network_quadlet_naming_the_podman_network() {
    let c = compiled();
    let net = file_contents(
        scroll(&c, "registry"),
        "/etc/containers/systemd/golem-registry-net.network",
    );
    assert!(net.contains("[Network]"), "got:\n{net}");
    assert!(
        net.contains("NetworkName=golem-registry-net"),
        "got:\n{net}"
    );
    assert!(net.contains("Driver=bridge"), "got:\n{net}");
    assert!(
        keys(scroll(&c, "registry")).contains(&"apt:aardvark-dns".to_string()),
        "a managed network resolves container names through aardvark-dns"
    );
}

#[test]
fn an_existing_network_is_referenced_by_bare_name_and_writes_no_unit() {
    let c = compiled();
    let s = scroll(&c, "web");
    let unit = file_contents(s, "/etc/containers/systemd/web.container");
    assert!(unit.contains("Network=frontend"), "got:\n{unit}");
    assert!(
        !unit.contains("Network=frontend.network"),
        "an existing network is not a quadlet reference; got:\n{unit}"
    );
    assert!(
        !keys(s).iter().any(|k| k.ends_with("frontend.network")),
        "golem does not write a unit for a network it does not own; got: {:?}",
        keys(s)
    );
    assert!(
        !keys(s).contains(&"apt:aardvark-dns".to_string()),
        "the owner of the network owns its resolver; got: {:?}",
        keys(s)
    );
}

#[test]
fn each_published_port_gets_its_own_firewall_rule() {
    let c = compiled();
    let s = scroll(&c, "registry");
    let nft = file_contents(s, "/etc/nftables.d/registry-5000.nft");
    assert!(
        nft.contains("5000"),
        "opens the published port; got:\n{nft}"
    );
    assert!(
        nft.contains("10.0.0.0/8"),
        "internal exposure allows the internal network; got:\n{nft}"
    );
    assert!(nft.contains("accept"), "got:\n{nft}");
}

#[test]
fn public_exposure_opens_each_port_to_the_world_with_its_own_drop_in() {
    let c = compiled();
    let s = scroll(&c, "web");
    let k = keys(s);
    assert!(
        k.contains(&"file:/etc/nftables.d/public-web-443.nft".to_string()),
        "got: {k:?}"
    );
    assert!(
        k.contains(&"file:/etc/nftables.d/public-web-80.nft".to_string()),
        "got: {k:?}"
    );
    assert!(
        !k.iter().any(|key| key.starts_with("fileline:")),
        "public exposure composes drop-in files, never a shared line; got: {k:?}"
    );

    let nft = file_contents(s, "/etc/nftables.d/public-web-443.nft");
    assert_eq!(
        nft,
        "table inet golem {\n  chain input {\n    tcp dport 443 accept comment \"web\"\n  }\n}\n",
        "the drop-in is a complete, additive golem-table fragment"
    );
}

#[test]
fn a_workload_contributing_a_drop_in_also_carries_the_nftables_base() {
    let c = compiled();
    let k = keys(scroll(&c, "web"));
    for base in [
        "apt:nftables",
        "file:/etc/nftables.d",
        "file:/etc/golem-nftables.conf",
        "file:/etc/nftables.d/00-base.nft",
        "file:/etc/systemd/system/golem-nftables.service",
        "systemd:golem-nftables.service",
    ] {
        assert!(k.contains(&base.to_string()), "missing {base}; got: {k:?}");
    }
}

#[test]
fn no_drop_in_declares_a_table_golem_does_not_own() {
    let c = compiled();
    for host in ["registry", "web"] {
        for glyph in scroll(&c, host).glyphs() {
            if let Glyph::Filesystem {
                path,
                entry: Entry::File { contents, .. },
            } = glyph
            {
                if path.starts_with("/etc/nftables.d/") {
                    assert!(
                        contents.to_string().contains("table inet golem"),
                        "{path} declares a foreign table; got:\n{contents}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_digest_pinned_image_renders_with_an_at_sign() {
    let c = compiled();
    let unit = file_contents(scroll(&c, "web"), "/etc/containers/systemd/web.container");
    assert!(
        unit.contains("Image=docker.io/library/caddy@sha256:abc123"),
        "digest-pinned image uses @; got:\n{unit}"
    );
}

#[test]
fn image_ref_defaults_registry_and_tag_for_a_bare_name() {
    let c = compiled_image_refs();
    assert_eq!(image_line(&c, "bare"), "Image=docker.io/registry:latest");
}

#[test]
fn image_ref_keeps_a_name_path_and_explicit_tag_without_a_registry() {
    let c = compiled_image_refs();
    assert_eq!(
        image_line(&c, "library"),
        "Image=docker.io/library/registry:2"
    );
}

#[test]
fn image_ref_reads_a_dotted_registry_off_the_first_segment() {
    let c = compiled_image_refs();
    assert_eq!(image_line(&c, "ghcr"), "Image=ghcr.io/dull/golem:v1");
}

#[test]
fn image_ref_treats_a_host_port_registry_colon_as_a_port_not_a_tag() {
    let c = compiled_image_refs();
    assert_eq!(
        image_line(&c, "hostport"),
        "Image=10.0.2.2:5000/website:latest"
    );
}

#[test]
fn image_ref_parses_a_digest_after_the_at_sign_with_no_tag() {
    let c = compiled_image_refs();
    assert_eq!(
        image_line(&c, "digest"),
        "Image=docker.io/alpine@sha256:abc123"
    );
}

#[test]
fn an_unexposed_workload_emits_no_firewall_glyph() {
    let c = compiled();
    let s = scroll(&c, "worker");
    assert!(
        !keys(s).iter().any(|k| k.contains("nftables")),
        "unexposed workload has no firewall glyph; got: {:?}",
        keys(s)
    );
}

fn dulliac() -> emet::Compiled {
    compile_file(&fixtures_dir().join("DulliacEntry.emet")).expect("DulliacEntry.emet compiles")
}

fn group<'a>(c: &'a emet::Compiled, name: &str) -> &'a emet::ir::Scroll {
    fn find<'a>(s: &'a emet::ir::Scroll, name: &str) -> Option<&'a emet::ir::Scroll> {
        if s.name == name {
            return Some(s);
        }
        match &s.contents {
            emet::ir::Contents::Groups(children) => {
                children.iter().find_map(|child| find(child, name))
            }
            emet::ir::Contents::Glyphs(_) => None,
        }
    }
    c.scrolls
        .iter()
        .find_map(|s| find(s, name))
        .unwrap_or_else(|| panic!("no scroll named {name}"))
}

#[test]
fn a_dns_challenge_renders_its_provider_instead_of_the_tls_one() {
    let c = dulliac();
    let config = file_contents(group(&c, "traefik"), "/etc/traefik/traefik.yml");
    assert!(config.contains("dnsChallenge:"), "got:\n{config}");
    assert!(config.contains("provider: \"digitalocean\""), "got:\n{config}");
    assert!(
        config.contains("delayBeforeCheck: \"30\""),
        "got:\n{config}"
    );
    assert!(
        !config.contains("tlsChallenge"),
        "a DNS challenge replaces the TLS one; got:\n{config}"
    );
}

#[test]
fn an_ingress_reads_its_provider_credentials_from_a_file_it_does_not_carry() {
    let c = dulliac();
    let unit = file_contents(
        group(&c, "traefik"),
        "/etc/containers/systemd/traefik.container",
    );
    assert!(
        unit.contains("EnvironmentFile=/etc/golem/traefik.env"),
        "got:\n{unit}"
    );
    assert!(
        !unit.contains("DO_AUTH_TOKEN"),
        "the token reaches the container through the file, never the manifest; got:\n{unit}"
    );
}

#[test]
fn a_workload_renders_its_environment_files() {
    let c = dulliac();
    let unit = file_contents(
        group(&c, "firewalled"),
        "/etc/containers/systemd/site.container",
    );
    assert!(
        unit.contains("EnvironmentFile=/etc/golem/site.env"),
        "got:\n{unit}"
    );
}

#[test]
fn container_glyphs_alone_carry_no_firewall_rule() {
    let c = dulliac();
    let bare = keys(group(&c, "bare"));
    assert!(
        !bare.iter().any(|k| k.contains("nftables")),
        "a consumer owning its own firewall composes the container half alone; got: {bare:?}"
    );
    let firewalled = keys(group(&c, "firewalled"));
    assert!(
        firewalled.iter().any(|k| k.contains("nftables")),
        "workloadGlyphs still pairs the container with golem's rules; got: {firewalled:?}"
    );
}
