use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn frame(payload: &serde_json::Value) -> String {
    let body = serde_json::to_string(payload).unwrap();
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

fn read_message(reader: &mut impl BufRead) -> serde_json::Value {
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let trimmed = header.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn read_until_publish(reader: &mut impl BufRead) -> serde_json::Value {
    loop {
        let message = read_message(reader);
        if message.get("method").and_then(|m| m.as_str())
            == Some("textDocument/publishDiagnostics")
        {
            return message;
        }
    }
}

#[test]
fn initialize_and_publish_diagnostics_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_emet-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": {} }
    });
    stdin.write_all(frame(&initialize).as_bytes()).unwrap();
    stdin.flush().unwrap();

    let init_response = read_message(&mut stdout);
    assert_eq!(init_response["id"], 1);
    assert_eq!(
        init_response["result"]["capabilities"]["textDocumentSync"],
        1
    );

    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    });
    stdin.write_all(frame(&initialized).as_bytes()).unwrap();

    let open_broken = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///broken.emet",
                "languageId": "emet",
                "version": 1,
                "text": "main : List Scroll\nmain =\n  undefinedThing\n"
            }
        }
    });
    stdin.write_all(frame(&open_broken).as_bytes()).unwrap();
    stdin.flush().unwrap();

    let broken = read_until_publish(&mut stdout);
    assert_eq!(broken["params"]["uri"], "file:///broken.emet");
    let diagnostics = broken["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 2);
    assert!(diagnostics[0]["message"]
        .as_str()
        .unwrap()
        .contains("undefinedThing"));

    let open_valid = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///valid.emet",
                "languageId": "emet",
                "version": 1,
                "text": "main : List Scroll\nmain =\n  []\n"
            }
        }
    });
    stdin.write_all(frame(&open_valid).as_bytes()).unwrap();
    stdin.flush().unwrap();

    let valid = read_until_publish(&mut stdout);
    assert_eq!(valid["params"]["uri"], "file:///valid.emet");
    assert!(valid["params"]["diagnostics"].as_array().unwrap().is_empty());

    let shutdown = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "shutdown",
        "params": serde_json::Value::Null
    });
    stdin.write_all(frame(&shutdown).as_bytes()).unwrap();
    stdin.flush().unwrap();
    let shutdown_response = read_message(&mut stdout);
    assert_eq!(shutdown_response["id"], 2);

    let exit = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": serde_json::Value::Null
    });
    stdin.write_all(frame(&exit).as_bytes()).unwrap();
    stdin.flush().unwrap();

    let status = child.wait().unwrap();
    assert!(status.success());
}
