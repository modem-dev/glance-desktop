//! Check the published contract against Serde's actual action inventory.
use super::*;
use crate::editor::actions::Action;
use serde::{Deserialize, de};
use std::collections::BTreeSet;

#[derive(Debug)]
struct InventoryError(Vec<&'static str>);
impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "action inventory: {:?}", self.0)
    }
}
impl std::error::Error for InventoryError {}
impl de::Error for InventoryError {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        panic!("Unexpected inventory deserialization error: {message}")
    }
    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self(expected.to_vec())
    }
}

fn action_schema() -> Value {
    tools()
        .into_iter()
        .find(|t| t["name"] == "dispatch_action")
        .unwrap()["inputSchema"]["properties"]["action"]
        .clone()
}

#[test]
fn every_serializable_action_is_exposed_or_explicitly_excluded() {
    // Serde supplies this list, including future variants and wire renames.
    // No source parsing or second handwritten list of application actions.
    let deserializer = de::value::MapDeserializer::<_, InventoryError>::new(
        [("type", "__mcp_inventory__")].into_iter(),
    );
    let Err(InventoryError(variants)) = Action::deserialize(deserializer) else {
        panic!("Inventory sentinel unexpectedly became a valid action")
    };
    let schema = action_schema();
    let exposed: BTreeSet<_> = schema["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    // These have semantic MCP alternatives. Prepared documents are serde(skip)
    // and must never appear in either the wire inventory or published schema.
    let excluded = [
        (
            "begin_tool_color_sampling",
            "sample_tool_color with source pixel coordinates instead of pointer gestures",
        ),
        (
            "begin_backdrop_color_sampling",
            "sample_backdrop_color with source pixel coordinates instead of pointer gestures",
        ),
        (
            "edit",
            "revision-scoped document tools instead of raw indices",
        ),
        (
            "begin_backdrop_adjustment",
            "set_backdrop_control instead of pointer gestures",
        ),
        (
            "begin_animation_adjustment",
            "set_animation_control instead of pointer gestures",
        ),
    ];
    for (name, reason) in excluded {
        assert!(!exposed.contains(name), "{name}: {reason}");
    }
    let accounted: BTreeSet<_> = exposed
        .into_iter()
        .chain(excluded.map(|(name, _)| name))
        .collect();
    assert!(!accounted.contains("apply_prepared_document"));
    assert_eq!(
        accounted,
        variants.into_iter().collect(),
        "Update the MCP schema or document an intentional exclusion for each action"
    );
}

#[test]
fn every_exposed_action_has_a_valid_round_trip_payload() {
    let fixtures = [
        json!({"type":"capture","area":true}),
        json!({"type":"copy_ocr","rectangle":[10,20,30,40],"engine":"chatgpt"}),
        json!({"type":"chatgpt_sign_in","account_id":"synthetic-account"}),
        json!({"type":"select_chatgpt_account","account_id":"synthetic-account"}),
        json!({"type":"set_ocr_engine","engine":"local"}),
        json!({"type":"set_chatgpt_model","model":"synthetic-model"}),
        json!({"type":"open_path","path":"/tmp/glance-synthetic.png"}),
        json!({"type":"select_tool","tool":"arrow"}),
        json!({"type":"select_region","rectangle":[10,20,30,40],"additive":true}),
        json!({"type":"select_annotations","ids":["0:0","0:2"]}),
        json!({"type":"set_color","color":[10,20,30,128]}),
        json!({"type":"sample_tool_color","position":[10,20]}),
        json!({"type":"set_stroke_width","width":3}),
        json!({"type":"set_appearance","style":crate::style::Style::default()}),
        json!({"type":"set_magnifier_zoom","zoom":3}),
        json!({"type":"set_counter_number","number":7}),
        json!({"type":"set_crop_ratio","ratio":null}),
        json!({"type":"nudge_selection","delta":[1,2],"remember":true}),
        json!({"type":"zoom","factor":2}),
        json!({"type":"zoom_at","factor":2,"anchor":[10,20]}),
        json!({"type":"pan_by","delta":[10,20]}),
        json!({"type":"close_panel","panel":"animation"}),
        json!({"type":"set_resize_scale","scale":2}),
        json!({"type":"resize","scale":2,"smart":true}),
        json!({"type":"set_backdrop","backdrop":crate::backdrop::Backdrop::default()}),
        json!({"type":"set_backdrop_format","format":"shorts"}),
        json!({"type":"set_backdrop_fill","gradient":true}),
        json!({"type":"select_motion","motion":"aurora"}),
        json!({"type":"randomize_motion","seed":12345}),
        json!({"type":"set_backdrop_preset","preset":0}),
        json!({"type":"set_backdrop_color","stop":1,"rgb":[17,34,255]}),
        json!({"type":"sample_backdrop_color","stop":0,"position":[10,20]}),
        json!({"type":"pick_backdrop_screen_color","stop":0}),
        json!({"type":"set_backdrop_control","control":"inside_padding","value":20}),
        json!({"type":"select_entrance","effect":"diagonal"}),
        json!({"type":"set_image_animation","animation":crate::animation::ImageAnimation::default()}),
        json!({"type":"set_animation_control","control":"duration","value":800}),
        json!({"type":"seek_animation","seconds":1.5}),
        json!({"type":"export_animation","format":"gif"}),
    ];
    let schema = action_schema();
    for name in schema["properties"]["type"]["enum"].as_array().unwrap() {
        let payload = fixtures
            .iter()
            .find(|f| f["type"] == *name)
            .cloned()
            .unwrap_or_else(|| json!({"type":name}));
        let action = Action::from_json(payload.clone())
            .unwrap_or_else(|e| panic!("Add a complete fixture for {name}: {e}"));
        validate_tool("dispatch_action", &json!({"action":payload})).unwrap();
        // Serializing fills defaulted nested fields and exposes any schema
        // field omissions even when a short fixture happens to deserialize.
        let canonical = serde_json::to_value(action).unwrap();
        validate_tool("dispatch_action", &json!({"action":canonical})).unwrap();
        Action::from_json(canonical).unwrap();
    }
    for payload in [
        json!({"type":"copy_ocr"}),
        json!({"type":"copy_ocr","engine":null}),
        json!({"type":"copy_ocr","engine":"local"}),
        json!({"type":"copy_ocr","engine":"chatgpt"}),
        json!({"type":"chatgpt_sign_in"}),
        json!({"type":"chatgpt_sign_in","account_id":null}),
        json!({"type":"set_ocr_engine","engine":"chatgpt"}),
        json!({"type":"copy_ocr","rectangle":null}),
        json!({"type":"copy_ocr","rectangle":[0,0,100,80]}),
        json!({"type":"select_annotations","ids":[]}),
        json!({"type":"select_annotations","ids":["4:0","4:2"]}),
        json!({"type":"select_region","rectangle":[0,0,10,20]}),
        json!({"type":"select_region","rectangle":[0,0,10,20],"additive":false}),
    ] {
        validate_tool("dispatch_action", &json!({"action":payload})).unwrap();
        let action = Action::from_json(payload).unwrap();
        validate_tool(
            "dispatch_action",
            &json!({"action":serde_json::to_value(action).unwrap()}),
        )
        .unwrap();
    }
    for stop in 0..=1 {
        for rgb in [[0, 0, 0], [255, 255, 255], [0, 128, 255]] {
            let payload = json!({"type":"set_backdrop_color","stop":stop,"rgb":rgb});
            validate_tool("dispatch_action", &json!({"action":payload})).unwrap();
            let action = Action::from_json(payload).unwrap();
            Action::from_json(serde_json::to_value(action).unwrap()).unwrap();
        }
    }
    for seed in [json!(null), json!(0), json!(12345), json!(u32::MAX)] {
        let payload = json!({"type":"randomize_motion","seed":seed});
        validate_tool("dispatch_action", &json!({"action":payload})).unwrap();
        let action = Action::from_json(payload).unwrap();
        Action::from_json(serde_json::to_value(action).unwrap()).unwrap();
    }
    for seed in [0, 12345, u32::MAX] {
        let backdrop = crate::backdrop::Backdrop {
            seed,
            ..Default::default()
        };
        validate_tool("set_backdrop", &json!({"backdrop":backdrop})).unwrap();
    }
    for motion in ["nebula", "stars"] {
        let payload = json!({"type":"select_motion","motion":motion});
        validate_tool("dispatch_action", &json!({"action":payload})).unwrap();
        let action = Action::from_json(payload).unwrap();
        assert_eq!(serde_json::to_value(action).unwrap()["motion"], "nebula");
        let backdrop = json!({"motion":motion,"seed":42});
        validate_tool("set_backdrop", &json!({"backdrop":backdrop})).unwrap();
        let backdrop: crate::backdrop::Backdrop = serde_json::from_value(backdrop).unwrap();
        let canonical = serde_json::to_value(backdrop).unwrap();
        assert_eq!(canonical["motion"], "nebula");
        validate_tool("set_backdrop", &json!({"backdrop":canonical})).unwrap();
    }
    for seed in [json!(-1), json!(4294967296_u64), json!(0.5)] {
        assert!(
            validate_tool(
                "dispatch_action",
                &json!({"action":{"type":"randomize_motion","seed":seed}})
            )
            .is_err()
        );
        assert!(validate_tool("set_backdrop", &json!({"backdrop":{"seed":seed}})).is_err());
    }
    for colors in [json!(null), json!([[1, 2, 3], [254, 128, 0]])] {
        let payload = json!({"backdrop":{"colors":colors}});
        validate_tool("set_backdrop", &payload).unwrap();
        let backdrop: crate::backdrop::Backdrop =
            serde_json::from_value(payload["backdrop"].clone()).unwrap();
        validate_tool("set_backdrop", &json!({"backdrop":backdrop})).unwrap();
    }
    for payload in [
        json!({"action":{"type":"set_backdrop_color","stop":2,"rgb":[1,2,3]}}),
        json!({"action":{"type":"set_backdrop_color","stop":0,"rgb":[256,2,3]}}),
        json!({"action":{"type":"set_backdrop_color","stop":0,"rgb":[1,2]}}),
        json!({"action":{"type":"sample_backdrop_color","stop":1,"position":[-1,0]}}),
    ] {
        assert!(validate_tool("dispatch_action", &payload).is_err());
    }
    for colors in [
        json!([[1, 2, 3]]),
        json!([[256, 2, 3], [1, 2, 3]]),
        json!([[1, 2, 3, 4], [1, 2, 3]]),
    ] {
        assert!(validate_tool("set_backdrop", &json!({"backdrop":{"colors":colors}})).is_err());
    }
    for format in crate::backdrop::Format::ALL {
        validate_tool(
            "dispatch_action",
            &json!({"action":{"type":"set_backdrop_format","format":format}}),
        )
        .unwrap();
    }
}
