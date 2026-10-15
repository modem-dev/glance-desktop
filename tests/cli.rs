//! Exercise the shipped executable without connecting to a user's editor or display.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn run(args: &[&str], input: &str) -> (i32, String, String) {
    let home = tempfile::tempdir().unwrap();
    let mut out = tempfile::tempfile().unwrap();
    let mut err = tempfile::tempfile().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_glance"))
        .args(args)
        .env("HOME", home.path())
        .env("XDG_CACHE_HOME", home.path())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::piped())
        .stdout(out.try_clone().unwrap())
        .stderr(err.try_clone().unwrap())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("terminal invocation timed out: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    use std::io::{Seek, SeekFrom};
    out.seek(SeekFrom::Start(0)).unwrap();
    err.seek(SeekFrom::Start(0)).unwrap();
    let mut stdout = String::new();
    let mut stderr = String::new();
    out.read_to_string(&mut stdout).unwrap();
    err.read_to_string(&mut stderr).unwrap();
    (status.code().unwrap(), stdout, stderr)
}

#[test]
fn executable_help_schema_and_errors_are_script_friendly() {
    for args in [
        &["--help"][..],
        &["--cli", "--help"],
        &["resize-image", "--help"],
    ] {
        let (status, out, err) = run(args, "");
        assert_eq!(status, 0, "{err}");
        assert!(out.contains("Usage:"));
        assert!(err.is_empty(), "{err}");
    }
    let (status, out, err) = run(&["dispatch-action", "--schema"], "");
    assert_eq!(status, 0, "{err}");
    let schema: Value = serde_json::from_str(&out).unwrap();
    assert!(
        schema["properties"]["action"]["properties"]["type"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("copy_ocr"))
    );
    for (args, message) in [
        (vec!["get-document"], "Native editor unavailable"),
        (vec!["resize-image", "--scale=0"], "Out of range"),
        (
            vec![
                "dispatch-action",
                "--action={\"type\":\"set_ask_glance_prompt\",\"prompt\":\"--mcp\"}",
            ],
            "Native editor unavailable",
        ),
        (vec!["code", "execute"], "Unknown command"),
    ] {
        let (status, out, err) = run(&args, "");
        assert_eq!(status, 1);
        assert!(out.is_empty(), "{out}");
        let error: Value = serde_json::from_str(&err).unwrap();
        assert!(
            error["error"].as_str().unwrap().contains(message),
            "{error}"
        );
    }
}

#[test]
fn native_mcp_entry_point_and_discovery_are_unchanged() {
    let input = format!(
        "{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
    );
    let (status, out, err) = run(&["--mcp"], &input);
    assert_eq!(status, 0, "{err}");
    let response: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(response["id"], 1);
    let tools = response["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "dispatch_action"));
    assert!(
        !tools
            .iter()
            .any(|tool| tool["name"].as_str().unwrap().starts_with("code"))
    );
}
