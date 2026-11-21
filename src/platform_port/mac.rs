//! The macOS port. The capture, voice and draw keys come from the key engine's
//! event taps (`crate::keys`), which need Accessibility; ⌘⌥C and ⌘⌥↩ are Carbon
//! hotkeys. Stills come from capture-core. AppKit and Carbon calls run on the
//! main thread.
#![allow(unexpected_cfgs)] // objc 0.2 macros use a legacy cargo-clippy feature.
use super::{
    Chord, KeyEvent, PlatformPort, Sink, Voice, VoiceEvent, VoiceSource, Watch, WatchEvent,
};
use crate::{
    keys::tap::Tap,
    lens::{
        attn, capture,
        session::{Area, DisplayFrame, HostTime},
        voice::{Clock, Speech},
        watch,
    },
    permission_guide::{self, Permission},
};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use image::RgbaImage;
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use std::{
    cell::RefCell,
    ffi::c_void,
    sync::{Arc, Mutex, OnceLock},
};

/// `NSEventModifierFlagShift`.
const SHIFT_FLAG: u64 = 0x2_0000;

const COMMAND_OPTION: Modifiers = Modifiers::SUPER.union(Modifiers::ALT);

thread_local! {
    /// Lives on the main thread, where Carbon registers hotkeys.
    static HOTKEYS: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
}

/// The chords that exist only during a capture (D4 on the plan).
fn capture_only_chords() -> [(HotKey, Chord); 2] {
    [
        (
            HotKey::new(Some(COMMAND_OPTION), Code::KeyC),
            Chord::Keyboard,
        ),
        (HotKey::new(Some(COMMAND_OPTION), Code::Enter), Chord::Send),
    ]
}

#[link(name = "System")]
unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut Timebase) -> i32;
    fn pthread_main_np() -> i32;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timebase {
    numer: u32,
    denom: u32,
}

/// Runs `job` on the main thread: now if this is it, otherwise soon.
fn on_main(job: impl FnOnce() + Send + 'static) {
    // SAFETY: plain libc query.
    if unsafe { pthread_main_np() } != 0 {
        return job();
    }
    queue_on_main(job);
}

/// Runs `job` on the main thread and waits for its result. From a worker, the main
/// thread must not be waiting on that worker.
fn on_main_wait<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    on_main(move || {
        let _ = sender.send(job());
    });
    receiver.recv().expect("the main thread dropped a job")
}

/// Runs `job` on the main thread after everything queued before it, from any thread.
pub(crate) fn queue_on_main(job: impl FnOnce() + Send + 'static) {
    extern "C" fn trampoline(context: *mut c_void) {
        // SAFETY: `context` is the box leaked below, and this runs once.
        let job = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce() + Send>>()) };
        job();
    }
    let job: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(job));
    // SAFETY: the main queue outlives the process, and the trampoline takes ownership.
    unsafe {
        dispatch_async_f(
            &raw const _dispatch_main_q,
            Box::into_raw(job).cast(),
            trampoline,
        );
    }
}

pub struct MacPort {
    speech: Arc<Speech>,
    /// Set once by [`PlatformPort::keys`].
    tap: OnceLock<Tap>,
    /// The process that had the keyboard when Lens last took it, as `hold_front` saw it.
    held_front: Arc<Mutex<Option<i32>>>,
}

/// The process of the app that is active now.
fn frontmost_pid() -> Option<i32> {
    let class = AnyClass::get(c"NSWorkspace")?;
    // SAFETY: `sharedWorkspace` and `frontmostApplication` return live objects or nil, and
    // `processIdentifier` is a plain integer property.
    unsafe {
        let workspace: Retained<AnyObject> = msg_send![class, sharedWorkspace];
        let app: Option<Retained<AnyObject>> = msg_send![&workspace, frontmostApplication];
        app.map(|app| msg_send![&app, processIdentifier])
    }
}

/// Hands the active app to `pid`. Lens is the active app when this runs, so it may
/// activate another (macOS 14's cooperative activation); a process that has quit is skipped.
fn activate_from_lens(pid: i32) {
    let Some(class) = AnyClass::get(c"NSRunningApplication") else {
        return;
    };
    // SAFETY: both receivers are live objects or nil, and the option is `0`: none.
    unsafe {
        let target: Option<Retained<AnyObject>> =
            msg_send![class, runningApplicationWithProcessIdentifier: pid];
        let me: Retained<AnyObject> = msg_send![class, currentApplication];
        if let Some(target) = target {
            let _: bool = msg_send![&target, activateFromApplication: &*me, options: 0usize];
        }
    }
}

impl MacPort {
    pub fn new() -> Self {
        Self {
            speech: Arc::default(),
            tap: OnceLock::new(),
            held_front: Arc::default(),
        }
    }

    fn clock(&self) -> Clock {
        // SAFETY: plain clock read.
        Arc::new(|| host_time(unsafe { mach_absolute_time() }))
    }
}

/// `mach_absolute_time` ticks as host time, the clock every capture time uses.
pub fn host_time(ticks: u64) -> HostTime {
    static TIMEBASE: OnceLock<Timebase> = OnceLock::new();
    let base = TIMEBASE.get_or_init(|| {
        let mut base = Timebase::default();
        // SAFETY: `base` is a valid out pointer.
        unsafe { mach_timebase_info(&mut base) };
        base
    });
    HostTime(ticks * u64::from(base.numer) / u64::from(base.denom))
}

impl Default for MacPort {
    fn default() -> Self {
        Self::new()
    }
}

fn install_hotkeys(sink: Sink<KeyEvent>) -> Result<(), String> {
    let manager =
        GlobalHotKeyManager::new().map_err(|e| format!("Global shortcuts unavailable: {e}"))?;
    let chords = capture_only_chords().map(|(key, chord)| (key.id(), chord));
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state != HotKeyState::Pressed {
            return;
        }
        if let Some((_, chord)) = chords.iter().find(|(id, _)| *id == event.id) {
            sink(KeyEvent::Pressed(*chord));
        }
    }));
    HOTKEYS.with(|hotkeys| *hotkeys.borrow_mut() = Some(manager));
    Ok(())
}

/// The current modifier flags. A class property that needs no permission and works
/// off the main thread, for keys held in any app.
fn modifier_flags() -> u64 {
    let class = AnyClass::get(c"NSEvent").expect("AppKit is linked");
    // SAFETY: a class property with no arguments.
    unsafe { msg_send![class, modifierFlags] }
}

impl PlatformPort for MacPort {
    fn now(&self) -> HostTime {
        // SAFETY: plain clock read.
        host_time(unsafe { mach_absolute_time() })
    }

    fn keys(&self, sink: Sink<KeyEvent>) {
        match Tap::start(sink.clone()) {
            Ok(tap) => {
                let _ = self.tap.set(tap);
            }
            Err(error) => eprintln!("Attn Lens: {error}"),
        }
        on_main(move || {
            if let Err(error) = install_hotkeys(sink) {
                eprintln!("Attn Lens: {error}");
            }
        });
    }

    fn hold_front(&self) {
        // Lens itself being in front means an earlier hold already noted the app before it.
        if let Some(pid) = frontmost_pid().filter(|&pid| pid != std::process::id() as i32) {
            *self.held_front.lock().unwrap() = Some(pid);
        }
    }

    fn return_front(&self) {
        let Some(pid) = self.held_front.lock().unwrap().take() else {
            return;
        };
        // Asked on the main thread, where the answer cannot change under the activation.
        on_main(move || {
            if frontmost_pid() == Some(std::process::id() as i32) {
                activate_from_lens(pid);
            }
        });
    }

    fn release_keys(&self) -> bool {
        on_main_wait(|| {
            let Some(class) = AnyClass::get(c"NSApplication") else {
                return false;
            };
            // SAFETY: AppKit calls on the main thread; `keyWindow` is a live window or nil.
            // Ordering the key panel out hands the keyboard back to the window that had
            // it, as a hide does: the active app's, or the editor's when Lens is in
            // front. Ordering it straight back in shows it again without taking keys,
            // within one turn of the run loop, so it never leaves the screen.
            unsafe {
                let app: Retained<AnyObject> = msg_send![class, sharedApplication];
                let window: Option<Retained<AnyObject>> = msg_send![&app, keyWindow];
                let Some(window) = window else {
                    return false;
                };
                let none: *const AnyObject = std::ptr::null();
                let _: () = msg_send![&window, orderOut: none];
                let _: () = msg_send![&window, orderFrontRegardless];
            }
            true
        })
    }

    fn capture_keys(&self, open: bool) {
        if let Some(tap) = self.tap.get() {
            tap.capture_open(open);
        }
        on_main(move || {
            HOTKEYS.with(|hotkeys| {
                let hotkeys = hotkeys.borrow();
                let Some(manager) = hotkeys.as_ref() else {
                    return;
                };
                for (key, _) in capture_only_chords() {
                    let result = if open {
                        manager.register(key)
                    } else {
                        manager.unregister(key)
                    };
                    if let Err(error) = result {
                        eprintln!("Attn Lens: {key:?}: {error}");
                    }
                }
            });
        });
    }

    fn pill_keys(&self, on: bool) {
        if let Some(tap) = self.tap.get() {
            tap.pill_keys(on);
        }
    }

    /// The origin if the system gives no pointer: the strip then stands at the
    /// area's corner, still on screen.
    fn pointer(&self) -> (f64, f64) {
        capture::pointer().unwrap_or_default()
    }

    fn pointer_display(&self) -> Result<Area, String> {
        let display = capture::display_under_pointer();
        let (width, height) = capture::display_size(display);
        Ok(Area {
            display,
            rect: [0., 0., width, height],
        })
    }

    fn displays(&self) -> Vec<DisplayFrame> {
        capture::displays()
    }

    fn shift_held(&self) -> bool {
        modifier_flags() & SHIFT_FLAG != 0
    }

    fn screen_recording(&self) -> bool {
        let granted = permission_guide::granted(Permission::ScreenRecording);
        if !granted {
            on_main(|| permission_guide::show(Permission::ScreenRecording));
        }
        granted
    }

    fn accessibility(&self) -> bool {
        let granted = permission_guide::granted(Permission::Accessibility);
        if !granted {
            on_main(|| permission_guide::show(Permission::Accessibility));
        }
        granted
    }

    fn still(&self, area: Area) -> Result<RgbaImage, String> {
        capture::screenshot(area)
    }

    fn screen_text(&self) -> Result<String, String> {
        if !self.screen_recording_allowed() {
            return Err("Screen Recording is not allowed for Attn Lens.".into());
        }
        let window = capture::front_window().ok_or("No window is in front.")?;
        crate::ocr::extract(&capture::window_still(window)?, None)
    }

    fn screen_recording_allowed(&self) -> bool {
        permission_guide::granted(Permission::ScreenRecording)
    }

    /// macOS's `/usr/share/dict/words` (web2, 235,976 words), read once. Its spell
    /// checker knows `attn`, `stdin` and `MCP`, the words worth spelling (F5 on s-0ww1ak).
    fn ordinary_word(&self, word: &str) -> bool {
        static WORDS: std::sync::OnceLock<std::collections::HashSet<String>> =
            std::sync::OnceLock::new();
        WORDS
            .get_or_init(|| {
                std::fs::read_to_string("/usr/share/dict/words")
                    .unwrap_or_default()
                    .lines()
                    .map(str::to_lowercase)
                    .collect()
            })
            .contains(word)
    }

    fn watch(&self, area: Area, sink: Sink<WatchEvent>) -> Watch {
        watch::start(area, sink)
    }

    fn voice(&self, source: VoiceSource, sink: Sink<VoiceEvent>) -> Result<Voice, String> {
        Ok(self.speech.start(source, self.clock(), sink))
    }

    fn end_voice(&self) {
        self.speech.release();
    }

    fn token(&self) -> Option<String> {
        attn::token()
    }

    fn store_token(&self, token: &str) -> Result<(), String> {
        attn::store_token(token).map_err(|failure| match failure {
            attn::Failure::NotPaired(text) | attn::Failure::Error(text) => text,
        })
    }

    fn forget_token(&self) {
        attn::forget_token();
    }

    fn copy_image(&self, image: RgbaImage) -> Result<(), String> {
        crate::platform::copy(image)
    }

    fn clipboard_image(&self) -> Result<RgbaImage, String> {
        crate::platform::clipboard_image()
    }

    fn copy_text(&self, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(text))
            .map_err(|e| e.to_string())
    }

    fn paste(&self, text: &str) -> Result<(), String> {
        let v_key = on_main_wait(crate::paste::v_key);
        crate::paste::paste(text, v_key)
    }

    fn ocr(&self, image: &RgbaImage, rectangle: Option<[u32; 4]>) -> Result<String, String> {
        crate::ocr::extract(image, rectangle)
    }

    fn pick_color(&self, sink: Sink<Result<Option<[u8; 3]>, String>>) {
        // AppKit shows the sampler on the main thread; this thread waits for its answer.
        on_main(move || match crate::platform::sample_screen_color() {
            Ok(answer) => {
                std::thread::spawn(move || {
                    sink(
                        answer
                            .recv_blocking()
                            .unwrap_or_else(|_| Err("Screen sampler closed".into())),
                    );
                });
            }
            Err(error) => sink(Err(error)),
        });
    }
}
