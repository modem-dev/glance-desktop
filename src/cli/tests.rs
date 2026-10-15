use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

#[test]
fn routing_preserves_desktop_and_native_mcp_without_scanning_payloads() {
    assert_eq!(mode(vec![]), Mode::Desktop(vec![]));
    for args in [
        argv(&["--automation"]),
        argv(&["--open", "--mcp"]),
        argv(&["--capture-area"]),
        argv(&["--capture-screen"]),
        argv(&["-psn_0_123"]),
    ] {
        assert_eq!(mode(args.clone()), Mode::Desktop(args));
    }
    assert_eq!(
        mode(argv(&["desktop", "--automation"])),
        Mode::Desktop(argv(&["--automation"]))
    );
    assert_eq!(mode(argv(&["--mcp"])), Mode::Mcp);
    for args in [
        argv(&["get-document"]),
        argv(&["--help"]),
        argv(&["--version"]),
        argv(&["not-a-command"]),
        argv(&[
            "dispatch-action",
            "--action",
            "{\"type\":\"set_ask_glance_prompt\",\"prompt\":\"--mcp\"}",
        ]),
    ] {
        assert_eq!(mode(args.clone()), Mode::Cli(args));
    }
    assert_eq!(
        mode(argv(&["--cli", "get-document"])),
        Mode::Cli(argv(&["get-document"]))
    );
}

#[test]
fn help_and_schema_never_connect_to_the_editor() {
    let disconnected =
        |_: &str, _: Value| -> Result<Value, String> { panic!("unexpected editor call") };
    let help = execute(&[], disconnected).unwrap();
    for tool in mcp::tools() {
        let name = tool["name"].as_str().unwrap().replace('_', "-");
        assert!(help.contains(&name));
        let args = vec![name.clone(), "--help".into()];
        assert!(
            execute(&args, disconnected)
                .unwrap()
                .contains(tool["description"].as_str().unwrap())
        );
        let schema: Value =
            serde_json::from_str(&execute(&[name, "--schema".into()], disconnected).unwrap())
                .unwrap();
        assert_eq!(schema, tool["inputSchema"]);
    }
    assert_eq!(
        execute(&argv(&["--version"]), disconnected).unwrap(),
        env!("CARGO_PKG_VERSION")
    );
    assert!(help.contains("Plain glance opens the editor"));
    assert!(!help.contains("code execute"));
}

#[test]
fn every_terminal_command_forwards_a_complete_payload_and_json_result() {
    let mark = json!({"tool":"rectangle","points":[[1,2],[10,20]],"color":[255,0,0,255],"width":2,"text":"--mcp"});
    let fixtures = [
        ("get_editor_state", json!({})),
        (
            "dispatch_action",
            json!({"action":{"type":"fit"},"expected_revision":7}),
        ),
        ("open_editor", json!({})),
        ("get_document", json!({})),
        (
            "import_image",
            json!({"path":"/synthetic/image with spaces.png","expected_revision":7}),
        ),
        ("add_annotation", json!({"mark":mark,"expected_revision":7})),
        (
            "update_annotation",
            json!({"id":"7:0","mark":mark,"expected_revision":7}),
        ),
        (
            "move_annotation",
            json!({"id":"7:0","dx":3,"dy":-4,"expected_revision":7}),
        ),
        (
            "delete_annotation",
            json!({"id":"7:0","expected_revision":7}),
        ),
        (
            "crop_image",
            json!({"x":1,"y":2,"width":10,"height":20,"expected_revision":7}),
        ),
        (
            "resize_image",
            json!({"scale":0.5,"smart":false,"expected_revision":7}),
        ),
        (
            "set_backdrop",
            json!({"enabled":true,"backdrop":{"padding":20,"colors":null},"expected_revision":7}),
        ),
        ("undo", json!({"expected_revision":7})),
        ("redo", json!({"expected_revision":7})),
        ("read_image", json!({"phase":0.5,"max_edge":128})),
        (
            "export_png",
            json!({"path":"/synthetic/export.png","phase":0.5}),
        ),
        ("export_mp4", json!({"path":"/synthetic/export.mp4"})),
        ("export_gif", json!({"path":"/synthetic/export.gif"})),
        (
            "read_video_frame",
            json!({"path":"/synthetic/input.mp4","seconds":1.5,"max_edge":128}),
        ),
    ];
    let actual: BTreeSet<_> = mcp::tools()
        .into_iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        actual,
        fixtures.iter().map(|(name, _)| name.to_string()).collect()
    );
    for (name, payload) in fixtures {
        for inline in [false, true] {
            let mut args = vec![name.replace('_', "-")];
            for (field, value) in payload.as_object().unwrap() {
                let flag = format!("--{}", field.replace('_', "-"));
                let text = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned);
                if inline {
                    args.push(format!("{flag}={text}"));
                } else {
                    args.extend([flag, text]);
                }
            }
            let result = execute(&args, |called, args| {
                assert_eq!(called, name);
                assert_eq!(args, payload);
                Ok(json!({"sentinel":123,"args":args}))
            })
            .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&result).unwrap(),
                json!({"sentinel":123,"args":payload})
            );
        }
    }
}

#[test]
fn parser_preserves_omissions_booleans_nested_actions_and_strings() {
    for (args, expected) in [
        (argv(&["set-backdrop"]), json!({})),
        (
            argv(&["import-image", "--clipboard"]),
            json!({"clipboard":true}),
        ),
        (
            argv(&["resize-image", "--scale=0.5", "--smart=false"]),
            json!({"scale":0.5,"smart":false}),
        ),
        (
            argv(&["resize-image", "--smart", "--scale", "0.5"]),
            json!({"scale":0.5,"smart":true}),
        ),
        (
            argv(&["import-image", "--path=--mcp"]),
            json!({"path":"--mcp"}),
        ),
        (
            argv(&["import-image", "--path=null"]),
            json!({"path":"null"}),
        ),
        (
            argv(&[
                "dispatch-action",
                "--action",
                "{\"type\":\"copy_ocr\",\"rectangle\":null,\"engine\":\"local\"}",
            ]),
            json!({"action":{"type":"copy_ocr","rectangle":null,"engine":"local"}}),
        ),
        (
            argv(&[
                "dispatch-action",
                "--action",
                "{\"type\":\"set_ask_glance_prompt\",\"prompt\":\"--mcp\"}",
            ]),
            json!({"action":{"type":"set_ask_glance_prompt","prompt":"--mcp"}}),
        ),
    ] {
        execute(&args, |_, actual| {
            assert_eq!(actual, expected);
            Ok(Value::Null)
        })
        .unwrap();
    }
}

#[test]
fn invalid_arguments_are_rejected_before_any_editor_call() {
    for args in [
        argv(&["code", "execute"]),
        argv(&["--mcp"]),
        argv(&["get-document", "extra"]),
        argv(&["get-document", "--unexpected", "true"]),
        argv(&["resize-image"]),
        argv(&["resize-image", "--scale"]),
        argv(&["resize-image", "--scale", "--smart"]),
        argv(&["resize-image", "--scale=NaN"]),
        argv(&["resize-image", "--scale=5"]),
        argv(&["resize-image", "--scale=0.5", "--scale=0.6"]),
        argv(&["resize-image", "--scale=0.5", "--smart=null"]),
        argv(&["read-image", "--max-edge=1"]),
        argv(&["read-image", "--max-edge=64.5"]),
        argv(&["dispatch-action", "--action={bad}"]),
        argv(&["dispatch-action", "--action={\"type\":\"not-real\"}"]),
        argv(&[
            "dispatch-action",
            "--action={\"type\":\"fit\",\"unexpected\":true}",
        ]),
        argv(&["get-document", "--help", "--schema"]),
    ] {
        assert!(
            execute(&args, |_, _| panic!(
                "invalid request reached editor: {args:?}"
            ))
            .is_err(),
            "{args:?}"
        );
    }
    assert_eq!(
        execute(&argv(&["get-document"]), |_, _| Err(
            "Native editor unavailable".into()
        ))
        .unwrap_err(),
        "Native editor unavailable"
    );
}
