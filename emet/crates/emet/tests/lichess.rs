use emet::compile_file;

fn lichess_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/lichess")
}

fn fleet() -> emet::Compiled {
    compile_file(&lichess_dir().join("fleet.emet")).expect("fleet.emet compiles")
}

fn scroll<'a>(c: &'a emet::Compiled, name: &str) -> &'a emet::ir::Scroll {
    c.scrolls
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no scroll named {name}"))
}

fn keys(s: &emet::ir::Scroll) -> Vec<String> {
    s.glyphs.iter().map(|g| g.key()).collect()
}

#[test]
fn fleet_produces_one_scroll_per_host() {
    let c = fleet();
    let mut names: Vec<String> = c.scrolls.iter().map(|s| s.name.clone()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "kaiju".to_string(),
            "manta".to_string(),
            "orbit".to_string(),
            "scaly".to_string(),
            "talos".to_string(),
            "zulip".to_string(),
        ]
    );
}

#[test]
fn scaly_is_a_single_networkless_workload() {
    let c = fleet();
    assert_eq!(
        keys(scroll(&c, "scaly")),
        vec![
            "apt:podman".to_string(),
            "file:/etc/containers/systemd/fishnet.container".to_string(),
            "systemd:fishnet.service".to_string(),
        ]
    );
}

#[test]
fn a_service_lowers_to_workload_plus_firewall_opening() {
    let c = fleet();
    let k = keys(scroll(&c, "kaiju"));
    assert!(k.contains(&"apt:podman".to_string()));
    assert!(k.contains(&"file:/etc/containers/systemd/mongod-lichess-primary.container".to_string()));
    assert!(k.contains(&"systemd:mongod-lichess-primary.service".to_string()));
    assert!(k.contains(&"file:/etc/nftables.d/mongod-lichess-primary.nft".to_string()));
}

#[test]
fn kaiju_has_no_ingress() {
    let c = fleet();
    let k = keys(scroll(&c, "kaiju"));
    assert!(!k.iter().any(|key| key.contains("nginx")));
    assert!(!k.iter().any(|key| key.contains("443")));
}

#[test]
fn an_ingress_installs_nginx_writes_a_site_and_opens_443() {
    let c = fleet();
    let k = keys(scroll(&c, "manta"));
    assert!(k.contains(&"apt:nginx".to_string()));
    assert!(k.contains(&"systemd:nginx.service".to_string()));
    assert!(k.contains(&"file:/etc/nginx/sites-enabled/lichess.org.conf".to_string()));
    assert!(k.iter().any(|key| key.contains("nftables") && key.contains("ingress")));
}

#[test]
fn service_firewall_allows_internal_sources_to_the_service_port() {
    let c = fleet();
    let s = scroll(&c, "kaiju");
    let nft = s
        .glyphs
        .iter()
        .find_map(|g| match g {
            emet::ir::Glyph::File { path, contents, .. }
                if path == "/etc/nftables.d/mongod-lichess-primary.nft" =>
            {
                Some(contents.clone())
            }
            _ => None,
        })
        .expect("mongo nftables fragment present");
    assert!(nft.contains("27017"), "opens the service port");
    assert!(nft.contains("10.0.0.0/8"), "allows the internal network");
    assert!(nft.contains("accept"));
}

#[test]
fn ingress_site_block_proxies_to_the_named_service_over_ssl() {
    let c = fleet();
    let s = scroll(&c, "manta");
    let site = s
        .glyphs
        .iter()
        .find_map(|g| match g {
            emet::ir::Glyph::File { path, contents, .. }
                if path == "/etc/nginx/sites-enabled/lichess.org.conf" =>
            {
                Some(contents.clone())
            }
            _ => None,
        })
        .expect("nginx site block present");
    assert!(site.contains("server_name lichess.org;"));
    assert!(site.contains("listen 443 ssl"));
    assert!(site.contains("ssl_certificate"));
    assert!(site.contains("proxy_pass"));
    assert!(site.contains("lila"));
}

#[test]
fn manta_workloads_have_no_firewall_opening() {
    let c = fleet();
    let k = keys(scroll(&c, "manta"));
    assert!(k.contains(&"file:/etc/containers/systemd/leroyjenkins-lila.container".to_string()));
    assert!(!k.contains(&"file:/etc/nftables.d/leroyjenkins-lila.nft".to_string()));
}
