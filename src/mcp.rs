//! Dependency-light MCP stdio transport; tool work runs off the GPUI thread.
use crate::{
    animation::Motion,
    automation::{self, Snapshot},
    document::{Document, Mark},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::{BufReader, Cursor, Read, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};

fn tool(name: &str, description: &str, properties: Value, required: &[&str], read: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
        "annotations":{"readOnlyHint":read,"destructiveHint":!read,"idempotentHint":read,"openWorldHint":false}})
}
pub fn tools() -> Vec<Value> {
    let number = json!({"type":"number"});
    let path =
        json!({"type":"string","description":"Absolute path on the computer running Glance."});
    let revision = json!({"type":"integer","minimum":0});
    let style = json!({"type":"object","properties":{
        "dash":{"type":"string","enum":["solid","dashed","dotted"]},
        "fill":{"type":"string","enum":["outline","filled"]},
        "radius":{"type":"number","minimum":0,"maximum":32768},
        "cleanup":{"type":"string","enum":["raw","smooth","adaptive"]},
        "start":{"type":"string","enum":["none","arrow","dot"]},
        "end":{"type":"string","enum":["none","arrow","dot"]},
        "dim":{"type":"number","minimum":0,"maximum":0.95}
    },"additionalProperties":false});
    let mark = json!({"type":"object","description":"Editable mark: tool, points [[x,y],...], color [r,g,b,a], width, text, curve (optional [x,y]), style (optional dash/fill/radius/cleanup/start/end/dim). Lines allow 2..32 points; a two-point line can have a curve. All coordinates source image pixels. Tools: arrow, pen, rectangle, highlight, pixelate, text, counter, spotlight, magnifier. Spotlight uses opposite corners. Magnifier points are [source center,lens center], width × 12 is lens radius, text is zoom 1.5..4 (default 2). Text font size = width × 7.","properties":{"style":style,"tool":{"type":"string","enum":["arrow","pen","rectangle","highlight","pixelate","text","counter","spotlight","magnifier"]},"points":{"type":"array","minItems":1,"maxItems":2000,"items":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2}},"color":{"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":4,"maxItems":4},"width":{"type":"number","exclusiveMinimum":0,"maximum":32768},"text":{"type":"string","maxLength":2000},"curve":{"type":["array","null"],"items":{"type":"number"},"minItems":2,"maxItems":2}},"required":["tool","points","color","width","text"],"additionalProperties":false});
    let action_type = json!({"type":"string","enum":["show","capture","open_image","open_path","save_image","copy_image","copy_remote","paste_image","copy_ocr","toggle_ask_glance","set_ask_glance_prompt","ask_glance","cancel_ask_glance","copy_ask_glance_answer","toggle_chatgpt_account_menu","toggle_chatgpt_picker","chatgpt_sign_in","cancel_chatgpt_sign_in","select_chatgpt_account","sign_out_chatgpt","set_ocr_engine","set_chatgpt_model","manage_chatgpt_usage","dismiss_chatgpt_welcome","copy","cut","paste","undo","redo","delete","duplicate_selection","select_all","select_annotations","select_region","select_tool","set_color","sample_tool_color","pick_tool_screen_color","set_stroke_width","set_appearance","set_magnifier_zoom","set_counter_number","set_crop_ratio","add_line_point","straighten_line","cycle_stroke_width","cycle_magnifier_zoom","nudge_selection","fit","actual_size","zoom","zoom_at","pan_by","toggle_backdrop","toggle_enhance","close_panel","set_resize_scale","toggle_smart_resize","apply_resize","resize","rotate","set_backdrop","set_backdrop_format","toggle_backdrop_enabled","set_backdrop_fill","select_motion","randomize_motion","set_backdrop_preset","set_backdrop_color","sample_backdrop_color","pick_backdrop_screen_color","set_backdrop_control","toggle_animation_panel","select_entrance","set_image_animation","set_animation_control","seek_animation","replay_animation","toggle_playback","export_animation","cancel_export","reveal_export","commit_text","cancel","help","quit"]});
    let mut action_properties = json!({
        "type": action_type,
        "area":{"type":"boolean"}, "path":path, "tool":{"type":"string"},
        "color":{"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":4,"maxItems":4},
        "style":style,"zoom":number,"number":{"type":"integer","minimum":1,"maximum":999},"ratio":{"type":["number","null"],"minimum":0.1,"maximum":10},
        "width":number,"factor":number,"scale":number,"smart":{"type":"boolean"},
        "delta":{"type":"array","items":number,"minItems":2,"maxItems":2},
        "anchor":{"type":"array","items":number,"minItems":2,"maxItems":2},
        "ids":{"type":"array","items":{"type":"string"},"maxItems":500,"description":"select_annotations: exact selection of revision-scoped object IDs from get_document; [] clears selection. Finish inline text first."},
        "rectangle":{"type":["array","null"],"items":number,"minItems":4,"maxItems":4,"description":"select_region: [x,y,width,height] in source image pixels; intersects annotation bounds. copy_ocr: optional integer source-pixel rectangle with positive dimensions inside the image; omitted/null recognizes the full source screenshot."},
        "additive":{"type":"boolean","description":"select_region: retain the previous selection (default false)."},
        "remember":{"type":"boolean"}, "panel":{"type":"string","enum":["backdrop","enhance","animation"]},
        "backdrop":{"type":["object","null"]},"gradient":{"type":"boolean"},"motion":{"type":"string","enum":["still","flow","nebula","stars","aurora","contours","paint","prism","liquid","lava"],"description":"nebula is the star and gas-cloud effect; stars is its legacy alias. Read-back uses nebula."},
        "seed":{"type":["integer","null"],"minimum":0,"maximum":4294967295_u64,"description":"randomize_motion: omitted/null picks a fresh seed; an integer reproduces that looping variation (0 restores the original)."},"preset":{"type":"integer","minimum":0},"control":{"type":"string"},"value":{"type":"integer","minimum":0},
        "format":{"type":"string","enum":["mp4","gif","auto","square","classic","photo","widescreen","portrait","vertical","youtube","shorts","pinterest"]},"effect":{"type":"string","enum":["none","diagonal","pop","tilt"]},"animation":{"type":"object","description":"Image entrance settings; omitted fields use defaults. The entrance, delay and optional exit must fit within the clip.","properties":{
            "effect":{"type":"string","enum":["none","diagonal","pop","tilt"]},
            "duration_ms":{"type":"integer","minimum":200,"maximum":2000},
            "delay_ms":{"type":"integer","minimum":0,"maximum":1000},
            "seconds":{"type":"integer","minimum":2,"maximum":15},
            "exit":{"type":"boolean"}
        },"additionalProperties":false},"seconds":number
    });
    action_properties["prompt"] = json!({"type":"string","maxLength":8000,"description":"ask_glance: nonempty request up to 8,000 UTF-8 bytes. Sends annotated image/prompt to OpenAI using the selected ChatGPT plan; runs tools on a private draft, then applies one undoable edit. set_ask_glance_prompt sets the prompt bar draft (empty allowed) without uploading."});
    action_properties["picker"] = json!({"type":"string","enum":["account","model"],"description":"toggle_chatgpt_picker opens/closes the account or model dropdown; with Ask Glance open and account settings closed, the model list opens above the prompt without opening settings. Selection uses select_chatgpt_account or set_chatgpt_model."});
    action_properties["engine"] = json!({"type":["string","null"],"enum":["local","chatgpt",null],"description":"copy_ocr: omitted/null uses the saved OCR engine; local stays offline; chatgpt sends the source image to OpenAI using the selected ChatGPT plan. set_ocr_engine: required local or chatgpt."});
    action_properties["account_id"] = json!({"type":["string","null"],"maxLength":128,"description":"Opaque account ID from get_editor_state.chatgpt.account.accounts. chatgpt_sign_in: omitted/null adds an account, a saved ID reconnects it. select_chatgpt_account requires a saved ID."});
    action_properties["model"] = json!({"type":"string","maxLength":256,"description":"set_chatgpt_model: exact slug from get_editor_state.chatgpt.account.models; models are account-specific."});
    let color_properties = json!({"stop":{"type":"integer","minimum":0,"maximum":1,"description":"Color endpoint: 0 is the solid/first color, 1 is the second gradient/motion color."},"rgb":{"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":3,"maxItems":3},"position":{"type":"array","items":{"type":"integer","minimum":0},"minItems":2,"maxItems":2,"description":"Source image pixel [x,y]; ignores annotations and backdrop."}});
    action_properties
        .as_object_mut()
        .unwrap()
        .extend(color_properties.as_object().unwrap().clone());
    vec![
        tool(
            "get_editor_state",
            "Read ask_glance (open/running, prompt, answer, status, error, steps, operation_id), and ChatGPT account metadata, available models, selected OCR engine, menu, sign-in/busy status and welcome state (never credentials). Read extraction (null, or revision, source rectangle, recognized text and engine), live tool, primary selected index, selected_indices and revision-scoped selected_ids, zoom, panels, busy state, operation ID/progress and status. Available while workers are busy, including combined foreground/backdrop animation preview and export. playback.preparing reports the preview loading cover; playback time holds until its first valid frame is ready. Animation operation.progress is the percentage shown in the window's persistent export bar. Use this after dispatch_action to inspect background work; dispatch cancel_export to stop an animation export.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "dispatch_action",
            "Dispatch the same typed action as the native toolbar and shortcuts. Example: action={\"type\":\"select_tool\",\"tool\":\"arrow\"}. Ask Glance: toggle_ask_glance opens the prompt bar; set_ask_glance_prompt (prompt) changes its draft. ask_glance (prompt) sends the annotated image and request to OpenAI using the ChatGPT plan. Inspect/crop/resize/edit annotation/backdrop tools work on a private draft; only completed runs apply as one undoable edit. Poll ask_glance.running/status/answer/error. cancel_ask_glance discards the pending draft; copy_ask_glance_answer copies the final reply. Failure, cancellation or revision/account conflict leaves the document unchanged. OCR: copy_ocr (optional rectangle [x,y,width,height] and engine local/chatgpt) uses the saved engine by default, starts a worker and copies recognized source text directly to the clipboard. ChatGPT OCR sends the image to OpenAI using the signed-in ChatGPT plan. chatgpt_sign_in opens the system browser for consent; cancel_chatgpt_sign_in cancels the pending callback; select_chatgpt_account, sign_out_chatgpt, set_chatgpt_model and set_ocr_engine share native account controls. manage_chatgpt_usage opens usage settings. dismiss_chatgpt_welcome acknowledges the first-use modal. Poll chatgpt.busy for account jobs, which run independently of document operations. Poll get_editor_state for completion and extraction.text; empty results and failures preserve the clipboard. Results expire after document changes. Other examples: fit, copy_image, copy_remote, resize (scale, smart), set_backdrop (backdrop), export_animation (format: mp4/gif). select_annotations (ids from get_document) sets the exact selection, including an empty array to clear it; finish inline text first. select_all selects every annotation (or all inline text while editing); select_region (rectangle [x,y,width,height], additive false by default) selects intersecting annotation bounds. nudge_selection, delete and duplicate_selection operate on the entire selection with one undo step. Copy/paste/undo/redo/delete are contextual to inline text. Image actions commit inline text. Returns revision and operation_id: a non-null ID means background work was accepted, not completed. set_color (color [r,g,b,a]) sets a custom annotation color; sample_tool_color (position [x,y]) samples source pixels and preserves opacity; pick_tool_screen_color opens the interactive screen eyedropper for the active tool or selection. set_backdrop_color (stop 0/1, rgb [r,g,b]) changes one endpoint; sample_backdrop_color (stop, position [x,y]) samples source pixels; pick_backdrop_screen_color (stop) opens the interactive screen eyedropper. randomize_motion chooses a fresh looping variation; optional seed reproduces one. Read backdrop.seed from get_document. Capture/open/save/export may show native dialogs. Existing revision-scoped annotation tools are also available.",
            json!({"action":{"type":"object","properties":action_properties,"required":["type"],"additionalProperties":false},"expected_revision":revision}),
            &["action"],
            false,
        ),
        tool(
            "open_editor",
            "Show the connected native Glance window. Start Glance --automation first.",
            json!({}),
            &[],
            false,
        ),
        tool(
            "get_document",
            "Read dimensions, backdrop, image animation and editable objects. Object IDs are revision-scoped; refresh after any edit.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "import_image",
            "Replace native canvas with an image from a local path OR base64 image bytes OR the system clipboard. This starts a new document; existing document is replaced.",
            json!({"path":path,"base64":{"type":"string"},"clipboard":{"type":"boolean"},"expected_revision":revision}),
            &[],
            false,
        ),
        tool(
            "add_annotation",
            "Add a selectable, undoable annotation to the native canvas.",
            json!({"mark":mark,"expected_revision":revision}),
            &["mark"],
            false,
        ),
        tool(
            "update_annotation",
            "Replace an existing mark using its current object ID.",
            json!({"id":{"type":"string"},"mark":mark,"expected_revision":revision}),
            &["id", "mark"],
            false,
        ),
        tool(
            "move_annotation",
            "Move an object by dx/dy, including its arrow curve.",
            json!({"id":{"type":"string"},"dx":number,"dy":number,"expected_revision":revision}),
            &["id", "dx", "dy"],
            false,
        ),
        tool(
            "delete_annotation",
            "Delete an object by current ID. Undoable.",
            json!({"id":{"type":"string"},"expected_revision":revision}),
            &["id"],
            false,
        ),
        tool(
            "crop_image",
            "Crop source image pixels and translate annotations. Undoable.",
            json!({"x":number,"y":number,"width":number,"height":number,"expected_revision":revision}),
            &["x", "y", "width", "height"],
            false,
        ),
        tool(
            "resize_image",
            "Resize image and editable marks; smart sharpening optional. Undoable.",
            json!({"scale":{"type":"number","minimum":0.1,"maximum":4},"smart":{"type":"boolean"},"expected_revision":revision}),
            &["scale"],
            false,
        ),
        tool(
            "set_backdrop",
            "Set framing and animation. Optional colors [[r,g,b],[r,g,b]] replace the preset with opaque sRGB endpoints; null uses preset colors. Preset 0 teal, 1 ocean, 2 lavender, 3 sunset, 4 rose, 5 cream, 6 slate, 7 white. seed is a reproducible motion variation (0 is the original); it preserves seamless looping. Nebula uses motion=nebula; stars remains a legacy alias, and read-back uses nebula. Omitted properties use defaults. enabled=false removes it.",
            json!({"enabled":{"type":"boolean"},"backdrop":{"type":"object","properties":{"format":{"type":"string","enum":["auto","square","classic","photo","widescreen","portrait","vertical","youtube","shorts","pinterest"]},"gradient":{"type":"boolean"},"motion":{"type":"string","enum":["still","flow","nebula","stars","aurora","contours","paint","prism","liquid","lava"]},"seconds":{"type":"integer","minimum":2,"maximum":15},"seed":{"type":"integer","minimum":0,"maximum":4294967295_u64,"description":"Deterministic looping motion variation; zero preserves the original."},"preset":{"type":"integer","minimum":0,"maximum":7},"colors":{"type":["array","null"],"minItems":2,"maxItems":2,"items":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"integer","minimum":0,"maximum":255}},"description":"Two opaque RGB endpoints; null restores preset colors."},"padding":{"type":"integer","minimum":0,"maximum":512},"inner_radius":{"type":"integer","minimum":0,"maximum":256},"inside_padding":{"type":"integer","minimum":0,"maximum":512},"shadow":{"type":"integer","minimum":0,"maximum":128}},"additionalProperties":false},"expected_revision":revision}),
            &[],
            false,
        ),
        tool(
            "undo",
            "Undo the last native or MCP edit.",
            json!({"expected_revision":revision}),
            &[],
            false,
        ),
        tool(
            "redo",
            "Redo an edit.",
            json!({"expected_revision":revision}),
            &[],
            false,
        ),
        tool(
            "read_image",
            "Rasterize current canvas and return PNG image content to the model. Includes annotations/backdrop. phase is normalized animation time 0..1; max_edge defaults 1600, maximum 4096.",
            json!({"phase":{"type":"number","minimum":0,"maximum":1},"max_edge":{"type":"integer","minimum":64,"maximum":4096}}),
            &[],
            true,
        ),
        tool(
            "export_png",
            "Save full-resolution rasterized PNG to a new local file. Existing files are never overwritten. Omit path for generated export path.",
            json!({"path":path,"phase":{"type":"number","minimum":0,"maximum":1}}),
            &[],
            false,
        ),
        tool(
            "export_mp4",
            "Export backdrop motion and/or image entrance with annotations into H.264 MP4 (30fps, max1920px, 2–15 seconds), starting at time zero. Requires a motion backdrop or image entrance; entrances also work over a still or absent backdrop. Existing files are never overwritten.",
            json!({"path":path}),
            &[],
            false,
        ),
        tool(
            "export_gif",
            "Export an infinitely repeating GIF of backdrop motion and/or image entrance (20fps, max960px, 2–15 seconds), starting at time zero. Uses a fixed palette. Requires a motion backdrop or image entrance; entrances also work over a still or absent backdrop. Existing files are never overwritten.",
            json!({"path":path}),
            &[],
            false,
        ),
        tool(
            "read_video_frame",
            "Decode a frame from an absolute local MP4 path at time seconds and return PNG image content. Uses AVFoundation on macOS or FFmpeg on Linux; remote URLs are rejected.",
            json!({"path":path,"seconds":{"type":"number","minimum":0},"max_edge":{"type":"integer","minimum":64,"maximum":4096}}),
            &["path", "seconds"],
            true,
        ),
    ]
}
fn num(args: &Value, name: &str) -> Result<f32, String> {
    let n = args[name]
        .as_f64()
        .ok_or_else(|| format!("Missing number: {name}"))?;
    if !n.is_finite() || n.abs() > 32768. {
        return Err(format!("{name} out of range"));
    }
    Ok(n as f32)
}
fn mark(args: &Value) -> Result<Mark, String> {
    let mark: Mark = serde_json::from_value(args["mark"].clone()).map_err(|e| e.to_string())?;
    crate::document::actions::validate_mark(&mark)?;
    Ok(mark)
}

fn object(args: &Value, s: &Snapshot) -> Result<usize, String> {
    let id = args["id"].as_str().ok_or("Missing object id")?;
    let (r, i) = id.split_once(':').ok_or("Invalid object id")?;
    let r: u64 = r.parse().map_err(|_| "Invalid revision")?;
    let i: usize = i.parse().map_err(|_| "Invalid index")?;
    if r != s.revision || i >= s.document.marks.len() {
        return Err("Stale object id; call get_document again".into());
    }
    Ok(i)
}
fn phase(args: &Value, default: f32) -> Result<f32, String> {
    if args.get("phase").is_none() {
        return Ok(default);
    }
    let p = num(args, "phase")?;
    if !(0. ..=1.).contains(&p) {
        return Err("phase must be 0..1".into());
    }
    Ok(p)
}
fn image_content(image: image::RgbaImage, args: &Value) -> Result<Value, String> {
    let edge = args
        .get("max_edge")
        .map_or(Ok(1600), |v| v.as_u64().ok_or("Invalid max_edge"))?;
    if !(64..=4096).contains(&edge) {
        return Err("max_edge must be 64..4096".into());
    }
    let edge = (edge as u32).min(image.width().max(image.height()));
    let image = image::DynamicImage::ImageRgba8(image).thumbnail(edge, edge);
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"content":[{"type":"image","mimeType":"image/png","data":STANDARD.encode(png.into_inner())}]}),
    )
}
fn output(args: &Value, extension: &str) -> Result<PathBuf, String> {
    if let Some(p) = args["path"].as_str() {
        let p = PathBuf::from(p);
        if !p.is_absolute() {
            return Err("Use an absolute output path".into());
        }
        return Ok(p);
    }
    let dir = automation::directory()?.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(dir.join(format!("Glance-{}-{stamp}.{extension}", std::process::id())))
}
fn read_local_image(path: &std::path::Path) -> Result<Vec<u8>, String> {
    if !path.is_absolute() {
        return Err("Use an absolute input path".into());
    }
    // Reject devices and FIFOs before opening. Limit the actual read as well as
    // the metadata so a growing file cannot bypass the import size limit.
    if !std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Input must be a local regular file".into());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    const LIMIT: u64 = 16 * 1024 * 1024;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("Input must be a regular file of at most 16 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Input file exceeds 16 MiB".into());
    }
    Ok(bytes)
}
fn decode(bytes: Vec<u8>) -> Result<image::RgbaImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16000);
    limits.max_image_height = Some(16000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
    if img.width() as u64 * img.height() as u64 > 32_000_000 {
        return Err("Image exceeds 32 megapixels".into());
    }
    Ok(img)
}
pub fn operate(name: &str, args: &Value, s: &mut Snapshot) -> Result<(Value, bool, bool), String> {
    validate_tool(name, args)?;
    use crate::document::actions::DocumentAction;
    let mut replace = false;
    let mut edit = None;
    match name {
        "get_document" => return Ok((automation::state(s), false, false)),
        "read_image" => {
            return Ok((
                image_content(s.document.export_at(phase(args, s.phase)?), args)?,
                false,
                false,
            ));
        }
        "import_image" => {
            let sources = usize::from(args["path"].is_string())
                + usize::from(args["base64"].is_string())
                + usize::from(args["clipboard"].as_bool() == Some(true));
            if sources != 1 {
                return Err("Provide exactly one of path, base64 or clipboard=true".into());
            }
            let image = if let Some(path) = args["path"].as_str() {
                decode(read_local_image(std::path::Path::new(path))?)?
            } else if let Some(data) = args["base64"].as_str() {
                if data.len() > 22 * 1024 * 1024 {
                    return Err("Input exceeds 16 MiB".into());
                }
                decode(STANDARD.decode(data).map_err(|e| e.to_string())?)?
            } else {
                let image = crate::platform::clipboard_image()?;
                if image.width() as u64 * image.height() as u64 > 32_000_000 {
                    return Err("Clipboard image exceeds 32 megapixels".into());
                }
                image
            };
            s.document = Document::new(image);
            replace = true;
        }
        "add_annotation" => edit = Some(DocumentAction::AddAnnotation { mark: mark(args)? }),
        "update_annotation" => {
            edit = Some(DocumentAction::UpdateAnnotation {
                index: object(args, s)?,
                mark: mark(args)?,
            })
        }
        "move_annotation" => {
            edit = Some(DocumentAction::MoveAnnotation {
                index: object(args, s)?,
                delta: (num(args, "dx")?, num(args, "dy")?),
                remember: true,
            })
        }
        "delete_annotation" => {
            edit = Some(DocumentAction::DeleteAnnotation {
                index: object(args, s)?,
            })
        }
        "crop_image" => {
            edit = Some(DocumentAction::Crop {
                rectangle: (
                    num(args, "x")?,
                    num(args, "y")?,
                    num(args, "width")?,
                    num(args, "height")?,
                ),
            })
        }
        "resize_image" => {
            edit = Some(DocumentAction::Resize {
                scale: num(args, "scale")?,
                smart: args["smart"].as_bool().unwrap_or(true),
            })
        }
        "set_backdrop" => {
            let backdrop = if args["enabled"].as_bool() == Some(false) {
                None
            } else {
                Some(
                    serde_json::from_value(args.get("backdrop").cloned().unwrap_or(json!({})))
                        .map_err(|e| e.to_string())?,
                )
            };
            edit = Some(DocumentAction::SetBackdrop { backdrop });
        }
        "undo" => edit = Some(DocumentAction::Undo),
        "redo" => edit = Some(DocumentAction::Redo),
        "export_png" => {
            let image = s.document.export_at(phase(args, s.phase)?);
            let path = output(args, "png")?;
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            let result = image::DynamicImage::ImageRgba8(image)
                .write_to(&mut writer, image::ImageFormat::Png)
                .map_err(|e| e.to_string())
                .and_then(|_| writer.flush().map_err(|e| e.to_string()));
            if let Err(e) = result {
                let _ = std::fs::remove_file(&path);
                return Err(e.to_string());
            }
            return Ok((
                json!({"path":path,"mimeType":"image/png","revision":s.revision}),
                false,
                false,
            ));
        }
        "export_mp4" | "export_gif" => {
            let is_gif = name == "export_gif";
            if !s.document.image_animation.enabled()
                && !s
                    .document
                    .backdrop
                    .is_some_and(|b| b.motion != Motion::Still)
            {
                return Err(
                    "Set a motion backdrop or image entrance before exporting an animation".into(),
                );
            }
            let path = output(args, if is_gif { "gif" } else { "mp4" })?;
            // Reserve destination before encoding; release it on any failure.
            let reservation = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            drop(reservation);
            let result = if is_gif {
                crate::gif_export::encode(
                    &s.document,
                    &path,
                    s.phase,
                    &AtomicBool::new(false),
                    |_| {},
                )
            } else {
                crate::video::encode(&s.document, &path, s.phase, &AtomicBool::new(false), |_| {})
            };
            if !matches!(result, Ok(true)) {
                let _ = std::fs::remove_file(&path);
                return Err(result.err().unwrap_or("Video canceled".into()));
            }
            return Ok((
                json!({"path":path,"mimeType":if is_gif {"image/gif"} else {"video/mp4"},"seconds":s.document.animation_seconds(),"fps":if is_gif{20}else{30},"loop":true,"revision":s.revision}),
                false,
                false,
            ));
        }
        "read_video_frame" => {
            let path = std::path::Path::new(args["path"].as_str().ok_or("Missing path")?);
            if !path.is_absolute() || !path.is_file() {
                return Err("Use an absolute path to a local video file".into());
            }
            let seconds = num(args, "seconds")?;
            if seconds < 0. {
                return Err("seconds must be nonnegative".into());
            }
            let frame_dir = tempfile::Builder::new()
                .prefix("glance-video-frame-")
                .tempdir()
                .map_err(|e| e.to_string())?;
            let output = frame_dir.path().join("frame.png");
            let helper = helper("glance-video-frame")?;
            let status = std::process::Command::new(helper)
                .arg(path)
                .arg(seconds.to_string())
                .arg(&output)
                .output()
                .map_err(|e| e.to_string())?;
            let result = if status.status.success() {
                crate::platform::load(&output).and_then(|i| image_content(i, args))
            } else {
                Err(String::from_utf8_lossy(&status.stderr).trim().into())
            };
            let _ = std::fs::remove_file(output);
            return Ok((result?, false, false));
        }
        _ => return Err(format!("Unknown tool: {name}")),
    }
    if let Some(edit) = edit {
        let outcome = edit.apply(&mut s.document)?;
        return Ok((Value::Null, outcome.changed, outcome.reset_view));
    }
    Ok((Value::Null, true, replace))
}
fn helper(name: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("Missing executable directory")?;
    for dir in [dir.to_path_buf(), dir.join(".."), dir.join("../..")] {
        let p = dir.join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(format!(
        "Missing {name}; run scripts/bundle.sh (macOS) or scripts/package-linux.sh first"
    ))
}
fn response(request: Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => Ok(
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"glance","version":env!("CARGO_PKG_VERSION")},"instructions":"Drive the native Glance editor via structured tools. Launch Glance --automation first. Image coordinates exclude backdrop padding. Read get_document before object edits; IDs are revision-scoped. read_image/read_video_frame return model-visible PNGs. Local paths refer to the computer running Glance, not ChatGPT uploaded file IDs; supply base64 bytes or stage files locally. Import replaces the current document; other edits support native undo."}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":tools()})),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or("");
            let args = request["params"]
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));
            let result = validate_tool(name, &args).and_then(|_| automation::call(name, args));
            Ok(match result {
                Ok(v) if v.get("content").is_some() => v,
                Ok(v) => {
                    json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v})
                }
                Err(e) => json!({"isError":true,"content":[{"type":"text","text":e}]}),
            })
        }
        _ => Err(json!({"code":-32601,"message":"Method not found"})),
    };
    Some(match result {
        Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
        Err(e) => json!({"jsonrpc":"2.0","id":id,"error":e}),
    })
}
pub fn run() -> Result<(), String> {
    let mut input = BufReader::new(std::io::stdin());
    let mut out = std::io::stdout().lock();
    while let Some(line) = automation::read_line(&mut input)? {
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => response(request),
            Err(_) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
            ),
        };
        if let Some(response) = response {
            writeln!(out, "{response}").map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn validate_tool(name: &str, args: &Value) -> Result<(), String> {
    let tool = tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or_else(|| format!("Unknown tool: {name}"))?;
    validate(args, &tool["inputSchema"], "arguments")
}
fn validate(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let types: Vec<&str> = if let Some(t) = schema["type"].as_str() {
        vec![t]
    } else {
        schema["type"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    };
    if !types.is_empty()
        && !types.iter().any(|t| match *t {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_u64().is_some() || value.as_i64().is_some(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        })
    {
        return Err(format!("Invalid type: {path}"));
    }
    if let Some(choices) = schema["enum"].as_array()
        && !choices.contains(value)
    {
        return Err(format!("Invalid choice: {path}"));
    }
    if let Some(n) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|min| n < min)
            || schema["exclusiveMinimum"]
                .as_f64()
                .is_some_and(|min| n <= min)
            || schema["maximum"].as_f64().is_some_and(|max| n > max))
    {
        return Err(format!("Out of range: {path}"));
    }
    if let Some(v) = value.as_str()
        && schema["maxLength"]
            .as_u64()
            .is_some_and(|max| v.chars().count() as u64 > max)
    {
        return Err(format!("Too long: {path}"));
    }
    if let Some(a) = value.as_array() {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|min| (a.len() as u64) < min)
            || schema["maxItems"]
                .as_u64()
                .is_some_and(|max| a.len() as u64 > max)
        {
            return Err(format!("Invalid array size: {path}"));
        }
        for item in a {
            validate(item, &schema["items"], path)?;
        }
    }
    if let Some(o) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !o.contains_key(key) {
                    return Err(format!("Missing {path}.{key}"));
                }
            }
        }
        for (key, v) in o {
            if let Some(property) = schema["properties"].get(key) {
                validate(v, property, &format!("{path}.{key}"))?
            } else if schema["additionalProperties"] == false {
                return Err(format!("Unknown field: {path}.{key}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod contract_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            document: Document::new(image::RgbaImage::from_pixel(
                100,
                80,
                image::Rgba([20, 30, 40, 255]),
            )),
            revision: 7,
            phase: 0.,
        }
    }
    fn arrow() -> Value {
        json!({"tool":"arrow","points":[[10,10],[60,50]],"curve":[30,5],"color":[255,56,100,255],"width":3,"text":""})
    }
    #[test]
    fn resized_annotation_geometry_round_trips_through_mcp() {
        let mut s = snapshot();
        let mut mark = arrow();
        mark["width"] = json!(32);
        mark["style"] = json!({"radius":128});
        operate("add_annotation", &json!({"mark":mark}), &mut s).unwrap();
        operate("resize_image", &json!({"scale":4,"smart":false}), &mut s).unwrap();
        let (document, _, _) = operate("get_document", &json!({}), &mut s).unwrap();
        let object = &document["objects"][0];
        let mut mark = object["mark"].clone();
        assert_eq!(mark["width"].as_f64(), Some(128.));
        assert_eq!(mark["style"]["radius"].as_f64(), Some(512.));
        mark["color"] = json!([10, 20, 30, 128]);
        operate(
            "update_annotation",
            &json!({"id":object["id"],"mark":mark}),
            &mut s,
        )
        .unwrap();
        operate("resize_image", &json!({"scale":0.1,"smart":false}), &mut s).unwrap();
        operate("resize_image", &json!({"scale":0.1,"smart":false}), &mut s).unwrap();
        operate("resize_image", &json!({"scale":0.1,"smart":false}), &mut s).unwrap();
        let mark = serde_json::to_value(&s.document.marks[0]).unwrap();
        assert!(mark["width"].as_f64().unwrap() < 0.5);
        operate(
            "update_annotation",
            &json!({"id":"7:0","mark":mark}),
            &mut s,
        )
        .unwrap();
        let mut zero_width = mark;
        zero_width["width"] = json!(0);
        assert!(validate_tool("add_annotation", &json!({"mark":zero_width})).is_err());
    }
    #[test]
    fn editable_workflow_and_raster_readback() {
        let mut s = snapshot();
        operate("add_annotation", &json!({"mark":arrow()}), &mut s).unwrap();
        s.revision += 1;
        assert_eq!(automation::state(&s)["objects"][0]["id"], "8:0");
        assert!(
            operate(
                "move_annotation",
                &json!({"id":"7:0","dx":5,"dy":8}),
                &mut s
            )
            .is_err()
        );
        operate(
            "move_annotation",
            &json!({"id":"8:0","dx":5,"dy":8}),
            &mut s,
        )
        .unwrap();
        assert_eq!(s.document.marks[0].curve, Some((35., 13.)));
        assert_eq!(s.document.marks[0].points[0], (15., 18.));
        let (image, changed, _) = operate("read_image", &json!({}), &mut s).unwrap();
        assert!(!changed);
        let decoded = decode(
            STANDARD
                .decode(image["content"][0]["data"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded.dimensions(), (100, 80));
        assert_ne!(decoded, *s.document.base);
        operate("delete_annotation", &json!({"id":"8:0"}), &mut s).unwrap();
        assert!(s.document.marks.is_empty());
        operate("undo", &json!({}), &mut s).unwrap();
        assert_eq!(s.document.marks.len(), 1);
        operate("undo", &json!({}), &mut s).unwrap();
        assert_eq!(s.document.marks[0].points[0], (10., 10.));
    }
    #[test]
    fn import_crop_resize_backdrop_and_validation() {
        let mut s = snapshot();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8((*s.document.base).clone())
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        operate(
            "import_image",
            &json!({"base64":STANDARD.encode(png.into_inner())}),
            &mut s,
        )
        .unwrap();
        operate(
            "crop_image",
            &json!({"x":5,"y":10,"width":50,"height":40}),
            &mut s,
        )
        .unwrap();
        operate("resize_image", &json!({"scale":2,"smart":false}), &mut s).unwrap();
        assert_eq!(s.document.base.dimensions(), (100, 80));
        operate(
            "set_backdrop",
            &json!({"backdrop":{"motion":"lava","padding":10,"inside_padding":6,"seconds":10}}),
            &mut s,
        )
        .unwrap();
        assert_eq!(s.document.export_at(0.).dimensions(), (132, 112));
        operate(
            "set_backdrop",
            &json!({"backdrop":{"format":"shorts","motion":"liquid","padding":10}}),
            &mut s,
        )
        .unwrap();
        assert_eq!(automation::state(&s)["backdrop"]["format"], "shorts");
        let (w, h) = s.document.export_at(0.37).dimensions();
        assert_eq!(w * 16, h * 9);
        for (name, args) in [
            ("set_backdrop", json!({"backdrop":{"format":"unknown"}})),
            ("set_backdrop", json!({"backdrop":{"preset":8}})),
            ("set_backdrop", json!({"backdrop":{"inside_padding":513}})),
            ("set_backdrop", json!({"backdrop":{"outer_radius":4}})),
            ("resize_image", json!({"scale":2,"smart":"yes"})),
            ("crop_image", json!({"x":-1,"y":0,"width":5,"height":5})),
            ("read_image", json!({"phase":5})),
            ("get_document", json!({"oops":1})),
        ] {
            assert!(operate(name, &args, &mut s).is_err(), "{name}")
        }
        assert!(
            operate(
                "import_image",
                &json!({"base64":"invalid","clipboard":true}),
                &mut s
            )
            .is_err()
        );
    }
    #[test]
    fn mcp_initialization_discovery_and_errors() {
        let initialized=response(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).unwrap();
        assert_eq!(initialized["result"]["serverInfo"]["name"], "glance");
        let list = response(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        assert_eq!(
            list["result"]["tools"].as_array().unwrap().len(),
            tools().len()
        );
        assert!(response(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        assert_eq!(
            response(json!({"id":3,"method":"bad"})).unwrap()["error"]["code"],
            -32601
        );
        assert_eq!(
            response(json!({"id":4,"method":"tools/call","params":{"name":"bad"}})).unwrap()["result"]
                ["isError"],
            true
        );
    }
    #[test]
    fn local_inputs_reject_urls_options_and_special_files_before_decoding() {
        let mut s = snapshot();
        let directory = tempfile::tempdir().unwrap();
        for input in [
            "https://example.com/video.mp4",
            "-help",
            "/dev/zero",
            "/dev/null",
            directory.path().to_str().unwrap(),
        ] {
            assert!(
                operate(
                    "read_video_frame",
                    &json!({"path":input,"seconds":0}),
                    &mut s
                )
                .unwrap_err()
                .contains("local video file")
            );
            assert!(operate("import_image", &json!({"path":input}), &mut s).is_err());
        }
        let image_path = directory.path().join("image.png");
        image::RgbaImage::new(2, 2).save(&image_path).unwrap();
        operate("import_image", &json!({"path":image_path}), &mut s).unwrap();
        assert_eq!(s.document.base.dimensions(), (2, 2));
        let large = directory.path().join("large.png");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(16 * 1024 * 1024 + 1)
            .unwrap();
        assert!(operate("import_image", &json!({"path":large}), &mut s).is_err());
        assert_eq!(s.document.base.dimensions(), (2, 2));
    }
    #[test]
    fn exports_preserve_existing_files() {
        let mut s = snapshot();
        let path =
            std::env::temp_dir().join(format!("glance-mcp-existing-{}.png", std::process::id()));
        std::fs::write(&path, b"original").unwrap();
        assert!(operate("export_png", &json!({"path":path}), &mut s).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    #[ignore = "requires bundled native encoder/decoder; writes real video and reads a frame"]
    fn native_mcp_video_roundtrip() {
        let mut s = Snapshot {
            document: Document::new(image::RgbaImage::from_pixel(
                320,
                180,
                image::Rgba([50, 90, 130, 255]),
            )),
            revision: 0,
            phase: 0.,
        };
        operate(
            "set_backdrop",
            &json!({"backdrop":{"motion":"flow","padding":40,"inner_radius":0,"seconds":2}}),
            &mut s,
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("glance-mcp-video-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.mp4");
        let (result, changed, _) = operate("export_mp4", &json!({"path":path}), &mut s).unwrap();
        assert!(!changed);
        assert_eq!(result["fps"], 30);
        assert!(std::fs::metadata(&path).unwrap().len() > 1000);
        let (image, _, _) = operate(
            "read_video_frame",
            &json!({"path":path,"seconds":1,"max_edge":400}),
            &mut s,
        )
        .unwrap();
        let bytes = STANDARD
            .decode(image["content"][0]["data"].as_str().unwrap())
            .unwrap();
        let decoded = decode(bytes).unwrap();
        assert_eq!(decoded.dimensions(), (400, 260));
        let pixel = decoded.get_pixel(150, 120);
        for (actual, expected) in pixel.0[..3].iter().zip([50i16, 90, 130]) {
            assert!((*actual as i16 - expected).abs() < 8)
        }
        assert!(
            operate(
                "read_video_frame",
                &json!({"path":path,"seconds":2.5}),
                &mut s
            )
            .is_err()
        );
        assert!(operate("export_mp4", &json!({"path":path}), &mut s).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
