use super::*;
use incurs::tool::{ToolCallOptions, ToolCallOutcome};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn every_editor_tool_keeps_its_exact_schema_and_annotations() {
    let catalog = editor_cli().try_tool_catalog().unwrap();
    let expected = mcp::tools();
    assert_eq!(catalog.definitions().len(), expected.len());
    for tool in expected {
        let actual = catalog.get(tool["name"].as_str().unwrap()).unwrap();
        assert_eq!(actual.input_schema, tool["inputSchema"]);
        assert_eq!(actual.description, tool["description"].as_str().unwrap());
        assert_eq!(
            actual.annotations.as_ref().unwrap().read_only_hint,
            tool["annotations"]["readOnlyHint"].as_bool()
        );
        let annotations = actual.annotations.as_ref().unwrap();
        assert_eq!(
            annotations.destructive_hint,
            tool["annotations"]["destructiveHint"].as_bool()
        );
        assert_eq!(
            annotations.idempotent_hint,
            tool["annotations"]["idempotentHint"].as_bool()
        );
        assert_eq!(
            annotations.open_world_hint,
            tool["annotations"]["openWorldHint"].as_bool()
        );
    }
}

#[test]
fn code_lifecycle_is_exposed_but_the_server_is_not_an_agent_tool() {
    let catalog = cli().try_tool_catalog().unwrap();
    let names: BTreeSet<_> = catalog.definitions().into_iter().map(|t| t.name).collect();
    for name in [
        "code_search",
        "code_execute",
        "code_execution",
        "code_decide",
        "code_cancel",
    ] {
        assert!(names.contains(name), "{name}: {names:?}");
    }
    assert!(!names.contains("code_serve"));
}

#[tokio::test]
async fn catalog_rejects_invalid_inputs_before_touching_the_editor() {
    let catalog = editor_cli().tool_catalog();
    for (name, args) in [
        ("dispatch_action", json!({"action":{"type":"__unknown__"}})),
        ("add_annotation", json!({"mark":{"tool":"rectangle"}})),
        ("read_image", json!({"max_edge":1})),
        ("export_png", json!({"phase":2})),
        ("crop_image", json!({"x":0,"y":0,"width":1})),
        ("get_document", json!({"unexpected":true})),
    ] {
        let args = args
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let outcome = catalog.call(name, args, ToolCallOptions::isolated()).await;
        let ToolCallOutcome::Error { message, .. } = outcome else {
            panic!("{name} accepted invalid input")
        };
        assert!(
            !message.contains("Native editor unavailable"),
            "{name}: {message}"
        );
    }
}

#[tokio::test]
async fn every_editor_method_forwards_a_complete_payload_and_result() {
    use serde_json::json;
    let mark = json!({"tool":"rectangle","points":[[1,2],[10,20]],"color":[255,0,0,255],"width":2,"text":""});
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
            json!({"path":"/synthetic/image.png","expected_revision":7}),
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
            json!({"enabled":true,"backdrop":{"padding":20},"expected_revision":7}),
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
    let catalog = editor_cli_with(Arc::new(|name, args| {
        Ok(json!({"name":name,"args":args,"sentinel":123}))
    }))
    .tool_catalog();
    let expected: BTreeSet<_> = fixtures.iter().map(|(name, _)| *name).collect();
    let actual: BTreeSet<_> = catalog
        .definitions()
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(actual, expected.iter().map(|s| s.to_string()).collect());
    for (name, payload) in fixtures {
        let args = payload
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let ToolCallOutcome::Ok { data, .. } =
            catalog.call(name, args, ToolCallOptions::isolated()).await
        else {
            panic!("{name} failed");
        };
        assert_eq!(
            data,
            json!({"name":name,"args":payload,"sentinel":123}),
            "{name}"
        );
    }
}

#[test]
fn json_cli_flags_keep_nested_values_and_omitted_options() {
    assert_eq!(
        arguments("dispatch_action", json!({"action":"{\"type\":\"fit\"}"})).unwrap(),
        json!({"action":{"type":"fit"}})
    );
    assert_eq!(
        arguments("set_backdrop", json!({"backdrop":"{\"padding\":20}"})).unwrap(),
        json!({"backdrop":{"padding":20}})
    );
    assert!(arguments("add_annotation", json!({"mark":"{bad}"})).is_err());
    assert_eq!(arguments("set_backdrop", json!({})).unwrap(), json!({}));
}
