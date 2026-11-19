//! The automation socket, with the fake port behind it and a stand-in `attn`.
use attn_lens::{
    lens::{Lens, automation},
    platform_port::fake::FakePort,
};
use rune::prefs::Prefs;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

struct Harness {
    socket: PathBuf,
}

impl Harness {
    fn new(dir: &Path, token: Option<&str>) -> Self {
        let lens = Lens::new(Arc::new(FakePort::new(vec![], token)), Prefs::memory());
        let socket = dir.join(format!("lens-{}.sock", token.unwrap_or("unpaired")));
        automation::listen(lens, &socket).unwrap();
        Self { socket }
    }
    fn ask(&self, request: &str) -> Value {
        serde_json::from_str(&automation::request(&self.socket, request).unwrap()).unwrap()
    }
    fn state(&self) -> Value {
        self.ask(r#"{"cmd":"state"}"#)["state"].clone()
    }
}

/// A stand-in that records its arguments and the PNGs as they were when sent.
fn stand_in(dir: &Path) -> PathBuf {
    let bin = dir.join("attn");
    attn_lens::stand_in::write_executable(
        &bin,
        &format!(
            r#"#!/bin/sh
cat > /dev/null
if [ "$2" = "sessions" ]; then echo '[{{"id":"s1","label":"one","agent":"claude","state":"idle"}}]'; exit 0; fi
if [ "$2" = "closed" ]; then echo 'send: session closed is closed' >&2; exit 1; fi
if [ "$2" = "gated" ]; then echo entered > "{dir}/entered"; cat "{dir}/gate" > /dev/null; echo '{{"id":"cap"}}'; exit 0; fi
echo "$@" > "{dir}/args"
for arg in "$@"; do case "$arg" in *.png) cp "$arg" "{dir}/";; esac; done
echo '{{"id":"cap"}}'
"#,
            dir = dir.display()
        ),
    );
    bin
}

#[test]
fn a_capture_is_started_read_and_sent_through_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    // The only test in this binary that reads the variable.
    unsafe { std::env::set_var("ATTN_BIN", stand_in(dir)) };
    let lens = Harness::new(dir, Some("tok"));

    assert_eq!(lens.state()["capture"], Value::Null);
    assert_eq!(lens.state()["input"], "None");

    // The capture key opens the picker; a pick opens the capture.
    assert_eq!(lens.ask(r#"{"cmd":"key","key":"capture"}"#)["ok"], true);
    assert_eq!(lens.state()["picking"][0]["id"], 1);
    assert_eq!(lens.state()["capture"], Value::Null);
    assert_eq!(lens.ask(r#"{"cmd":"pick"}"#)["ok"], true);
    assert_eq!(lens.state()["picking"], Value::Null);
    assert_eq!(lens.state()["capture"]["area"]["rect"][2], 640.);
    assert_eq!(lens.ask(r#"{"cmd":"key","key":"draw-down"}"#)["ok"], true);
    assert_eq!(lens.state()["input"], "All");
    let stroke = r#"{"cmd":"stroke","points":[[10,10],[200,120]]}"#;
    assert_eq!(lens.ask(stroke)["ok"], true);
    assert_eq!(lens.state()["capture"]["stills"][0]["cause"], "draw");
    assert_eq!(lens.state()["capture"]["stills"][0]["marks"], 1);
    lens.ask(r#"{"cmd":"key","key":"draw-up"}"#);
    assert_eq!(lens.state()["input"], "None");
    assert!(
        lens.ask(stroke)["error"]
            .as_str()
            .unwrap()
            .contains("draw-down")
    );

    assert_eq!(lens.ask(r#"{"cmd":"sessions"}"#)["sessions"][0]["id"], "s1");

    // A refused send keeps the capture and attn's words.
    assert_eq!(
        lens.ask(r#"{"cmd":"send","target":"closed","message":"hi"}"#)["ok"],
        true
    );
    let failed = lens.state();
    assert_eq!(failed["send"]["state"], "failed");
    assert_eq!(failed["send"]["error"], "send: session closed is closed");
    assert_eq!(failed["capture"]["stills"][0]["cause"], "draw");

    // Sending again uses the stills it has, then closes the capture.
    lens.ask(r#"{"cmd":"key","key":"capture"}"#);
    assert_eq!(lens.state()["capture"]["stills"][1]["cause"], "hotkey");
    assert_eq!(
        lens.ask(r#"{"cmd":"send","target":"s1","message":"look here"}"#)["ok"],
        true
    );
    let sent = lens.state();
    assert_eq!(sent["send"]["state"], "sent");
    assert_eq!(sent["send"]["target"], "s1");
    assert_eq!(sent["capture"], Value::Null);
    let args = std::fs::read_to_string(dir.join("args")).unwrap();
    assert!(
        args.starts_with("send s1 --token-stdin --json --file "),
        "{args}"
    );
    assert!(args.contains("-m look here\n\nScreen capture"), "{args}");
    for frame in ["frame-01.png", "frame-02.png"] {
        let image = image::open(dir.join(frame)).unwrap();
        assert_eq!((image.width(), image.height()), (1280, 800));
    }

    // A send that finishes after its capture was replaced leaves the new capture open.
    for fifo in ["entered", "gate"] {
        let made = std::process::Command::new("mkfifo")
            .arg(dir.join(fifo))
            .status()
            .unwrap();
        assert!(made.success());
    }
    lens.ask(r#"{"cmd":"pick"}"#);
    let first = lens.state()["capture"]["id"].clone();
    let socket = lens.socket.clone();
    let send = std::thread::spawn(move || {
        automation::request(&socket, r#"{"cmd":"send","target":"gated"}"#).unwrap()
    });
    std::fs::read(dir.join("entered")).unwrap();
    // A still asked for during the send would be lost with the capture, so it is refused aloud.
    lens.ask(r#"{"cmd":"key","key":"capture"}"#);
    let sending = lens.state();
    assert_eq!(sending["capture"]["stills"].as_array().unwrap().len(), 1);
    assert_eq!(
        sending["error"],
        "Attn Lens is sending this capture, so no still was taken."
    );
    lens.ask(r#"{"cmd":"end"}"#);
    lens.ask(r#"{"cmd":"pick"}"#);
    let second = lens.state()["capture"]["id"].clone();
    assert_ne!(first, second);
    std::fs::write(dir.join("gate"), b"go").unwrap();
    send.join().unwrap();
    let after = lens.state();
    assert_eq!(after["send"]["state"], "sent");
    assert_eq!(after["capture"]["id"], second);
    lens.ask(r#"{"cmd":"end"}"#);

    // Requests that cannot work say what is missing.
    let no_capture = lens.ask(r#"{"cmd":"send","target":"s1"}"#);
    assert_eq!(no_capture["ok"], false);
    assert!(no_capture["error"].as_str().unwrap().contains("right ⌘"));
    assert!(
        lens.ask(r#"{"cmd":"nope"}"#)["error"]
            .as_str()
            .unwrap()
            .contains("state, key, pick")
    );
    assert!(
        lens.ask(r#"{"cmd":"key","key":"nope"}"#)["error"]
            .as_str()
            .unwrap()
            .contains("draw-down")
    );

    // Without a pairing, a send fails and says so.
    let unpaired = Harness::new(dir, None);
    unpaired.ask(r#"{"cmd":"pick"}"#);
    unpaired.ask(r#"{"cmd":"send","target":"s1"}"#);
    assert_eq!(
        unpaired.state()["send"]["error"],
        "Attn Lens is not paired with attn."
    );
}

/// The child of `a_live_socket_is_kept_and_a_stale_one_is_replaced`.
#[test]
#[ignore = "run by a_live_socket_is_kept_and_a_stale_one_is_replaced"]
fn bind_a_socket_and_exit() {
    if let Some(path) = std::env::var_os("LENS_TEST_SOCKET") {
        std::mem::forget(std::os::unix::net::UnixListener::bind(path).unwrap());
    }
}

#[test]
fn a_live_socket_is_kept_and_a_stale_one_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lens.sock");
    let lens = || Lens::new(Arc::new(FakePort::new(vec![], None)), Prefs::memory());

    // A socket whose process is gone. Bound in a process of its own: another test
    // forking here could keep a listener of this process alive past its drop.
    let bound = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "bind_a_socket_and_exit"])
        .env("LENS_TEST_SOCKET", &path)
        .status()
        .unwrap();
    assert!(bound.success());
    automation::listen(lens(), &path).unwrap();

    let refused = automation::listen(lens(), &path).unwrap_err();
    assert!(refused.contains("already"), "{refused}");
    let reply = automation::request(&path, r#"{"cmd":"state"}"#).unwrap();
    assert!(reply.contains(r#""ok":true"#), "{reply}");
}

#[test]
fn wait_returns_when_the_state_changes_and_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let harness = Arc::new(Harness::new(dir.path(), None));
    let at_rest = harness.ask(r#"{"cmd":"wait","since":999,"timeout_ms":0}"#)["generation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        harness.ask(&format!(
            r#"{{"cmd":"wait","since":{at_rest},"timeout_ms":0}}"#
        ))["generation"],
        at_rest,
        "nothing changed, so the timeout returns the same generation"
    );

    let waiter = {
        let harness = harness.clone();
        std::thread::spawn(move || {
            harness.ask(&format!(
                r#"{{"cmd":"wait","since":{at_rest},"timeout_ms":600000}}"#
            ))
        })
    };
    harness.ask(r#"{"cmd":"key","key":"capture"}"#);
    let reply = waiter.join().unwrap();
    assert!(reply["generation"].as_u64().unwrap() > at_rest);
    assert!(reply["state"]["picking"].is_array(), "{reply}");
    assert_eq!(
        harness.ask(r#"{"cmd":"wait","since":0}"#)["error"],
        "wait needs a whole number \"timeout_ms\""
    );
}

#[test]
fn a_pick_over_an_open_capture_closes_it_and_its_voice() {
    let dir = tempfile::tempdir().unwrap();
    let lens = Harness::new(dir.path(), None);
    lens.ask(r#"{"cmd":"key","key":"capture"}"#);
    lens.ask(r#"{"cmd":"pick"}"#);
    lens.ask(r#"{"cmd":"key","key":"voice"}"#);
    assert_eq!(lens.state()["voice"]["state"], "starting");
    let first = lens.state()["capture"]["id"].clone();

    assert_eq!(lens.ask(r#"{"cmd":"pick-last"}"#)["ok"], true);
    assert_ne!(lens.state()["capture"]["id"], first);
    assert_eq!(lens.state()["voice"]["state"], "off");

    // The first voice was stopped, so the helper is free for this capture's.
    lens.ask(r#"{"cmd":"key","key":"voice"}"#);
    assert_eq!(lens.state()["error"], Value::Null);
    assert_eq!(lens.state()["voice"]["state"], "starting");
}

#[test]
fn voice_wav_takes_only_a_regular_file_by_its_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let lens = Harness::new(dir, Some("wav"));
    let fifo = dir.join("voice.wav");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let refused = lens.ask(&format!(
        r#"{{"cmd":"voice-wav","path":"{}"}}"#,
        fifo.display()
    ));
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .ends_with("is not a regular file."),
        "{refused}"
    );
    let relative = lens.ask(r#"{"cmd":"voice-wav","path":"voice.wav"}"#);
    assert_eq!(relative["ok"], false, "{relative}");
    let wav: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "speech/tests/fixtures/short-phrases.wav",
    ]
    .iter()
    .collect();
    let taken = lens.ask(&format!(
        r#"{{"cmd":"voice-wav","path":"{}"}}"#,
        wav.display()
    ));
    assert_eq!(taken["ok"], true, "{taken}");
    assert_eq!(lens.ask(r#"{"cmd":"voice-wav"}"#)["ok"], true);
}

#[test]
fn screen_text_replies_with_the_front_windows_text_or_the_reads_error() {
    use attn_lens::platform_port::PlatformPort;
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("lens-screen.sock");
    let port = Arc::new(FakePort::new(
        vec![
            ("screen-text:open src/lens/capture.rs".into(), 1.),
            ("screen-text:fail".into(), 2.),
        ],
        None,
    ));
    automation::listen(Lens::new(port.clone(), Prefs::memory()), &socket).unwrap();
    let ask = || -> Value {
        serde_json::from_str(&automation::request(&socket, r#"{"cmd":"screen_text"}"#).unwrap())
            .unwrap()
    };

    let empty = ask();
    assert_eq!(
        (&empty["ok"], &empty["text"]),
        (&Value::Bool(true), &"".into())
    );
    port.tick(1.);
    let read = ask();
    assert_eq!(read["text"], "open src/lens/capture.rs", "{read}");
    assert!(read["ms"].is_u64(), "{read}");
    port.tick(2.);
    let failed = ask();
    assert_eq!(failed["ok"], false, "{failed}");
    assert_eq!(failed["error"], "The fake screen read failed.");
}
