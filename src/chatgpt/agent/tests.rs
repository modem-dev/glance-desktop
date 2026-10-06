use super::*;
use crate::document::{Mark, Tool};
fn mark() -> Value {
    json!({"tool":"rectangle","points":[[5,5],[30,25]],"color":[255,50,0,255],"width":3,"text":"","curve":null})
}
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,"name":name,"namespace":"glance","arguments":args.to_string(),"status":"completed"})
}
fn message(text: &str) -> Value {
    json!({"type":"message","id":"msg_test","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]})
}
fn doc() -> Document {
    Document::new(image::RgbaImage::from_pixel(
        80,
        60,
        image::Rgba([30, 40, 50, 255]),
    ))
}
#[test]
fn multi_turn_tools_replay_reasoning_and_apply_one_undoable_draft() {
    let mut original = doc();
    crate::document::actions::DocumentAction::AddAnnotation {
        mark: serde_json::from_value(mark()).unwrap(),
    }
    .apply(&mut original)
    .unwrap();
    let baseline = original.marks.clone();
    let mut round = 0;
    let result=run(original,7,"synthetic-model","Crop and highlight",&AtomicBool::new(false),|request| {
        assert_eq!(request["store"],false);assert_eq!(request["stream"],true);
        assert_eq!(request["tools"][0]["type"],"namespace");assert_eq!(request["tools"][0]["name"],"glance");
        for field in ["previous_response_id","temperature","max_output_tokens","max_tool_calls","conversation"] {assert!(request.get(field).is_none());}
        round+=1;
        Ok(match round {
            1 => vec![json!({"type":"reasoning","id":"rs_test","summary":[],"encrypted_content":"synthetic-encrypted-state"}),call("one","crop_image",json!({"x":0,"y":0,"width":60,"height":40,"expected_revision":7}))],
            2 => {
                let input=request["input"].as_array().unwrap();
                assert!(input.iter().any(|i|i["encrypted_content"]=="synthetic-encrypted-state"));
                let out=input.iter().find(|i|i["call_id"]=="one"&&i["type"]=="function_call_output").unwrap();
                let state:Value=serde_json::from_str(out["output"].as_str().unwrap()).unwrap();assert_eq!(state["revision"],8);assert_eq!(state["width"],60);
                vec![call("two","add_annotation",json!({"mark":mark(),"expected_revision":8})),call("three","read_image",json!({}))]
            },
            3 => {assert!(request["input"].as_array().unwrap().iter().any(|i|i["content"][0]["text"].as_str().is_some_and(|s|s.starts_with("Current framed"))));vec![message("Cropped and highlighted.")]},
            _ => panic!("Unexpected turn")
        })
    },|_,_|{}).unwrap();
    assert_eq!(round, 3);
    assert_eq!(result.steps, 3);
    assert_eq!(result.answer, "Cropped and highlighted.");
    assert!(result.changed);
    let mut document = result.document;
    assert_eq!(document.base.dimensions(), (60, 40));
    assert_eq!(document.marks.len(), 2);
    document.undo();
    assert_eq!(document.base.dimensions(), (80, 60));
    assert_eq!(document.marks, baseline);
    document.undo();
    assert!(document.marks.is_empty());
    document.redo();
    assert_eq!(document.marks, baseline);
    document.redo();
    assert_eq!(document.base.dimensions(), (60, 40));
    assert_eq!(document.marks.len(), 2);
}
#[test]
fn stale_ids_and_invalid_tools_do_not_mutate_draft_and_can_be_corrected() {
    let mut draft = Snapshot {
        document: doc(),
        revision: 4,
        phase: 0.5,
    };
    execute("add_annotation", &json!({"mark":mark()}), &mut draft).unwrap();
    assert_eq!(draft.revision, 5);
    let before = draft.document.marks.clone();
    for (name, args) in [
        ("delete_annotation", json!({"id":"4:0"})),
        (
            "crop_image",
            json!({"x":0,"y":0,"width":20,"height":20,"expected_revision":4}),
        ),
        ("resize_image", json!({"scale":50})),
        ("export_png", json!({"path":"/tmp/never-created.png"})),
        ("import_image", json!({"path":"/tmp/never-read.png"})),
        ("dispatch_action", json!({"action":{"type":"copy_remote"}})),
    ] {
        assert!(execute(name, &args, &mut draft).is_err());
        assert_eq!(draft.document.marks, before);
        assert_eq!(draft.revision, 5);
    }
    execute("delete_annotation", &json!({"id":"5:0"}), &mut draft).unwrap();
    assert!(draft.document.marks.is_empty());
    let mut round = 0;
    let result = run(
        doc(),
        0,
        "synthetic",
        "Highlight",
        &AtomicBool::new(false),
        |body| {
            round += 1;
            Ok(match round {
                1 => vec![call(
                    "one",
                    "crop_image",
                    json!({"x":0,"y":0,"width":-1,"height":20}),
                )],
                2 => {
                    assert!(
                        body["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|i| i["output"].as_str().is_some_and(|s| s.contains("error")))
                    );
                    vec![call("two", "add_annotation", json!({"mark":mark()}))]
                }
                _ => vec![message("Highlighted.")],
            })
        },
        |_, _| {},
    )
    .unwrap();
    assert_eq!(result.document.base.dimensions(), (80, 60));
    assert_eq!(result.document.marks.len(), 1);
}
#[test]
fn failures_cancellation_duplicate_ids_and_step_limit_never_produce_a_document() {
    let cancel = AtomicBool::new(false);
    let mut round = 0;
    let result = run(
        doc(),
        0,
        "synthetic",
        "Highlight",
        &cancel,
        |_| {
            round += 1;
            if round == 1 {
                Ok(vec![call("one", "add_annotation", json!({"mark":mark()}))])
            } else {
                Err("connection interrupted".into())
            }
        },
        |_, _| {},
    );
    assert!(result.is_err());
    let result = run(
        doc(),
        0,
        "synthetic",
        "Highlight",
        &cancel,
        |_| {
            cancel.store(true, Ordering::Relaxed);
            Ok(vec![call("one", "add_annotation", json!({"mark":mark()}))])
        },
        |_, _| {},
    );
    assert!(result.is_err());
    let result = run(
        doc(),
        0,
        "synthetic",
        "Highlight",
        &AtomicBool::new(false),
        |_| {
            Ok(vec![
                call("same", "add_annotation", json!({"mark":mark()})),
                call("same", "add_annotation", json!({"mark":mark()})),
            ])
        },
        |_, _| {},
    );
    assert!(result.is_err());
    let mut round = 0;
    let result = run(
        doc(),
        0,
        "synthetic",
        "Keep going",
        &AtomicBool::new(false),
        |_| {
            round += 1;
            Ok(vec![call(&round.to_string(), "get_document", json!({}))])
        },
        |_, _| {},
    );
    assert!(result.is_err());
    assert_eq!(round, MAX_ROUNDS);
}
#[test]
fn answer_only_preserves_redo_and_refusal_or_empty_output_is_failure() {
    let mut d = doc();
    d.commit(serde_json::from_value::<Mark>(mark()).unwrap());
    d.undo();
    assert!(d.can_redo());
    let result = run(
        d,
        0,
        "synthetic",
        "Explain this",
        &AtomicBool::new(false),
        |_| Ok(vec![message("A sample image.")]),
        |_, _| {},
    )
    .unwrap();
    assert!(!result.changed);
    assert!(result.document.can_redo());
    for output in [
        vec![],
        vec![json!({"type":"message","content":[{"type":"refusal","refusal":"No"}]})],
        vec![
            call("one", "add_annotation", json!({"mark":{"tool":Tool::Crop}})),
            json!({"type":"shell_call"}),
        ],
    ] {
        assert!(
            run(
                doc(),
                0,
                "synthetic",
                "Explain",
                &AtomicBool::new(false),
                |_| Ok(output.clone()),
                |_, _| {}
            )
            .is_err()
        );
    }
}
#[test]
fn completed_stream_is_required_and_partial_tool_calls_are_discarded() {
    let output = vec![call("one", "get_document", json!({}))];
    let partial = format!(
        "data: {}\n\n",
        json!({"type":"response.output_item.done","output_index":0,"item":output[0]})
    );
    assert!(consume_stream(Cursor::new(partial.clone()), &AtomicBool::new(false)).is_err());
    let complete = format!(
        "{partial}data: {}\n\n",
        json!({"type":"response.completed","response":{"status":"completed","output":output}})
    );
    assert_eq!(
        consume_stream(Cursor::new(complete), &AtomicBool::new(false)).unwrap(),
        output
    );
    for event in [
        json!({"type":"response.incomplete"}),
        json!({"type":"response.failed"}),
        json!({"type":"response.completed","response":{"status":"incomplete","output":output}}),
    ] {
        assert!(
            consume_stream(
                Cursor::new(format!("data: {event}\n\n")),
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    assert!(consume_stream(Cursor::new("data: nope\n\n"), &AtomicBool::new(false)).is_err());
}
#[test]
fn tool_surface_matches_mcp_and_bounds_inputs() {
    for tool in tools()[0]["tools"].as_array().unwrap() {
        let published = crate::mcp::tools()
            .into_iter()
            .find(|p| p["name"] == tool["name"])
            .unwrap();
        assert_eq!(tool["parameters"], published["inputSchema"]);
    }
    assert!(validate_prompt("").is_err());
    assert!(validate_prompt(&"a".repeat(MAX_PROMPT + 1)).is_err());
    assert!(validate_prompt("Highlight error").is_ok());
    let too_big = Document::new(image::RgbaImage::new(4001, 4000));
    assert!(dimensions(&too_big).is_err());
    let mut draft = Snapshot {
        document: doc(),
        revision: 0,
        phase: 0.5,
    };
    assert!(execute("read_image", &json!({"max_edge":u32::MAX}), &mut draft).is_err());
}

#[test]
fn streamed_items_survive_empty_terminal_output_across_editing_rounds() {
    let cancel = AtomicBool::new(false);
    let reasoning = json!({"type":"reasoning","id":"rs_test","summary":[],"encrypted_content":"synthetic_encrypted_content"});
    let mut round = 0;
    let result = run(
        doc(),
        0,
        "synthetic",
        "Add a box",
        &cancel,
        |body| {
            round += 1;
            let output = if round == 1 {
                vec![
                    reasoning.clone(),
                    call("one", "add_annotation", json!({"mark":mark()})),
                ]
            } else {
                assert!(body["input"].as_array().unwrap().contains(&reasoning));
                assert!(
                    body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["type"] == "function_call_output" && i["call_id"] == "one")
                );
                vec![message("Added the box.")]
            };
            let mut stream = String::new();
            for (index, item) in output.into_iter().enumerate() {
                stream.push_str(&format!(
                    "data: {}\n\n",
                    json!({"type":"response.output_item.done","output_index":index,"item":item})
                ));
            }
            stream.push_str(&format!(
                "data: {}\n\n",
                json!({"type":"response.completed","response":{"status":"completed","output":[]}})
            ));
            consume_stream(Cursor::new(stream), &cancel)
        },
        |_, _| {},
    )
    .unwrap();
    assert_eq!(round, 2);
    assert_eq!(result.steps, 1);
    assert_eq!(result.answer, "Added the box.");
    assert_eq!(result.document.marks.len(), 1);
    let mut d = result.document;
    d.undo();
    assert!(d.marks.is_empty());
    d.redo();
    assert_eq!(d.marks.len(), 1);
}
#[test]
fn finalized_items_are_ordered_and_never_execute_after_failed_or_partial_streams() {
    let first = call("one", "get_document", json!({}));
    let last = message("Done.");
    let event = |index: usize, item: &Value| {
        format!(
            "data: {}\n\n",
            json!({"type":"response.output_item.done","output_index":index,"item":item})
        )
    };
    let items = format!("{}{}", event(1, &last), event(0, &first));
    let terminal = |response: Value| {
        format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":response})
        )
    };
    assert_eq!(
        consume_stream(
            Cursor::new(format!(
                "{items}{}",
                terminal(json!({"status":"completed"}))
            )),
            &AtomicBool::new(false)
        )
        .unwrap(),
        vec![first.clone(), last.clone()]
    );
    for tail in [
        "".into(),
        "data: {\"type\":\"response.failed\"}\n\n".into(),
        "data: {\"type\":\"response.incomplete\"}\n\n".into(),
        terminal(json!({"status":"incomplete","output":[]})),
    ] {
        assert!(
            consume_stream(
                Cursor::new(format!("{items}{tail}")),
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    for invalid in [
        format!(
            "{}{}",
            event(1, &last),
            terminal(json!({"status":"completed","output":[]}))
        ),
        format!(
            "{}{}{}",
            event(0, &first),
            event(0, &first),
            terminal(json!({"status":"completed","output":[]}))
        ),
        format!(
            "{}{}",
            event(0, &first),
            terminal(json!({"status":"completed","output":[last]}))
        ),
        format!(
            "{}{}",
            event(128, &first),
            terminal(json!({"status":"completed","output":[]}))
        ),
        format!(
            "{}{}",
            event(0, &json!({"type":"function_call","status":"in_progress"})),
            terminal(json!({"status":"completed","output":[]}))
        ),
    ] {
        assert!(consume_stream(Cursor::new(invalid), &AtomicBool::new(false)).is_err());
    }
    assert!(
        consume_stream(
            Cursor::new(format!(
                "{items}{}",
                terminal(json!({"status":"completed","output":[]}))
            )),
            &AtomicBool::new(true)
        )
        .is_err()
    );
}
