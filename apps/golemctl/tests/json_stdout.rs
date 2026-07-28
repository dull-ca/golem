use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::thread;

fn serve_post_manifest_then_one_terminal_progress_poll(listener: TcpListener) {
    for _ in 0..2 {
        let (stream, _) = listener.accept().unwrap();
        respond_with_fixture(stream);
    }
}

fn respond_with_fixture(mut stream: TcpStream) {
    let request = read_request(&mut stream);
    let body = if request.starts_with("POST /manifest") {
        r#"{"reconcile_id":1}"#.to_string()
    } else {
        r#"{
            "reconcile_id": 1,
            "phase": "settled",
            "units": [],
            "events": [
                {"seq": 1, "at": "2026-07-26T00:00:00Z", "level": "info",
                 "kind": "lifecycle", "unit_path": ["scaly"], "glyph_key": "apt:podman",
                 "message": "install apt:podman"}
            ],
            "cursor": 1,
            "report": {"outcome": "settled", "revision": {"id": 1}, "units": []}
        }"#
        .to_string()
    };
    let status = if request.starts_with("POST /manifest") {
        "202 Accepted"
    } else {
        "200 OK"
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut buf = [0u8; 8192];
    let mut text = String::new();
    loop {
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..n]));
        if text.contains("\r\n\r\n") {
            break;
        }
    }
    text
}

fn run_golemctl_apply(addr: &str, json: bool) -> std::process::Output {
    let manifest_path = std::env::temp_dir().join(format!(
        "golemctl-json-stdout-test-{}.bin",
        std::process::id()
    ));
    std::fs::write(&manifest_path, b"\x00").unwrap();

    let mut argv = vec![
        "apply".to_string(),
        manifest_path.display().to_string(),
        addr.to_string(),
    ];
    if json {
        argv.push("--json".to_string());
    }

    let output = Command::new(env!("CARGO_BIN_EXE_golemctl"))
        .args(&argv)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn golemctl");

    std::fs::remove_file(&manifest_path).ok();
    output
}

#[test]
fn json_mode_stdout_is_exactly_the_report_and_parses_as_json() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let server =
        thread::spawn(move || serve_post_manifest_then_one_terminal_progress_poll(listener));

    let output = run_golemctl_apply(&addr, true);
    server.join().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout did not parse as JSON: {e}\nstdout: {stdout:?}"));
    assert_eq!(parsed["outcome"], "settled");

    assert!(stderr.contains("logs:"), "stderr: {stderr:?}");
    assert!(stderr.contains("apt:podman"), "stderr: {stderr:?}");
    assert!(!stdout.contains("logs:"), "stdout: {stdout:?}");
    assert!(!stdout.contains("apt:podman"), "stdout: {stdout:?}");
}

#[test]
fn plain_mode_stdout_carries_logs_and_event_lines() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let server =
        thread::spawn(move || serve_post_manifest_then_one_terminal_progress_poll(listener));

    let output = run_golemctl_apply(&addr, false);
    server.join().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("logs:"), "stdout: {stdout:?}");
    assert!(stdout.contains("apt:podman"), "stdout: {stdout:?}");
}
