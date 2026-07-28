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
            } if p == path => Some(contents.as_str()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no file glyph at {path}"))
}

#[test]
fn workload_lowers_to_podman_apt_container_volume_service_firewall_and_directory() {
    let c = compiled();
    let s = scroll(&c, "registry");
    assert_eq!(
        keys(s),
        vec![
            "apt:podman".to_string(),
            "file:/srv/registry/uploads".to_string(),
            "file:/etc/containers/systemd/golem-registry-data.volume".to_string(),
            "file:/etc/containers/systemd/registry.container".to_string(),
            "systemd:registry.service".to_string(),
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
fn public_exposure_opens_each_port_to_the_world_plus_a_shared_chain_line() {
    let c = compiled();
    let s = scroll(&c, "web");
    let k = keys(s);
    assert!(
        k.contains(&"file:/etc/nftables.d/web-443.nft".to_string()),
        "got: {k:?}"
    );
    assert!(
        k.contains(&"file:/etc/nftables.d/web-80.nft".to_string()),
        "got: {k:?}"
    );
    assert!(
        k.iter()
            .any(|key| key.starts_with("fileline:/etc/nftables.d/ingress.nft")),
        "public exposure appends to the shared ingress chain; got: {k:?}"
    );
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
