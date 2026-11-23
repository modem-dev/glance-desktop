//! Every window, driven through Rune's headless tooling against the fake port and
//! a stand-in `attn`. Trees are read with `--window KEY --tree out.json --at mark:NAME`.
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// A scratch folder with a stand-in `attn` that lists `sessions`, and records what
/// it is sent: its arguments in `args` and each PNG as it was then. `attn model
/// call` keeps its arguments in `model-args` and its stdin in `model-stdin`, adds
/// the request to the lines of `model-requests`, which each run starts anew, then
/// prints `model-out`, and `model-err` on stderr, and exits with `model-code` when
/// a test wrote them; otherwise it answers "Refined: " and the request's transcript.
/// With `model-hang` it waits 600 s instead, then notes in `model-survived` that
/// nothing stopped it. 600 s outlasts any run: one takes 4 s on a Mac and about
/// 65 s on Linux CI, where 20 s let a call outlive the wait before its deadline.
struct Run {
    dir: tempfile::TempDir,
    script: &'static str,
}

impl Run {
    fn new(script: &'static str, sessions: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        attn_lens::stand_in::write_executable(
            &path.join("attn"),
            &format!(
                r#"#!/bin/sh
if [ "$1" = "model" ]; then
  cat > "{dir}/model-stdin"; echo "$@" > "{dir}/model-args"
  tail -n +2 "{dir}/model-stdin" >> "{dir}/model-requests"; echo >> "{dir}/model-requests"
  if [ -f "{dir}/model-hang" ]; then
    sleep 600 < /dev/null > /dev/null 2>&1; echo "$$" >> "{dir}/model-survived"; exit 1
  fi
  if [ -f "{dir}/model-code" ]; then
    cat "{dir}/model-out" 2>/dev/null; cat "{dir}/model-err" >&2 2>/dev/null
    exit "$(cat "{dir}/model-code")"
  fi
  raw="$(tail -n +2 "{dir}/model-stdin" | sed 's/.*\\"transcript\\":\\"\([^\\]*\)\\".*/\1/')"
  echo "{{\"harness\":\"claude\",\"model\":\"sonnet\",\"model_ms\":900,\"text\":\"Refined: $raw\"}}"
  exit 0
fi
cat > /dev/null
if [ "$2" = "sessions" ] && [ -f "{dir}/first-listing" ]; then
  cat "{dir}/first-listing"; rm "{dir}/first-listing"; exit 0
fi
if [ "$2" = "sessions" ]; then echo '{sessions}'; exit 0; fi
echo "$@" > "{dir}/args"
for arg in "$@"; do case "$arg" in *.png) cp "$arg" "{dir}/";; esac; done
echo '{{"id":"cap"}}'
"#,
                dir = path.display()
            ),
        );
        image::RgbaImage::from_pixel(16, 10, image::Rgba([40, 44, 52, 255]))
            .save(path.join("backdrop.png"))
            .unwrap();
        Self { dir, script }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn command(&self, mark: &str) -> Command {
        let script: PathBuf = [env!("CARGO_MANIFEST_DIR"), "headless", self.script]
            .iter()
            .collect();
        // What this run copies and pastes, apart from every other run's.
        let clipboard = self.clipboard(mark);
        let _ = std::fs::remove_dir_all(&clipboard);
        // Each run's requests, apart from the runs before it.
        let _ = std::fs::remove_file(self.path("model-requests"));
        std::fs::create_dir(&clipboard).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_attn-lens"));
        command
            .env("ATTN_BIN", self.path("attn"))
            .env("ATTN_LENS_FAKE_CLIPBOARD", clipboard)
            // Settings live here, apart from the real prefs.json and every other run's.
            .env("RUNE_PREFS_DIR", self.path("prefs"))
            .arg("--desktop")
            .arg(self.path("backdrop.png"))
            .args(["--scale", "1"])
            .arg("--script")
            .arg(script)
            .arg("--at")
            .arg(format!("mark:{mark}"))
            // Each run starts a new log, so a mark's log is only what led to it.
            .env("ATTN_LENS_FAKE_FOCUS", self.path(&format!("focus-{mark}")));
        command
    }

    /// The apps Lens gave the keyboard back to, in order, by the time of `mark`.
    fn focus_returned(&self, mark: &str) -> Vec<String> {
        self.tree("bar", mark);
        std::fs::read_to_string(self.path(&format!("focus-{mark}")))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn clipboard(&self, mark: &str) -> PathBuf {
        self.path(&format!("clipboard-{mark}"))
    }

    /// The pill keys the app in front received up to `mark`, by the last run read at it.
    fn app_keys(&self, mark: &str) -> Vec<String> {
        std::fs::read_to_string(self.clipboard(mark).join("app-keys.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["key"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    /// The texts pasted up to `mark`, by the last run read at it.
    fn pasted(&self, mark: &str) -> Vec<String> {
        std::fs::read_to_string(self.clipboard(mark).join("pasted.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["text"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    /// The text copied up to `mark`, by the last run read at it.
    fn copied(&self, mark: &str) -> Option<String> {
        std::fs::read_to_string(self.clipboard(mark).join("text.txt")).ok()
    }

    fn tree(&self, window: &str, mark: &str) -> Value {
        let out = self.path(&format!("{window}-{mark}.json"));
        let run = self
            .command(mark)
            .args(["--window", window, "--tree"])
            .arg(&out)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
    }
}

fn window<'a>(tree: &'a Value, key: &str) -> &'a Value {
    tree["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["key"] == key)
        .unwrap_or_else(|| panic!("no window {key} in {tree}"))
}

fn visible(tree: &Value, key: &str) -> bool {
    window(tree, key)["visible"].as_bool().unwrap()
}

fn frame(tree: &Value, key: &str) -> Vec<f64> {
    let frame = &window(tree, key)["frame"];
    frame
        .as_array()
        .unwrap_or_else(|| panic!("{key} has no frame: {frame}"))
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

/// An element that is really there: Rune keeps removed ones in the tree while they fade.
fn element<'a>(tree: &'a Value, key: &str) -> Option<&'a Value> {
    tree["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == key && e["exiting"] != true)
}

fn text<'a>(tree: &'a Value, key: &str) -> Option<&'a str> {
    element(tree, key).and_then(|e| e["text"].as_str())
}

fn label<'a>(tree: &'a Value, key: &str) -> Option<&'a str> {
    element(tree, key).and_then(|e| e["label"].as_str())
}

const HANDLES: [&str; 4] = [
    "handle-top-left",
    "handle-top-right",
    "handle-bottom-left",
    "handle-bottom-right",
];

/// The outline on the ink layer, and a handle window at each corner the bar does not
/// cover, inside the area. A covered corner's handle hides: it would stand above the
/// bar and take its clicks.
fn has_area_frame(tree: &Value) {
    assert!(element(tree, "area-outline").is_some(), "no outline");
    let ink = frame(tree, "ink");
    let bar = visible(tree, "bar").then(|| frame(tree, "bar"));
    for key in HANDLES {
        // The corner's 24-point square, from the area: a hidden handle is not placed.
        let x = if key.ends_with("left") {
            ink[0]
        } else {
            ink[0] + ink[2] - 24.
        };
        let y = if key.contains("top") {
            ink[1]
        } else {
            ink[1] + ink[3] - 24.
        };
        let covered = bar.as_ref().is_some_and(|bar| {
            x < bar[0] + bar[2] && bar[0] < x + 24. && y < bar[1] + bar[3] && bar[1] < y + 24.
        });
        if covered {
            assert!(!visible(tree, key), "{key} shows over the bar");
            continue;
        }
        assert!(visible(tree, key), "{key} is hidden");
        let handle = frame(tree, key);
        assert!(
            handle[0] >= ink[0]
                && handle[1] >= ink[1]
                && handle[0] + handle[2] <= ink[0] + ink[2]
                && handle[1] + handle[3] <= ink[1] + ink[3],
            "{key} at {handle:?} is outside the area {ink:?}"
        );
    }
}

const ONE_SESSION: &str = r#"[{"id":"s1","label":"one","agent":"claude","state":"idle"}]"#;
const TWO_SESSIONS: &str = r#"[{"id":"s1","label":"one","agent":"claude","state":"idle","last_focused_at":"2026-11-07T10:00:00Z"},{"id":"s2","label":"two","agent":"codex","state":"working"}]"#;

#[test]
fn the_app_exposes_its_windows() {
    let run = Command::new(env!("CARGO_BIN_EXE_attn-lens"))
        .arg("--list-windows")
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&run.stdout);
    let listed: Vec<&str> = listed.lines().collect();
    assert_eq!(&listed[..2], ["editor", "bar"]);
    assert!(
        listed.contains(&"picker-0") && listed.contains(&"ink"),
        "{listed:?}"
    );
}

#[test]
fn launch_dock_and_capture_keys_reach_the_windows() {
    let run = Run::new("foundation.json", "[]");

    // Lens starts with no window.
    let launched = run.tree("bar", "launched");
    assert!(!visible(&launched, "editor"));
    assert!(!visible(&launched, "bar"));
    assert!(!visible(&launched, "picker-0"));
    assert!(!visible(&launched, "ink"));

    // A Dock click opens the empty editor (D18).
    let dock = run.tree("editor", "dock-clicked");
    assert!(visible(&dock, "editor"));
    assert!(!visible(&dock, "bar"));
    assert!(text(&dock, "editor-empty").is_some());

    // A pick opens a capture and takes no still yet.
    let open = run.tree("bar", "capture-open");
    assert!(visible(&open, "bar"));
    assert_eq!(label(&open, "still-0"), None);

    // The capture key during a capture, a click and a window switch each take a still, in
    // order; voice segments arrive.
    let stills = run.tree("bar", "stills-taken");
    assert_eq!(label(&stills, "still-0"), Some("Hotkey 1280×800"));
    assert_eq!(label(&stills, "still-1"), Some("Click 1280×800"));
    assert_eq!(label(&stills, "still-2"), Some("WindowSwitch 1280×800"));
    assert_eq!(label(&stills, "still-3"), None);
    assert_eq!(text(&stills, "voice"), Some("Listening · right ⌥ to stop"));
    assert_eq!(text(&stills, "segment-0"), Some("this button does nothing"));

    // ⌘⌥↩ without a session names what is missing and keeps the capture.
    let refused = run.tree("bar", "send-refused");
    assert!(visible(&refused, "bar"));
    assert_eq!(label(&refused, "chip"), Some("No sessions in attn"));
    assert_eq!(
        text(&refused, "error"),
        Some("The capture has no target session.")
    );
    assert!(label(&refused, "still-2").is_some());
}

#[test]
fn the_picker_covers_every_display_and_says_both_ways_in() {
    let run = Run::new("pick.json", ONE_SESSION);

    let picking = run.tree("picker-1", "picking");
    assert_eq!(frame(&picking, "picker-0"), [0., 0., 1280., 800.]);
    assert_eq!(frame(&picking, "picker-1"), [1280., 0., 1024., 768.]);
    assert!(visible(&picking, "picker-0"));
    assert!(visible(&picking, "picker-1"));
    assert!(!visible(&picking, "picker-2"), "no third display");
    let hint = text(&picking, "picker-hint").unwrap();
    assert!(hint.contains("Drag for an area"), "{hint}");
    assert!(hint.contains("Click for the whole display"), "{hint}");
    assert!(!hint.contains("last area"), "no last area yet: {hint}");

    // ↩ with no last area leaves the picker open; Esc closes it and starts nothing.
    let enter = run.tree("picker-0", "enter-without-last");
    assert!(visible(&enter, "picker-0"));
    let cancelled = run.tree("bar", "cancelled");
    assert!(!visible(&cancelled, "picker-0"));
    assert!(!visible(&cancelled, "picker-1"));
    assert!(!visible(&cancelled, "bar"));
    assert!(!visible(&cancelled, "ink"));

    // A drag on the first display makes the area, and the bar sits outside it.
    let picked = run.tree("ink", "area-display-1");
    assert!(!visible(&picked, "picker-0"));
    assert_eq!(frame(&picked, "ink"), [200., 100., 400., 300.]);
    has_area_frame(&picked);
    let bar = frame(&picked, "bar");
    assert!(bar[1] >= 400., "the bar overlaps the area: {bar:?}");

    // Esc in the bar closes the capture; the picker now offers the last area.
    let closed = run.tree("bar", "capture-closed");
    assert!(!visible(&closed, "ink"));
    assert!(!visible(&closed, "bar"));
    let again = run.tree("picker-0", "picking-again");
    assert!(text(&again, "picker-hint").unwrap().contains("↩ last area"));
    let last = run.tree("ink", "last-area");
    assert_eq!(frame(&last, "ink"), [200., 100., 400., 300.]);
    assert!(visible(&last, "ink"));
}

#[test]
fn a_click_takes_the_whole_second_display_and_keeps_its_outline() {
    let run = Run::new("pick.json", ONE_SESSION);
    let whole = run.tree("ink", "whole-display-2");
    assert_eq!(frame(&whole, "ink"), [1280., 0., 1024., 768.]);
    has_area_frame(&whole);
    // The outline stays inside the display, where it can be seen.
    let outline = element(&whole, "area-outline").unwrap();
    assert_eq!(outline["rect"], json!([0., 0., 1024., 768.]));
    assert_eq!(
        frame(&whole, "handle-bottom-right"),
        [2280., 744., 24., 24.]
    );
    // No room outside a whole display, so the bar stays on it, inside the bottom edge.
    let bar = frame(&whole, "bar");
    assert!(bar[0] >= 1280., "the bar left the display: {bar:?}");
    assert!(bar[1] + bar[3] <= 768., "the bar left the display: {bar:?}");

    // The outline and handles stay for the rest of the capture: a still changes nothing.
    let still = run.tree("ink", "still-taken");
    has_area_frame(&still);
    let strip = run.tree("bar", "still-taken");
    assert_eq!(label(&strip, "still-0"), Some("Hotkey 2048×1536"));
}

#[test]
fn an_area_on_the_second_display_adjusts_by_its_corners() {
    let run = Run::new("pick.json", ONE_SESSION);
    let picked = run.tree("ink", "area-display-2");
    assert_eq!(frame(&picked, "ink"), [1400., 100., 300., 200.]);
    assert!(frame(&picked, "bar")[1] >= 300.);
    has_area_frame(&picked);

    let bottom_right = run.tree("ink", "bottom-right-moved");
    assert_eq!(frame(&bottom_right, "ink"), [1400., 100., 350., 240.]);
    has_area_frame(&bottom_right);

    // Moving the top-left corner moves the window under the pointer, and it still lands.
    let top_left = run.tree("ink", "top-left-moved");
    assert_eq!(frame(&top_left, "ink"), [1380., 90., 370., 250.]);
    has_area_frame(&top_left);

    // On the display with the menu bar, the top handles sit below it, where they
    // cannot take the Apple menu's or the clock's clicks.
    let whole = run.tree("ink", "whole-display-1");
    assert_eq!(frame(&whole, "ink"), [0., 0., 1280., 800.]);
    assert_eq!(frame(&whole, "handle-top-left"), [0., 24., 24., 24.]);
    assert_eq!(frame(&whole, "handle-top-right"), [1256., 24., 24., 24.]);
    assert_eq!(
        frame(&whole, "handle-bottom-right"),
        [1256., 776., 24., 24.]
    );
}

#[test]
fn the_draw_key_draws_the_picked_tool_and_shift_makes_a_box() {
    let run = Run::new("capture.json", ONE_SESSION);

    // With the draw key up, nothing is drawn.
    let open = run.tree("ink", "area-picked");
    assert_eq!(label(&open, "mark-0"), None);

    // fn+2 picks Arrow, and the first press took the still, before ink.
    let arrow = run.tree("ink", "arrow-drawn");
    assert_eq!(label(&arrow, "mark-0"), Some("arrow"));
    let bar = run.tree("bar", "arrow-drawn");
    assert_eq!(label(&bar, "still-0"), Some("Draw 800×600"));

    // During a stroke, what is drawn so far shows.
    let mid = run.tree("ink", "mid-stroke");
    assert_eq!(label(&mid, "draft"), Some("freehand"));
    // The handles are off while drawing, so a stroke can start on a corner.
    for key in HANDLES {
        assert!(!visible(&mid, key), "{key} shows while drawing");
    }

    // fn+1 goes back to Pen, and its stroke joins the still that press began.
    let freehand = run.tree("ink", "freehand-drawn");
    assert_eq!(label(&freehand, "mark-1"), Some("freehand"));
    assert_eq!(label(&run.tree("bar", "freehand-drawn"), "still-1"), None);

    // ⇧ with the draw key makes a box.
    let boxed = run.tree("ink", "box-drawn");
    assert_eq!(label(&boxed, "mark-2"), Some("box"));
    assert!(element(&boxed, "draft").is_none());

    // With the key up again, a press draws nothing and takes no still.
    let up = run.tree("ink", "press-with-key-up");
    assert_eq!(label(&up, "mark-3"), None);
    has_area_frame(&up);
    assert_eq!(
        label(&run.tree("bar", "press-with-key-up"), "still-1"),
        None
    );

    // A capture-key still ends the view, so the next stroke takes a still of its own.
    let bar = run.tree("bar", "hotkey-still");
    assert_eq!(label(&bar, "still-1"), Some("Hotkey 800×600"));
    let bar = run.tree("bar", "drawn-after-still");
    assert_eq!(label(&bar, "still-2"), Some("Draw 800×600"));

    // So does moving a corner: the next stroke's still covers the new area.
    let resized = run.tree("ink", "drawn-after-resize");
    assert_eq!(frame(&resized, "ink"), [200., 100., 440., 330.]);
    assert_eq!(label(&resized, "mark-4"), Some("freehand"));
    let bar = run.tree("bar", "drawn-after-resize");
    assert_eq!(label(&bar, "still-3"), Some("Draw 880×660"));

    // With every still dropped, ⌘⌥↩ takes one of the area and sends it (D7).
    let dropped = run.tree("bar", "all-dropped");
    assert_eq!(label(&dropped, "still-0"), None);
    let sent = run.tree("bar", "sent-after-drops");
    assert!(!visible(&sent, "bar"), "the capture stayed open");
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s1 "), "{args}");
    assert!(run.path("frame-01.png").exists(), "{args}");
}

/// What each mark on the ink layer is, in order.
fn ink_kinds(run: &Run, mark: &str) -> Vec<String> {
    let tree = run.tree("ink", mark);
    (0..)
        .map_while(|i| label(&tree, &format!("mark-{i}")).map(str::to_owned))
        .collect()
}

/// The tool strip as the ink layer shows it: its rect, in the layer's points, and the tool it names.
fn strip(run: &Run, mark: &str) -> Option<([f64; 4], String)> {
    let tree = run.tree("ink", mark);
    element(&tree, "tool-strip")?;
    Some((
        rect(&tree, "tool-strip"),
        label(&tree, "tool-strip")?.to_owned(),
    ))
}

/// Every tool's chip is inside the layer, and none covers `pointer`.
fn chips_clear_of(tree: &Value, pointer: (f64, f64), layer: (f64, f64)) {
    for tool in ["pen", "arrow", "box", "highlight", "label", "counter"] {
        let [x, y, w, h] = rect(tree, &format!("tool-{tool}"));
        assert!(
            x >= 0. && y >= 0. && x + w <= layer.0 && y + h <= layer.1,
            "{tool} at {:?} leaves the layer",
            [x, y, w, h]
        );
        let covers = (x..=x + w).contains(&pointer.0) && (y..=y + h).contains(&pointer.1);
        assert!(!covers, "{tool} at {:?} covers the pointer", [x, y, w, h]);
    }
}

#[test]
fn near_a_corner_the_arc_turns_to_stay_on_screen() {
    let run = Run::new("tool-strip-edge.json", ONE_SESSION);
    // The pointer at the area's top left corner, (10, 10) on the layer.
    let tree = run.tree("ink", "strip-shown");
    chips_clear_of(&tree, (10., 10.), (600., 450.));
    // A stroke from the pointer draws.
    assert_eq!(ink_kinds(&run, "drawn"), ["freehand"]);
}

#[test]
fn fn_shows_the_tool_strip_and_each_stroke_is_its_tool() {
    let run = Run::new("tool-strip.json", ONE_SESSION);

    // Without fn, a digit belongs to the app in front, and the tool stays Pen.
    assert_eq!(state(&run, "digit-before-fn")["tool"], "Pen");
    assert_eq!(run.app_keys("digit-before-fn"), ["key:2"]);

    // fn fans the tools out around where the pointer was, (220, 260) on the
    // layer, with no chip over it, so the first stroke starts clear.
    let (_, tool) = strip(&run, "strip-shown").expect("no strip while fn is down");
    assert_eq!(tool, "Pen");
    chips_clear_of(&run.tree("ink", "strip-shown"), (220., 260.), (600., 450.));

    // A straight drag with Pen stays freehand: nothing is guessed from its shape.
    assert_eq!(ink_kinds(&run, "pen-drawn"), ["freehand"]);

    // fn+2 picks Arrow without reaching the app in front, and the strip says so.
    assert_eq!(strip(&run, "arrow-picked").unwrap().1, "Arrow");
    assert_eq!(run.app_keys("arrow-picked"), ["key:2"]);
    assert_eq!(ink_kinds(&run, "arrow-drawn")[1..], ["arrow"]);

    // ⇧ makes a box with Pen or Arrow.
    assert_eq!(ink_kinds(&run, "shift-boxed")[2..], ["box"]);

    // A click on the strip picks its tool and draws nothing.
    assert_eq!(state(&run, "box-clicked")["tool"], "Box");
    assert_eq!(ink_kinds(&run, "box-clicked").len(), 3);
    assert_eq!(ink_kinds(&run, "box-drawn")[3..], ["box"]);

    assert_eq!(ink_kinds(&run, "highlight-drawn")[4..], ["highlight"]);
    assert_eq!(ink_kinds(&run, "counted")[5..], ["counter", "counter"]);
    let still = &state(&run, "counted")["capture"]["stills"][0];
    assert_eq!(
        still["kinds"],
        json!([
            "freehand",
            "arrow",
            "box",
            "box",
            "highlight",
            "counter",
            "counter"
        ])
    );

    // Letting go of fn hides the strip and gives the digits back.
    assert!(strip(&run, "strip-gone").is_none(), "the strip stayed");
    assert_eq!(state(&run, "digit-after-fn")["tool"], "Counter");
    assert_eq!(run.app_keys("digit-after-fn"), ["key:2", "key:1"]);

    // The tool sticks until changed.
    assert_eq!(strip(&run, "counter-sticks").unwrap().1, "Counter");

    // Label places a label that takes its text in the bar.
    assert!(element(&run.tree("bar", "label-placed"), "label").is_some());
}

#[test]
fn a_still_left_from_a_closed_capture_does_not_stop_the_next_from_sending() {
    // Capture A asks for a still and closes before it lands; capture B, with no
    // still of its own, sends while A's is still on the way.
    let run = Run::new("stale-still.json", TWO_SESSIONS);
    let closed = run.tree("bar", "a-closed");
    assert!(!visible(&closed, "bar"), "capture A stayed open");
    let sent = run.tree("bar", "b-sent");
    assert!(!visible(&sent, "bar"), "capture B stayed open");
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s1 "), "{args}");
    let frame = image::open(run.path("frame-01.png")).unwrap().to_rgba8();
    assert_eq!((frame.width(), frame.height()), (400, 300), "not B's area");
}

/// The capture as the ink layer's state element reports it.
fn capture_state(run: &Run, mark: &str) -> Value {
    let tree = run.tree("ink", mark);
    let state = label(&tree, "capture-state").expect("no capture-state element");
    serde_json::from_str::<Value>(state).unwrap()["capture"].clone()
}

fn causes(capture: &Value) -> Vec<&str> {
    capture["stills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["cause"].as_str().unwrap())
        .collect()
}

fn ink_count(capture: &Value) -> usize {
    capture["ink"].as_array().unwrap().len()
}

#[test]
fn a_send_whose_still_fails_keeps_the_capture_and_retries() {
    let run = Run::new("send-still-fails.json", ONE_SESSION);
    let failed = capture_state(&run, "failed");
    assert_eq!(causes(&failed), ["hotkey"]);
    assert!(!run.path("args").exists(), "the stale stills were sent");
    let bar = run.tree("bar", "failed");
    assert!(
        serde_json::to_string(&bar)
            .unwrap()
            .contains("The fake still failed."),
        "the failure is not shown"
    );

    let run = Run::new("send-still-fails.json", ONE_SESSION);
    run.tree("bar", "retried");
    assert!(
        run.path("frame-02.png").exists(),
        "the retry sent no new still"
    );
}

#[test]
fn an_event_from_a_stopped_watch_leaves_the_new_capture_alone() {
    let run = Run::new("late-watch.json", ONE_SESSION);
    assert_eq!(ink_count(&capture_state(&run, "after-late")), 1);
    assert_eq!(ink_count(&capture_state(&run, "after-own")), 0);
}

#[test]
fn a_failed_click_still_leaves_the_change_for_send_to_capture() {
    let run = Run::new("click-still-fails.json", ONE_SESSION);
    run.tree("bar", "sent");
    assert!(
        run.path("frame-02.png").exists(),
        "the click's change was not sent"
    );
}

#[test]
fn a_change_from_before_the_stroke_leaves_its_ink() {
    let run = Run::new("change-before-still.json", ONE_SESSION);
    assert_eq!(ink_count(&capture_state(&run, "sampled")), 1);
}

#[test]
fn a_change_after_the_stroke_fades_its_ink() {
    let run = Run::new("change-after-still.json", ONE_SESSION);
    assert_eq!(ink_count(&capture_state(&run, "sampled")), 0);
}

#[test]
fn stills_come_with_their_causes_and_ink_fades_only_once_its_still_exists() {
    let run = Run::new("stills.json", ONE_SESSION);

    // A scroll stales the ink, but it stays until its frame is saved.
    let unsaved = capture_state(&run, "scrolled-unsaved");
    assert_eq!(causes(&unsaved), Vec::<&str>::new());
    assert_eq!(ink_count(&unsaved), 1);
    assert_eq!(unsaved["ink"][0]["fading"], false);

    let fading = capture_state(&run, "fading");
    assert_eq!(causes(&fading), ["draw"]);
    assert_eq!(fading["ink"][0]["fading"], true);
    assert_eq!(ink_count(&capture_state(&run, "faded")), 0);

    // The scroll ended the view, so the next stroke has its own still.
    let second = capture_state(&run, "drew-after-scroll");
    assert_eq!(causes(&second), ["draw", "draw"]);
    assert_eq!(ink_count(&second), 1);

    // A click takes a still and fades the ink.
    let clicked = capture_state(&run, "clicked");
    assert_eq!(causes(&clicked), ["draw", "draw", "click"]);
    assert_eq!(ink_count(&clicked), 0);

    // A change below the threshold leaves ink alone; most of the area clears it.
    assert_eq!(ink_count(&capture_state(&run, "small-change")), 1);
    assert_eq!(ink_count(&capture_state(&run, "big-change")), 0);

    let more = capture_state(&run, "two-more");
    assert_eq!(
        causes(&more),
        ["draw", "draw", "click", "draw", "window_switch", "hotkey"]
    );
    for still in more["stills"].as_array().unwrap() {
        for span in still["times"].as_array().unwrap() {
            let (start, end) = (span[0].as_u64().unwrap(), span[1].as_u64().unwrap());
            assert!(
                still["at"].as_u64().unwrap() <= start,
                "frame after mark: {still}"
            );
            assert!(start <= end, "span runs backward: {still}");
        }
    }
    assert_eq!(more["stills"][0]["marks"], 1);
    assert_eq!(more["stills"][0]["times"].as_array().unwrap().len(), 1);

    // After a resize the watch covers the new area.
    assert_eq!(causes(&capture_state(&run, "after-resize"))[6], "click");

    // The scroll after the last still gets one at Send.
    let send = capture_state(&run, "send-still");
    assert_eq!(*causes(&send).last().unwrap(), "send");
    assert!(
        !visible(&run.tree("bar", "sent"), "bar"),
        "send left it open"
    );
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    // Eight frames, and two sheets for them.
    assert_eq!(args.matches("--file ").count(), 10, "{args}");
}

#[test]
fn a_corner_drag_keeps_the_opposite_corner_where_it_was() {
    // The area spans 100–200 across. The top-left corner, dragged past the
    // minimum and back, moves only its own edges; so does the bottom-right
    // corner dragged off the display and back.
    let run = Run::new("corners.json", TWO_SESSIONS);
    assert_eq!(
        frame(&run.tree("bar", "picked"), "ink"),
        [100., 100., 100., 200.]
    );
    assert_eq!(
        frame(&run.tree("bar", "past-anchor"), "ink"),
        [164., 100., 36., 200.]
    );
    assert_eq!(
        frame(&run.tree("bar", "back-from-anchor"), "ink"),
        [150., 100., 50., 200.]
    );
    assert_eq!(
        frame(&run.tree("bar", "off-display"), "ink"),
        [150., 100., 1130., 700.]
    );
    assert_eq!(
        frame(&run.tree("bar", "back-on-display"), "ink"),
        [150., 100., 150., 300.]
    );
}

#[test]
fn the_bar_owns_the_keyboard_and_takes_a_label_s_text() {
    let run = Run::new("bar.json", TWO_SESSIONS);

    // The bar starts on the session last focused in attn, with the keys.
    let open = run.tree("bar", "open");
    assert_eq!(label(&open, "chip"), Some("→ one · this Mac"));
    assert_eq!(text(&open, "chip-target"), Some("→ one"));
    assert!(
        element(&open, "chip-target").unwrap().get("help").is_none(),
        "a short label was cut"
    );
    assert_eq!(text(&open, "keys"), Some("Typing here"));
    for key in ["message", "edit", "send"] {
        assert!(element(&open, key).is_some(), "{key} is missing");
    }
    // Edit, Send and the chip stand at one height.
    let height = |key: &str| element(&open, key).unwrap()["rect"][3].as_f64().unwrap();
    assert_eq!(height("edit"), height("send"));
    assert_eq!(height("chip"), height("send"));

    assert_eq!(text(&run.tree("bar", "typed"), "message"), Some("hi"));

    // fn and a digit pick a tool, and it stays picked.
    let highlighted = run.tree("ink", "highlighted");
    assert_eq!(label(&highlighted, "mark-0"), Some("highlight"));

    let counted = run.tree("ink", "counted");
    assert_eq!(label(&counted, "mark-1"), Some("counter"));
    assert_eq!(label(&counted, "mark-2"), Some("counter"));

    // A label takes its text in the bar, then typing returns to the message.
    let placing = run.tree("bar", "label-placed");
    assert!(element(&placing, "label").is_some());
    assert!(element(&placing, "message").is_none());
    let labelled = run.tree("ink", "labelled");
    assert_eq!(label(&labelled, "mark-3"), Some("label"));
    assert_eq!(text(&labelled, "mark-3"), Some("here"));
    let message = run.tree("bar", "labelled");
    assert_eq!(text(&message, "message"), Some("hi"));

    // A page click gives the page the keys, and the field says so. ⌘⌥C takes them back.
    let page = run.tree("bar", "page-clicked");
    assert_eq!(text(&page, "keys"), Some("Typing on the page"));
    assert_eq!(
        text(&run.tree("bar", "keys-back"), "keys"),
        Some("Typing here")
    );
    assert_eq!(
        text(&run.tree("bar", "typed-again"), "message"),
        Some("hi!")
    );
}

/// Eight sessions as attn lists them: a long label, every state, and four on an
/// outpost, which attn names by its id.
const EIGHT_SESSIONS: &str = r#"[{"id":"s1","label":"Lens walkthrough with the user","agent":"claude","state":"waiting_input","last_focused_at":"2026-11-13T10:00:00Z"},{"id":"s2","label":"Review main","agent":"codex","state":"idle"},{"id":"s3","label":"Bar polish","agent":"claude","state":"working"},{"id":"s4","label":"Notebook sync","agent":"claude","state":"pending_approval"},{"id":"s5","label":"Outpost build","agent":"claude","state":"working","host":"7a35fcaf-012f-4a33-8239-1da4d324514f"},{"id":"s6","label":"Speech helper models","agent":"codex","state":"idle","host":"7a35fcaf-012f-4a33-8239-1da4d324514f"},{"id":"s7","label":"Test history service token for the sandbox","agent":"claude","state":"waiting_input","host":"7a35fcaf-012f-4a33-8239-1da4d324514f"},{"id":"s8","label":"Tart VM","agent":"claude","state":"launching","host":"7a35fcaf-012f-4a33-8239-1da4d324514f","host_label":"ai-sandbox"}]"#;

fn rect(tree: &Value, key: &str) -> [f64; 4] {
    let rect = &element(tree, key).unwrap_or_else(|| panic!("no {key}"))["rect"];
    std::array::from_fn(|i| rect[i].as_f64().unwrap())
}

fn inside(tree: &Value, child: &str, parent: &str) {
    let (c, p) = (rect(tree, child), rect(tree, parent));
    assert!(
        c[0] >= p[0] && c[0] + c[2] <= p[0] + p[2] && c[1] >= p[1] && c[1] + c[3] <= p[1] + p[3],
        "{child} at {c:?} crosses {parent} at {p:?}"
    );
}

#[test]
fn a_long_session_label_ends_in_an_ellipsis_and_the_palette_reads_as_words() {
    let run = Run::new("bar-sessions.json", EIGHT_SESSIONS);

    // The label is cut inside the chip; the host after it stays whole.
    let open = run.tree("bar", "open");
    let full = "→ Lens walkthrough with the user · this Mac";
    assert_eq!(label(&open, "chip"), Some(full));
    assert_eq!(
        element(&open, "chip-target").unwrap()["help"].as_str(),
        Some(full),
        "the long label was not cut"
    );
    assert_eq!(text(&open, "chip-host"), Some("· this Mac"));
    inside(&open, "chip-target", "chip");
    inside(&open, "chip-host", "chip");

    // Each row: the label, the state in words, and where it runs, never an id.
    let palette = run.tree("bar", "palette-open");
    inside(&palette, "chip-host", "chip");
    let meta = |index: usize| text(&palette, &format!("palette-meta-{index}")).unwrap();
    assert_eq!(meta(0), "claude · waiting for you · this Mac");
    assert_eq!(meta(3), "claude · needs approval · this Mac");
    assert_eq!(meta(4), "claude · working · outpost 7a35fcaf");
    assert_eq!(meta(7), "claude · starting · ai-sandbox");
    for index in 0..8 {
        let row = meta(index);
        assert!(!row.contains('_'), "row {index} shows an enum: {row}");
        assert!(!row.contains("7a35fcaf-"), "row {index} shows an id: {row}");
        inside(
            &palette,
            &format!("palette-meta-{index}"),
            &format!("palette-row-{index}"),
        );
    }
}

#[test]
fn the_strip_drops_stills_the_palette_chooses_and_send_delivers() {
    let run = Run::new("bar.json", TWO_SESSIONS);

    let stills = run.tree("bar", "three-stills");
    assert_eq!(label(&stills, "still-0"), Some("Draw 800×600"));
    assert_eq!(label(&stills, "still-1"), Some("Click 800×600"));
    assert_eq!(label(&stills, "still-2"), Some("Hotkey 800×600"));
    assert!(element(&stills, "drop-2").is_some());

    let dropped = run.tree("bar", "still-dropped");
    assert!(label(&dropped, "still-1").is_some());
    assert_eq!(label(&dropped, "still-2"), None);

    let palette = run.tree("bar", "palette-open");
    assert!(element(&palette, "palette-row-0").is_some());
    assert!(element(&palette, "palette-row-1").is_some());
    let closed = frame(&dropped, "bar");
    assert!(
        frame(&palette, "bar")[3] > closed[3],
        "the bar did not grow"
    );

    // ↓ moves the selection, and the selected row stands out.
    let moved = run.tree("bar", "palette-moved");
    let fill = |tree: &Value, key: &str| element(tree, key).unwrap()["fill"].clone();
    assert_ne!(fill(&moved, "palette-row-1"), fill(&moved, "palette-row-0"));

    let chosen = run.tree("bar", "session-chosen");
    assert!(element(&chosen, "palette-row-0").is_none());
    assert_eq!(
        frame(&chosen, "bar")[3],
        closed[3],
        "the bar did not shrink"
    );

    // ⌘⌥↩ sends from anywhere: the chosen session, the message, and the kept stills.
    let sent = run.tree("bar", "sent");
    assert!(!visible(&sent, "bar"));
    assert!(!visible(&sent, "ink"));
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s2 "), "{args}");
    assert!(args.contains("-m hi!\n\nScreen capture"), "{args}");
    assert!(run.path("frame-02.png").exists());
    assert!(
        !Path::new(&run.path("frame-03.png")).exists(),
        "the dropped still was sent"
    );

    // The sent frame carries the marks; the area's outline never reaches it.
    let frame_one = image::open(run.path("frame-01.png")).unwrap().to_rgba8();
    assert_eq!((frame_one.width(), frame_one.height()), (800, 600));
    let untouched = |x: u32, y: u32| [(x % 256) as u8, (y % 256) as u8, 128, 255];
    for corner in [(0, 0), (799, 0), (0, 599), (799, 599), (12, 12)] {
        assert_eq!(
            frame_one.get_pixel(corner.0, corner.1).0,
            untouched(corner.0, corner.1)
        );
    }
    let (x, y) = (2 * (300 - 200), 2 * (250 - 100));
    assert_ne!(
        frame_one.get_pixel(x, y).0,
        untouched(x, y),
        "the counter is not on the frame"
    );
}

#[test]
fn a_failed_listing_names_itself_and_enter_asks_again() {
    let run = Run::new("capture.json", "not json");
    let bar = run.tree("bar", "area-picked");
    assert_eq!(label(&bar, "chip"), Some("Sessions unavailable"));
    let status = text(&bar, "status").unwrap();
    assert!(status.contains("JSON Lens cannot read"), "{status}");
    assert!(status.contains("list the sessions again"), "{status}");
}

#[test]
fn an_unpaired_lens_pairs_from_the_bar() {
    let run = Run::new("pair.json", ONE_SESSION);
    // attn refuses the token until `attn send pair` has run.
    let attn = std::fs::read_to_string(run.path("attn")).unwrap();
    let dir = run.dir.path().display();
    attn_lens::stand_in::write_executable(
        &run.path("attn"),
        &attn.replacen(
            "cat > /dev/null\n",
            &format!(
                r#"cat > /dev/null
if [ "$2" = "pair" ]; then
  touch "{dir}/paired"
  echo '{{"id":"p1","name":"Attn Lens","code":"4242","requested_at":"2026-11-03T00:00:00Z"}}'
  echo '{{"sender":{{"id":"s","name":"Attn Lens","created_at":"2026-11-03T00:00:00Z"}},"token":"tok"}}'
  exit 0
fi
if [ ! -e "{dir}/paired" ]; then echo 'token unknown: run attn send pair' >&2; exit 1; fi
"#
            ),
            1,
        ),
    );

    let unpaired = run.tree("bar", "unpaired");
    assert_eq!(label(&unpaired, "chip"), Some("Not paired with attn"));
    assert!(
        text(&unpaired, "status")
            .unwrap()
            .contains("Press ↩ to pair")
    );

    let paired = run.tree("bar", "paired");
    assert_eq!(label(&paired, "chip"), Some("→ one · this Mac"));
    assert_eq!(text(&paired, "status"), None);
}

#[test]
fn holding_the_voice_key_talks_until_release_and_a_tap_toggles() {
    let run = Run::new("voice-hold.json", ONE_SESSION);
    let voice = |mark| text(&run.tree("bar", mark), "voice").map(str::to_string);

    // The voice key with no capture open dictates: with nothing said it pastes
    // nothing, and the capture key still works.
    assert_eq!(voice("open").as_deref(), Some("right ⌥ voice"));
    let open = state(&run, "open");
    assert_eq!(open["capture"]["segments"], serde_json::json!([]));
    assert_eq!(open["error"], Value::Null);

    // A hold starts voice and its release stops it, keeping what was said.
    assert_eq!(
        voice("holding").as_deref(),
        Some("Listening · right ⌥ to stop")
    );
    assert_eq!(voice("released").as_deref(), Some("right ⌥ voice"));
    assert_eq!(
        text(&run.tree("bar", "released"), "segment-0"),
        Some("held words")
    );

    // A hold while a tap's voice runs keeps it, and the release stops it.
    assert_eq!(
        voice("tapped-then-held").as_deref(),
        Some("Listening · right ⌥ to stop")
    );
    assert_eq!(
        voice("tapped-then-released").as_deref(),
        Some("right ⌥ voice")
    );
    // A release with voice off starts nothing.
    assert_eq!(voice("stray-release").as_deref(), Some("right ⌥ voice"));
}

#[test]
fn the_bar_shows_how_far_voice_has_got_and_waits_for_the_last_sentence() {
    let run = Run::new("voice.json", ONE_SESSION);
    let voice = |mark| text(&run.tree("bar", mark), "voice").map(str::to_string);

    assert_eq!(voice("open").as_deref(), Some("right ⌥ voice"));
    assert_eq!(voice("starting").as_deref(), Some("Starting voice…"));
    // The first voice fetches the model, with progress.
    assert_eq!(
        voice("downloading").as_deref(),
        Some("Downloading speech model 25%")
    );
    assert_eq!(voice("loading").as_deref(), Some("Loading speech model…"));
    assert_eq!(voice("warming").as_deref(), Some("Starting microphone…"));
    assert_eq!(
        voice("listening").as_deref(),
        Some("Listening · right ⌥ to stop")
    );
    assert_eq!(
        text(&run.tree("bar", "segment"), "segment-0"),
        Some("open the settings")
    );

    // Stopping waits for the sentence in progress, and a voice tap then does nothing.
    assert_eq!(voice("finishing").as_deref(), Some("Finishing…"));
    assert_eq!(voice("still-finishing").as_deref(), Some("Finishing…"));
    assert_eq!(voice("stopped").as_deref(), Some("right ⌥ voice"));

    // Voice that fails on its own says why and leaves the capture open.
    let failed = run.tree("bar", "failed");
    assert_eq!(text(&failed, "voice"), Some("right ⌥ voice"));
    assert_eq!(
        text(&failed, "error"),
        Some("Voice stopped: the speech helper quit.")
    );
    assert!(visible(&failed, "bar"));

    // Send waits for voice to finish, and the bar says so.
    let finishing = run.tree("bar", "send-finishing");
    assert_eq!(text(&finishing, "voice"), Some("Finishing…"));
    assert!(visible(&finishing, "bar"));
    assert!(
        !run.path("args").exists(),
        "nothing sent before voice finished"
    );
    assert!(!visible(&run.tree("bar", "sent"), "bar"));
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s1 "), "{args}");

    // A voice tap while a send is delivering starts voice on that capture; the send
    // closing the capture stops it, so the next capture's voice starts afresh.
    assert_eq!(
        text(&run.tree("bar", "voice-during-send"), "voice"),
        Some("Starting voice…")
    );
    let again = run.tree("bar", "sent-again");
    assert!(
        !visible(&again, "bar"),
        "{:?} {:?} {:?}",
        text(&again, "error"),
        text(&again, "status"),
        text(&again, "voice")
    );
    let after = run.tree("bar", "voice-after-send");
    assert_eq!(text(&after, "voice"), Some("Starting voice…"));
    assert_eq!(text(&after, "error"), None);
}

#[test]
fn an_older_listing_finishing_last_cannot_change_the_target() {
    let run = Run::new("listing-order.json", ONE_SESSION);
    std::fs::write(
        run.path("first-listing"),
        r#"[{"agent":"claude","id":"s2","label":"newest","state":"idle"}]"#,
    )
    .unwrap();
    run.tree("bar", "sent");
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s2 "), "{args}");
}

#[test]
fn send_delivers_the_timeline_a_sheet_and_the_frames() {
    let run = Run::new("deliver.json", ONE_SESSION);
    run.tree("bar", "sent");
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.contains("still 1 (draw)"), "{args}");
    assert!(args.contains("on still 1"), "{args}");
    assert!(args.contains("\"it breaks here\""), "{args}");
    assert!(args.contains("still 2 (hotkey)"), "{args}");
    assert_eq!(args.matches("--file ").count(), 3, "{args}");
    let sheet = image::open(run.path("sheet.png")).unwrap();
    assert!(sheet.width().max(sheet.height()) <= 1600);
    assert!(run.path("frame-02.png").exists());
}

#[test]
fn voice_on_a_new_capture_does_not_wait_for_the_last_one_to_stop() {
    let run = Run::new("voice-new-capture.json", ONE_SESSION);

    // Esc closed the capture and its voice is still stopping, held in a job.
    let second = run.tree("bar", "second-voice");
    assert_eq!(text(&second, "voice"), Some("Listening · right ⌥ to stop"));
    assert_eq!(text(&second, "error"), None);

    // The old voice ending leaves the new one running.
    let later = run.tree("bar", "old-voice-finished");
    assert_eq!(text(&later, "voice"), Some("Listening · right ⌥ to stop"));

    // A closed capture's voice failing while it stops is not the next capture's error.
    let clean = run.tree("bar", "after-closed-voice-failed");
    assert_eq!(text(&clean, "error"), None);
}

#[test]
fn a_closed_captures_finish_does_not_start_the_next_captures_send() {
    let run = Run::new("voice-closed-finish.json", ONE_SESSION);

    // The first capture's finish returned, then the newest job ran: the second
    // capture's own finish, not a send the first one started in its place.
    run.tree("bar", "old-finish-returned");
    assert!(!run.path("args").exists(), "sent before its voice finished");

    assert!(!visible(&run.tree("bar", "sent"), "bar"));
    assert!(run.path("args").exists());
}

#[test]
fn a_stopped_voices_late_failure_leaves_the_next_voice_running() {
    let run = Run::new("voice-late-failure.json", ONE_SESSION);
    let second = run.tree("bar", "second-voice");
    assert_eq!(text(&second, "voice"), Some("Listening · right ⌥ to stop"));

    let later = run.tree("bar", "after-late-failure");
    assert_eq!(text(&later, "voice"), Some("Listening · right ⌥ to stop"));
}

#[test]
fn send_right_after_voice_stops_waits_for_its_last_sentence() {
    let run = Run::new("voice-send.json", ONE_SESSION);

    // A voice tap stopped voice and Send came while it finished: the send's still is
    // taken, but nothing goes to attn until the last sentence is in.
    let waiting = run.tree("bar", "still-while-finishing");
    assert_eq!(text(&waiting, "voice"), Some("Finishing…"));
    assert_eq!(text(&waiting, "segment-0"), None);
    assert!(!run.path("args").exists(), "sent before voice finished");

    let finished = run.tree("bar", "voice-finished");
    assert_eq!(text(&finished, "segment-0"), Some("and then it crashes"));
    assert_eq!(text(&finished, "voice"), Some("right ⌥ voice"));
    assert!(!run.path("args").exists(), "the send runs after the finish");

    assert!(!visible(&run.tree("bar", "sent"), "bar"));
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert!(args.starts_with("send s1 "), "{args}");

    // A finish that fails stops the send, which would lose the sentence, and says why.
    let failed = run.tree("bar", "send-failed");
    assert!(visible(&failed, "bar"));
    assert_eq!(
        text(&failed, "status"),
        Some("Voice stopped: transcription failed")
    );
}

/// Whether a run of identical pixels shows, which the fake's gradient never has.
fn is_pixelated(path: &Path) -> bool {
    let frame = image::open(path).unwrap().to_rgba8();
    frame.rows().any(|row| {
        row.collect::<Vec<_>>()
            .windows(6)
            .any(|run| run.iter().all(|p| *p == run[0]))
    })
}

fn state(run: &Run, mark: &str) -> Value {
    let tree = run.tree("ink", mark);
    serde_json::from_str(label(&tree, "capture-state").expect("no capture-state element")).unwrap()
}

#[test]
fn an_edited_still_round_trips_and_is_what_is_sent() {
    let run = Run::new("roundtrip.json", ONE_SESSION);
    let drawn = capture_state(&run, "drawn");
    assert_eq!(drawn["stills"][0]["marks"], 1);

    // The still opens in the editor; the live surfaces wait.
    let editing = run.tree("editor", "editing");
    assert!(visible(&editing, "editor"));
    assert!(!visible(&editing, "bar"));
    assert!(!visible(&editing, "ink"));
    assert_eq!(state(&run, "editing")["editing"], 0);

    // Back returns to the same capture, with the edit kept and editable.
    let back = state(&run, "back");
    assert_eq!(back["editing"], Value::Null);
    assert_eq!(back["capture"]["id"], drawn["id"]);
    assert_eq!(back["capture"]["stills"][0]["marks"], 2);
    // The ink mark keeps its time; the editor's has none.
    assert_eq!(
        back["capture"]["stills"][0]["times"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let returned = run.tree("bar", "back");
    assert!(visible(&returned, "bar"));
    assert!(!visible(&returned, "editor"));

    // Reopening brings back the marks and the undo history.
    assert!(visible(&run.tree("editor", "reopened"), "editor"));
    assert_eq!(state(&run, "undone")["capture"]["stills"][0]["marks"], 1);
    assert_eq!(
        state(&run, "back-again")["capture"]["stills"][0]["marks"],
        2
    );

    // Send delivers the edited document.
    assert_eq!(state(&run, "sent")["tile"]["sent"], true);
    assert!(
        is_pixelated(&run.path("frame-01.png")),
        "the sent frame is raw"
    );

    // The tile holds for 5 s after Send, then leaves.
    assert!(visible(&run.tree("tile", "tile-holds"), "tile"));
    let left = run.tree("tile", "tile-left");
    assert!(!visible(&left, "tile"));
    assert_eq!(state(&run, "tile-left")["tile"], Value::Null);
}

#[test]
fn a_dropped_still_is_not_sent_and_the_other_is_pixelated() {
    let run = Run::new("roundtrip-secret.json", ONE_SESSION);
    assert_eq!(
        capture_state(&run, "two-stills")["stills"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        capture_state(&run, "dropped")["stills"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    run.tree("bar", "sent");
    assert!(
        is_pixelated(&run.path("frame-01.png")),
        "the kept still is raw"
    );
    assert!(
        !run.path("frame-02.png").exists(),
        "the dropped still was sent"
    );
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert_eq!(args.matches(".png").count(), 1, "{args}");
}

#[test]
fn send_works_from_inside_the_editor() {
    let run = Run::new("roundtrip-send.json", ONE_SESSION);
    assert!(visible(&run.tree("editor", "pixelated"), "editor"));
    let sent = run.tree("editor", "sent");
    assert!(!visible(&sent, "editor"));
    assert_eq!(state(&run, "sent")["send"]["state"], "sent");
    assert!(
        is_pixelated(&run.path("frame-01.png")),
        "the sent frame is raw"
    );
}

#[test]
fn every_way_a_capture_ends_gives_the_keyboard_back_to_the_app_the_user_was_in() {
    let run = Run::new("focus-returns.json", ONE_SESSION);
    // Each capture starts with another app in front, then ends a different way.
    let ends = [
        ("bar-sent", "the bar's ↩"),
        ("hotkey-sent", "⌘⌥↩"),
        ("escaped", "Esc in the bar"),
        ("pick-cancelled", "Esc in the picker"),
        ("editor-sent", "Send in the editor"),
    ];
    let apps = ["Terminal", "Browser", "Terminal", "Browser", "Browser"];
    for (index, (mark, way)) in ends.iter().enumerate() {
        assert_eq!(
            run.focus_returned(mark),
            apps[..=index],
            "after {way}, the keyboard went back to each app in turn"
        );
    }
    // The user switched apps before sending: Lens leaves their choice alone.
    assert_eq!(run.focus_returned("moved-on"), apps);
}

#[test]
fn an_escaped_capture_keeps_its_tile_and_reopens() {
    let run = Run::new("roundtrip-esc.json", ONE_SESSION);
    let escaped = state(&run, "escaped");
    assert_eq!(escaped["capture"], Value::Null);
    assert_eq!(escaped["tile"]["sent"], false);
    assert_eq!(escaped["tile"]["leaving"], false);
    assert!(visible(&run.tree("tile", "still-there"), "tile"));
    let reopened = state(&run, "reopened");
    assert_eq!(reopened["tile"], Value::Null);
    assert_eq!(reopened["capture"]["stills"].as_array().unwrap().len(), 2);
    assert!(visible(&run.tree("bar", "reopened"), "bar"));
}

#[test]
fn closing_the_editor_during_an_operation_keeps_the_result() {
    let run = Run::new("roundtrip-close-busy.json", ONE_SESSION);
    // Waits for the rotate instead of racing it.
    assert!(state(&run, "closed")["editing"].is_number());
    assert_eq!(state(&run, "done")["editing"], Value::Null);
    assert!(visible(&run.tree("bar", "done"), "bar"));
    assert_eq!(state(&run, "sent")["send"]["state"], "sent");
    let frame = image::open(run.path("frame-01.png")).unwrap();
    assert!(
        frame.height() > frame.width(),
        "the sent frame lost the rotate"
    );
    // A Dock click afterwards shows no capture's frame.
    let dock = run.tree("editor", "dock");
    assert!(visible(&dock, "editor"));
    assert!(
        dock.to_string().contains("editor-empty"),
        "the editor still holds the frame"
    );
}

#[test]
fn closing_the_editor_while_a_still_loads_gives_the_capture_back() {
    let run = Run::new("roundtrip-close-load.json", ONE_SESSION);
    assert_eq!(state(&run, "done")["editing"], Value::Null);
    assert!(visible(&run.tree("bar", "done"), "bar"));
}

#[test]
fn send_from_the_editor_adds_no_fresh_still() {
    let run = Run::new("roundtrip-send-no-fresh.json", ONE_SESSION);
    assert_eq!(state(&run, "sent")["send"]["state"], "sent");
    let args = std::fs::read_to_string(run.path("args")).unwrap();
    assert_eq!(args.matches(".png").count(), 1, "{args}");
}

#[test]
fn an_edit_returning_from_a_closed_capture_leaves_the_new_one_alone() {
    let run = Run::new("roundtrip-old-edit.json", ONE_SESSION);
    assert_eq!(state(&run, "sent")["send"]["state"], "sent");
    let frame = image::open(run.path("frame-01.png")).unwrap();
    assert!(frame.width() > frame.height(), "the old rotate landed here");
}

#[test]
fn edit_opens_the_newest_still_when_an_older_one_is_on_its_way() {
    let run = Run::new("roundtrip-edit-newest-landed.json", ONE_SESSION);
    assert!(state(&run, "editing")["editing"].is_number());
}

#[test]
fn a_closing_capture_cancels_a_send_queued_in_the_editor() {
    let run = Run::new("roundtrip-close-cancels-send.json", ONE_SESSION);
    assert_eq!(state(&run, "done")["send"]["state"], "idle");
    assert!(!run.path("args").exists(), "a frame was sent");
}

#[test]
fn an_earlier_voice_finishing_leaves_a_reopened_captures_new_voice() {
    let run = Run::new("roundtrip-voice-reopen.json", ONE_SESSION);
    // The earlier voice holds the helper until it finishes; a voice tap waits for it.
    assert_eq!(
        state(&run, "still-finishing")["voice"]["state"],
        "finishing"
    );
    assert_eq!(state(&run, "done")["voice"]["state"], "listening");
}

#[test]
fn a_tile_keeps_the_last_sentence_of_a_voice_closed_by_esc() {
    let run = Run::new("roundtrip-voice-esc.json", ONE_SESSION);
    // Reopened before the helper finished: Send would wait for it.
    assert_eq!(state(&run, "reopened")["voice"]["state"], "finishing");
    let done = state(&run, "done");
    assert_eq!(done["voice"]["state"], "off");
    assert_eq!(done["capture"]["segments"][0]["text"], "final words");
}

#[test]
fn an_older_still_failing_leaves_a_waiting_edit_alone() {
    let run = Run::new("roundtrip-edit-older-fails.json", ONE_SESSION);
    assert!(state(&run, "editing")["editing"].is_number());
}

#[test]
fn esc_during_a_send_that_finishes_voice_leaves_no_stuck_voice() {
    let run = Run::new("roundtrip-send-esc-voice.json", ONE_SESSION);
    assert_eq!(state(&run, "reopened")["voice"]["state"], "off");
}

#[test]
fn bar_keys_work_with_the_field_not_focused() {
    let run = Run::new("bar-keys-off-field.json", ONE_SESSION);
    assert!(element(&run.tree("bar", "palette"), "palette-label-0").is_some());
    assert!(element(&run.tree("bar", "palette-closed"), "palette-label-0").is_none());
    assert_eq!(state(&run, "escaped")["capture"], Value::Null);
}

/// The dictation as the automation state reports it.
fn dictation(run: &Run, mark: &str) -> Value {
    state(run, mark)["dictation"].clone()
}

#[test]
fn a_dictation_shows_its_words_then_pastes_attns_refinement_once() {
    let run = Run::new("dictation.json", ONE_SESSION);

    // The pill shows at once, then the words as they come.
    let pill = run.tree("pill", "starting");
    assert!(visible(&pill, "pill"));
    assert_eq!(text(&pill, "pill-status"), Some("Starting voice…"));
    let pill = run.tree("pill", "recording");
    assert_eq!(text(&pill, "pill-words"), Some("um so deploy to staging"));
    assert_eq!(
        text(&pill, "pill-status"),
        Some("Listening · right ⌥ to stop")
    );
    assert!(run.pasted("recording").is_empty());

    // Voice stops; attn gets the token, then one request, and the text pastes once.
    let refined = dictation(&run, "refined");
    assert_eq!(refined["state"], "settled");
    assert_eq!(refined["note"], "Pasted");
    assert_eq!(run.pasted("refined"), ["Refined: um so deploy to staging"]);
    assert_eq!(
        std::fs::read_to_string(run.path("model-args"))
            .unwrap()
            .trim(),
        "model call --token-stdin"
    );
    let stdin = std::fs::read_to_string(run.path("model-stdin")).unwrap();
    let (token, request) = stdin.split_once('\n').unwrap();
    assert_eq!(token, "fake-token");
    // 0.3 s recorded: 15 s plus a quarter of it. No harness: the user's choice in attn.
    assert_eq!(
        serde_json::from_str::<Value>(request).unwrap(),
        json!({
            "purpose": "dictation-refine",
            "tier": "standard",
            "system": attn_lens::lens::refiner::instructions(),
            "input": r#"{"transcript":"um so deploy to staging"}"#,
            "limit_ms": 15_075,
        })
    );
    assert_eq!(
        text(&run.tree("pill", "refined"), "pill-status"),
        Some("Pasted")
    );
    assert!(!visible(&run.tree("pill", "gone"), "pill"));

    // A hold dictates until release.
    assert_eq!(dictation(&run, "held")["note"], "Pasted");
    assert_eq!(
        run.pasted("held"),
        ["Refined: um so deploy to staging", "Refined: held words"]
    );
    // The Dock menu copies the last dictation.
    assert_eq!(
        dictation(&run, "copy-last")["note"],
        "Copied the last dictation"
    );
    assert_eq!(
        run.copied("copy-last").as_deref(),
        Some("Refined: held words")
    );

    // Without Accessibility the text goes to the clipboard, and the pill says why.
    let copied = dictation(&run, "copied");
    assert_eq!(copied["note"], "Copied — allow Accessibility to paste");
    assert_eq!(run.pasted("copied").len(), 2);
    assert_eq!(
        run.copied("copied").as_deref(),
        Some("Refined: copied words")
    );
}

#[test]
fn a_dictation_waiting_for_attn_pastes_raw_on_enter_nothing_on_esc_and_raw_past_its_deadline() {
    let run = Run::new("dictation-refining.json", ONE_SESSION);

    // ↩ while recording is the app's; while attn refines, it is the pill's.
    let refining = dictation(&run, "refining");
    assert_eq!(refining["state"], "refining");
    assert_eq!(
        text(&run.tree("pill", "refining"), "pill-status"),
        Some("Refining… · ↩ paste as is · esc cancel")
    );
    assert_eq!(dictation(&run, "entered")["note"], "Pasted as is");
    assert_eq!(run.pasted("entered"), ["raw words"]);
    // attn's answer after ↩ is dropped.
    assert_eq!(dictation(&run, "late")["note"], "Pasted as is");
    assert_eq!(run.pasted("late"), ["raw words"]);

    // Esc pastes nothing, and keeps the words for Copy Last Dictation.
    let escaped = state(&run, "escaped");
    assert_eq!(escaped["dictation"]["note"], "Cancelled · nothing pasted");
    assert_eq!(escaped["last_dictation"], "never mind");
    assert_eq!(run.pasted("escaped"), ["raw words"]);

    // The call's limit for 0.2 s of speech is 15.05 s; Lens waits 2 s more.
    assert_eq!(dictation(&run, "waiting")["state"], "refining");
    let expired = dictation(&run, "expired");
    assert_eq!(
        expired["note"],
        "Not refined: attn did not answer within 17 s · pasted as is"
    );
    assert_eq!(dictation(&run, "deadline-pasted")["state"], "settled");
    assert_eq!(run.pasted("deadline-pasted"), ["raw words", "too slow"]);

    // Calls that do not answer: ↩, Esc and the deadline each stop theirs.
    std::fs::write(run.path("model-hang"), "").unwrap();
    assert_eq!(dictation(&run, "deadline-pasted")["state"], "settled");
    assert!(
        !run.path("model-survived").exists(),
        "a call ran on after its dictation settled"
    );
}

#[test]
fn an_empty_dictation_pastes_nothing_and_a_failed_refinement_pastes_raw_with_attns_error() {
    let run = Run::new("dictation-fallbacks.json", ONE_SESSION);
    assert_eq!(dictation(&run, "empty")["note"], "Didn't catch anything");
    assert!(run.pasted("empty").is_empty());
    assert!(
        !run.path("model-args").exists(),
        "attn was asked about nothing"
    );

    // attn ran the call and it failed.
    std::fs::write(
        run.path("model-out"),
        r#"{"harness":"claude","model":"sonnet","error":"claude: exited 1"}"#,
    )
    .unwrap();
    std::fs::write(run.path("model-code"), "1").unwrap();
    let failed = state(&run, "settled");
    assert_eq!(
        failed["dictation"]["note"],
        "Not refined: claude: exited 1 · pasted as is"
    );
    assert_eq!(run.pasted("settled"), ["raw words"]);

    // attn no longer knows the token: Lens is unpaired, and the raw text pastes.
    std::fs::remove_file(run.path("model-out")).unwrap();
    std::fs::write(
        run.path("model-err"),
        "model call: the sender token matches no paired sender; it was unpaired or never allowed. Pair again with attn send pair --name <name>",
    )
    .unwrap();
    let unpaired = state(&run, "settled");
    assert!(
        unpaired["dictation"]["note"]
            .as_str()
            .unwrap()
            .starts_with("Not refined: model call: the sender token matches no paired sender"),
        "{unpaired}"
    );
    assert!(unpaired["attn"].as_str().unwrap().starts_with("Unpaired"));
    assert_eq!(run.pasted("settled"), ["raw words"]);

    // No attn to run.
    std::fs::set_permissions(run.path("attn"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let missing = dictation(&run, "settled");
    let note = missing["note"].as_str().unwrap();
    assert!(note.starts_with("Not refined: Could not run "), "{note}");
    assert!(note.ends_with(" · pasted as is"), "{note}");
    assert_eq!(run.pasted("settled"), ["raw words"]);
}

#[test]
fn the_capture_key_during_a_dictation_hands_its_voice_and_words_to_the_capture() {
    let run = Run::new("dictation-handover.json", ONE_SESSION);
    // The dictation runs on while the area is picked.
    assert_eq!(dictation(&run, "picking")["state"], "recording");

    let open = state(&run, "open");
    assert_eq!(open["dictation"], Value::Null);
    assert_eq!(open["capture"]["segments"][0]["text"], "look at this");
    // The capture's timeline starts where the talk did, at the voice key's frame.
    let started = open["capture"]["started"].as_u64().unwrap();
    assert!((100_000_000..120_000_000).contains(&started), "{started}");
    assert_eq!(open["voice"]["state"], "listening");
    assert!(!visible(&run.tree("pill", "open"), "pill"));

    let stopped = state(&run, "stopped");
    assert_eq!(stopped["capture"]["segments"][1]["text"], "and this");
    assert_eq!(stopped["voice"]["state"], "off");
    assert!(run.pasted("stopped").is_empty());
    assert!(
        !run.path("model-args").exists(),
        "a handed-over dictation was refined"
    );
}

#[test]
fn the_area_picker_takes_enter_and_esc_from_a_dictation_attn_is_refining() {
    let run = Run::new("dictation-picker.json", ONE_SESSION);
    let picking = state(&run, "picking");
    assert_eq!(picking["dictation"]["state"], "refining");
    assert!(picking["picking"].is_array());

    // While the picker shows, ↩ is the picker's, not the dictation's.
    let cancelled = state(&run, "cancelled");
    assert_eq!(cancelled["picking"], Value::Null);
    assert_eq!(cancelled["dictation"]["state"], "refining");
    assert_eq!(run.app_keys("cancelled"), ["key:enter"]);

    // A cancelled pick gives the waiting dictation its keys back.
    assert_eq!(
        dictation(&run, "escaped")["note"],
        "Cancelled · nothing pasted"
    );
    assert_eq!(run.app_keys("escaped"), ["key:enter"]);
    assert!(run.pasted("after").is_empty());
}

#[test]
fn a_capture_taking_a_dictation_attn_is_refining_gives_enter_back_to_the_app() {
    let run = Run::new("dictation-handover-refining.json", ONE_SESSION);
    assert_eq!(dictation(&run, "refining")["state"], "refining");

    let open = state(&run, "open");
    assert_eq!(open["dictation"], Value::Null);
    assert_eq!(open["capture"]["segments"][0]["text"], "look at this");

    // Lens stops taking ↩ with the hand-over, so it reaches the app in front.
    assert_eq!(dictation(&run, "after"), Value::Null);
    assert_eq!(run.app_keys("after"), ["key:enter"]);
    // The refinement still waiting when the capture took over is dropped unpasted.
    assert!(run.pasted("after").is_empty());
}

#[test]
fn the_bar_drags_out_of_the_way_and_stays_on_its_display() {
    let run = Run::new("bar-drag.json", ONE_SESSION);
    let bar = |mark: &str| frame(&run.tree("bar", mark), "bar");

    // The area is too tall for the bar above or below it, so it starts inside the
    // bottom edge, over the area.
    // The handles stand above the bar, so the corners it covers hide.
    let open = run.tree("bar", "open");
    assert_eq!(frame(&open, "bar"), [100., 708., 760., 84.]);
    assert!(!visible(&open, "handle-bottom-left"));
    assert!(!visible(&open, "handle-bottom-right"));
    assert!(visible(&open, "handle-top-left"));

    // A drag on its background moves it with the pointer. Nothing is drawn, no still
    // is taken, and the field keeps the keys.
    let dragged = run.tree("bar", "dragged");
    assert_eq!(frame(&dragged, "bar"), [100., 400., 760., 84.]);
    assert!(visible(&dragged, "handle-bottom-left"));
    assert_eq!(text(&dragged, "keys"), Some("Typing here"));
    let capture = capture_state(&run, "dragged");
    assert_eq!(capture["stills"].as_array().unwrap().len(), 0);
    assert_eq!(capture["ink"].as_array().unwrap().len(), 0);
    assert_eq!(
        text(&run.tree("bar", "typed-after-drag"), "message"),
        Some("hi!")
    );

    // Ink lands where the bar was (fn+2 picked Arrow), and the bar stays where it was dragged.
    let drawn = run.tree("ink", "drawn-under");
    assert_eq!(label(&drawn, "mark-0"), Some("arrow"));
    assert_eq!(frame(&drawn, "bar"), [100., 400., 760., 84.]);

    // Dragged past the display's corner, it stops whole on the display. The press
    // took the keys from the page, and the drag gives them back.
    assert_eq!(
        text(&run.tree("bar", "page-keys"), "keys"),
        Some("Typing on the page")
    );
    assert_eq!(run.focus_returned("at-edge"), ["released"]);
    let edge = run.tree("bar", "at-edge");
    assert_eq!(frame(&edge, "bar"), [512., 708., 760., 84.]);
    assert_eq!(text(&edge, "keys"), Some("Typing on the page"));

    // A click into the field gives the bar the keys on purpose: a drag after it keeps
    // them. Rune cannot take focus from a headless window, so the script blurs the bar
    // after the hand-back above, as ordering the panel out does on the Mac.
    assert_eq!(run.focus_returned("field-then-drag"), ["released"]);
    assert_eq!(
        text(&run.tree("bar", "field-then-drag"), "keys"),
        Some("Typing here")
    );

    // A click in the field's padding, inside its border, takes the keys for the field:
    // it neither drags the bar nor hands them back.
    assert_eq!(run.focus_returned("field-padding"), ["released"]);
    let padding = run.tree("bar", "field-padding");
    assert_eq!(text(&padding, "keys"), Some("Typing here"));
    assert_eq!(frame(&padding, "bar"), [512., 708., 760., 84.]);

    // The palette grows the bar, which rises to stay on the display.
    let palette = run.tree("bar", "palette-at-edge");
    assert!(element(&palette, "palette-label-0").is_some());
    let [x, y, _, height] = frame(&palette, "bar")[..] else {
        unreachable!()
    };
    assert_eq!((x, y + height), (512., 792.));

    // The next capture, on the second display, starts at the default place, and a
    // drag toward the first display stops at the second's edge.
    assert_eq!(bar("display-2-open"), [1300., 676., 760., 84.]);
    assert_eq!(capture_state(&run, "display-2-open")["bar_at"], Value::Null);
    assert_eq!(bar("display-2-dragged"), [1288., 368., 760., 84.]);

    // Voice shows its words in the bar, which stays where it was dragged.
    let voice = run.tree("bar", "voice-dragged");
    assert!(text(&voice, "segment-0").is_some_and(|words| words.starts_with("move this")));
    assert_eq!(frame(&voice, "bar"), [1288., 368., 760., 84.]);

    // A whole display puts the bar over the bottom-left corner, whose handle hides. A
    // resize that moves the bar above the area shows it again as the resize lands.
    let whole = run.tree("bar", "whole-display-1");
    assert!(!visible(&whole, "handle-bottom-left"));
    let resized = run.tree("bar", "resize-uncovered");
    assert_eq!(frame(&resized, "bar")[1], 308.);
    assert!(visible(&resized, "handle-bottom-left"));
    has_area_frame(&run.tree("ink", "resize-uncovered"));
}

/// The states the accessibility tree gives the element `key`, such as "checked".
fn states(tree: &Value, key: &str) -> Vec<String> {
    fn find<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
        if node["key"] == key {
            return Some(node);
        }
        node["children"]
            .as_array()?
            .iter()
            .find_map(|child| find(child, key))
    }
    let node = find(&tree["accessibility"], key).unwrap_or_else(|| panic!("no {key}"));
    node["states"]
        .as_array()
        .map(|s| s.iter().map(|v| v.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn screen_words_on(run: &Run, mark: &str) -> bool {
    let states = states(&run.tree("settings", mark), "screen-words-control");
    assert!(
        states.iter().any(|s| s == "checked" || s == "unchecked"),
        "{states:?}"
    );
    states.iter().any(|s| s == "checked")
}

fn saved_screen_words(run: &Run) -> Value {
    let prefs = run.path("prefs/dev.attn.lens/prefs.json");
    serde_json::from_slice::<Value>(&std::fs::read(prefs).unwrap()).unwrap()["dictation.screen_words"]
        .clone()
}

#[test]
fn the_settings_window_saves_use_words_on_screen_across_a_relaunch() {
    // Each read runs the script again on this run's prefs.json, so each is a relaunch,
    // and each that reaches "flipped" has clicked the switch once more.
    let run = Run::new("settings.json", ONE_SESSION);

    // Hidden until Settings… opens it, with the switch on when nothing is saved.
    assert!(!visible(&run.tree("settings", "launched"), "settings"));
    let opened = run.tree("settings", "opened");
    assert!(visible(&opened, "settings"));
    assert_eq!(text(&opened, "dictation-title"), Some("Dictation"));
    assert_eq!(
        text(&opened, "screen-words-label"),
        Some("Use words on screen")
    );
    assert!(screen_words_on(&run, "opened"));
    assert_eq!(state(&run, "opened")["settings"]["screen_words"], true);

    // A click turns it off and saves at once.
    assert!(!screen_words_on(&run, "flipped"));
    assert_eq!(saved_screen_words(&run), false);

    // Relaunched, Lens reads it off, and the window shows it.
    assert_eq!(state(&run, "opened")["settings"]["screen_words"], false);
    assert!(!screen_words_on(&run, "opened"));

    // A click turns it back on, which Lens reports and saves.
    let flipped = state(&run, "flipped");
    assert_eq!(flipped["settings"]["screen_words"], true);
    assert_eq!(flipped["settings_error"], Value::Null);
    assert_eq!(saved_screen_words(&run), true);
    assert!(screen_words_on(&run, "opened"));
}

#[test]
fn closing_the_settings_window_hides_it_and_the_dock_menu_shows_it_again() {
    let run = Run::new("settings.json", ONE_SESSION);
    // The click before the close turned it off; reopened, the window still shows that.
    let reopened = run.tree("settings", "reopened");
    assert!(visible(&reopened, "settings"));
    assert!(states(&reopened, "screen-words-control").contains(&"unchecked".to_string()));
    assert!(!visible(&run.tree("settings", "closed"), "settings"));
}

#[test]
fn a_setting_that_cannot_be_saved_holds_for_the_run_and_the_window_says_why() {
    let run = Run::new("settings.json", ONE_SESSION);
    // A file where the settings folder should be, so the save cannot make it.
    std::fs::write(run.path("prefs"), "").unwrap();
    let flipped = run.tree("settings", "flipped");
    assert!(states(&flipped, "screen-words-control").contains(&"unchecked".to_string()));
    let error = text(&flipped, "settings-error").expect("no error shown");
    assert!(error.starts_with("Not saved: "), "{error}");
    let state = state(&run, "flipped");
    assert_eq!(state["settings"]["screen_words"], false);
    assert!(state["settings_error"].is_string());
}

/// A scene's tree, rendered as `scripts/snap.sh` does: the stand-in `attn` of
/// `tools/scenes`, on a 1440×900 desktop.
fn scene_tree(window: &str, scene: &str, theme: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let root = env!("CARGO_MANIFEST_DIR");
    let out = dir.path().join("tree.json");
    let run = Command::new(env!("CARGO_BIN_EXE_attn-lens"))
        .env("ATTN_BIN", format!("{root}/tools/scenes/attn"))
        .env("RUNE_PREFS_DIR", dir.path())
        .args(["--window", window, "--scene", scene, "--theme", theme])
        .args([
            "--desktop",
            &format!("{root}/tools/scenes/desktop-light.png"),
        ])
        .args(["--size", "1440x900", "--scale", "1", "--at", "1", "--tree"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap()
}

/// WCAG's contrast ratio between two `#rrggbbaa` colours, both opaque.
fn contrast(a: &Value, b: &Value) -> f64 {
    let luminance = |hex: &Value| {
        let hex = hex.as_str().unwrap().trim_start_matches('#');
        let channel = |i: usize| {
            let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn the_capture_hud_is_dark_in_every_appearance_and_its_parts_stay_apart() {
    let paint = |key: &str, tree: &Value| element(tree, key).unwrap()["fill"].clone();
    let fill = |tree: &Value| paint("bar", tree);

    // The HUD paints the same whatever the system appearance (D8).
    let (light, dark) = (
        scene_tree("bar", "open", "light"),
        scene_tree("bar", "open", "dark"),
    );
    for key in ["bar", "chip", "field", "edit", "send"] {
        assert_eq!(paint(key, &light), paint(key, &dark), "{key}");
    }
    assert_eq!(
        paint("tile", &scene_tree("tile", "sent", "light")),
        paint("tile", &scene_tree("tile", "sent", "dark"))
    );
    assert_eq!(
        paint("pill", &scene_tree("pill", "recording", "light")),
        paint("pill", &scene_tree("pill", "recording", "dark"))
    );

    // Send stands out from the bar, and the chip ends before the field starts.
    assert!(contrast(&paint("send", &light), &fill(&light)) >= 3.);
    let long = scene_tree("bar", "long-label", "light");
    for tree in [&light, &long] {
        assert!(rect(tree, "chip")[0] + rect(tree, "chip")[2] <= rect(tree, "field")[0]);
        inside(tree, "chip-target", "chip");
        inside(tree, "chip-host", "chip");
    }

    // Every palette row stays inside the bar.
    let palette = scene_tree("bar", "palette", "light");
    for index in 0..8 {
        inside(&palette, &format!("palette-row-{index}"), "bar");
    }

    // Voice's progress ends before Edit starts.
    let downloading = scene_tree("bar", "voice-downloading", "light");
    let voice = rect(&downloading, "voice");
    assert!(voice[0] + voice[2] <= rect(&downloading, "edit")[0]);

    // An error reads against the bar.
    let failed = scene_tree("bar", "voice-failed", "light");
    let error = element(&failed, "error").expect("the failure shows");
    assert!(contrast(&error["color"], &fill(&failed)) >= 4.5);
}

/// Each `attn model call` input the stand-in got, in order, as JSON.
fn model_inputs(run: &Run) -> Vec<Value> {
    std::fs::read_to_string(run.path("model-requests"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let request: Value = serde_json::from_str(line).unwrap();
            serde_json::from_str(request["input"].as_str().unwrap()).unwrap()
        })
        .collect()
}

/// The fake's lines in `name` beside the clipboard, by the time of `mark`.
fn fake_lines(run: &Run, mark: &str, name: &str) -> Vec<Value> {
    std::fs::read_to_string(run.clipboard(mark).join(name))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn a_dictation_sends_only_the_screen_words_it_said_and_never_waits_for_the_read() {
    let run = Run::new("dictation-screen-words.json", ONE_SESSION);
    assert_eq!(
        state(&run, "end")["capture"]["segments"][0]["text"],
        "rename scope test environment"
    );
    let said = "rename scope test environment";
    let plain = json!({ "transcript": said });
    assert_eq!(
        model_inputs(&run),
        [
            // Of the screen's identifiers, only the one said; "skill" is on screen
            // twice and sounds like "scale".
            json!({
                "transcript": "rename scope test environment and load the scale",
                "screen_words": ["ScopeTestEnvironment"],
                "maybe_misheard": [{ "heard": "scale", "screen": "skill" }],
            }),
            // Nothing on screen was said, and nothing sounds like it.
            json!({ "transcript": "deploy to staging now" }),
            // The read was held past refinement.
            plain.clone(),
            // The held read came back during the next dictation, whose own read found
            // nothing worth spelling: the late one's words did not reach it.
            plain.clone(),
            // The read failed.
            plain.clone(),
            // Screen Recording is not allowed.
            plain.clone(),
            // "Use words on screen" is off.
            plain,
            // The hand-over to a capture asked for nothing.
        ]
    );
    let reads: Vec<String> = fake_lines(&run, "end", "screen-reads.jsonl")
        .iter()
        .map(|read| match &read["text"] {
            Value::String(text) => text.chars().take(12).collect(),
            _ => read["error"].as_str().unwrap().to_string(),
        })
        .collect();
    // One read per dictation with the setting on, the hand-over's included; none
    // while it was off.
    assert_eq!(
        reads,
        [
            "Rename Scope",
            "Rename Scope",
            "Nothing wort",
            "Rename Scope",
            "The fake screen read failed.",
            "Screen Recording is not allowed for Attn Lens.",
            "Rename Scope",
        ]
    );
    // A missing permission or a failed read shows no guide.
    assert!(fake_lines(&run, "end", "guides.jsonl").is_empty());
    assert_eq!(run.pasted("end").len(), 7);
    assert!(state(&run, "switched-on")["settings"]["screen_words"] == true);
}

#[test]
fn a_dictation_reads_each_window_that_comes_in_front_once_and_never_waits_for_one() {
    let run = Run::new("dictation-screen-words-switch.json", ONE_SESSION);
    state(&run, "end");
    assert_eq!(
        model_inputs(&run),
        [
            // Started over window main and moved to b: identifiers from both.
            json!({
                "transcript": "rename scope test environment and call fetch release notes",
                "screen_words": ["ScopeTestEnvironment", "fetch_release_notes"],
            }),
            // main, b, main, b: each read once.
            json!({
                "transcript": "rename scope test environment",
                "screen_words": ["ScopeTestEnvironment"],
            }),
            // b's read was still out when refinement started: main's words alone.
            json!({
                "transcript": "call fetch release notes and rename scope test environment",
                "screen_words": ["ScopeTestEnvironment"],
            }),
        ]
    );
    // Each read of the fake's, by window, up to `mark`.
    let reads = |mark| -> Vec<String> {
        state(&run, mark);
        fake_lines(&run, mark, "screen-reads.jsonl")
            .iter()
            .map(|read| read["window"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(reads("revisited"), ["main", "b", "main", "b"]);
    // The paste went ahead with b's read held; b was read once it was let go.
    assert_eq!(reads("pasted"), ["main", "b", "main", "b", "main"]);
    assert_eq!(run.pasted("pasted").len(), 3);
    assert_eq!(reads("late"), ["main", "b", "main", "b", "main", "b"]);
    let out = run.path("bar-late.json");
    let logged = run
        .command("late")
        .env("LENS_DICTATION_LOG", "1")
        .args(["--window", "bar", "--tree"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(logged.status.success());
    let log = String::from_utf8_lossy(&logged.stderr);
    assert!(
        log.contains("screen read 1 came back after the dictation moved on, dropped"),
        "{log}"
    );
    assert!(
        log.contains("screen read 1 not back, refining without it"),
        "{log}"
    );
    // Times and counts, never the words.
    assert!(!log.contains("ScopeTestEnvironment") && !log.contains("fetch_release_notes"));
}

#[test]
fn the_settings_row_says_when_screen_recording_is_not_allowed() {
    let note = "Screen Recording is not allowed for Attn Lens, so dictation does not read the \
                screen. Allow it in System Settings › Privacy & Security.";
    let denied = scene_tree("settings", "dictation-no-screen-recording", "dark");
    assert_eq!(text(&denied, "screen-words-note"), Some(note));
    assert!(
        element(
            &scene_tree("settings", "dictation", "dark"),
            "screen-words-note"
        )
        .is_none()
    );
    // Off, the switch reads nothing, so nothing is missing.
    assert!(
        element(
            &scene_tree("settings", "dictation-off", "dark"),
            "screen-words-note"
        )
        .is_none()
    );
}
