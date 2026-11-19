//! The Rune app: a Dock app whose windows read the Lens state. It starts with no
//! window, the capture key (right ⌘) opens the area picker, and a Dock click opens the empty editor.
use crate::{
    editor::{self, Editor},
    lens::{
        EditorCall, Input, KeyOwner, Lens, Summary, automation,
        bar::{self, Bar},
        ink::{Corner, Handle, Ink, handle_view, ink_view},
        pill::{self, Pill},
        scenes,
        select::{self, PICKER_SLOTS, Picker},
        session::{Area, DisplayFrame},
        tile::{self, Tile},
    },
    platform_port,
};
use rune::{
    AnyWindowSpec, App, AppHandle, AppSpec, Cx, DisplayTarget, El, KeyEvent, KeyPolicy,
    NativeDisplayId, OverlayHandle, OverlayInput, OverlayLevel, OverlayOptions, OverlayPlacement,
    OverlaySpec, Tx, WindowOptions, WindowSpec,
};
use std::sync::{Arc, Mutex};

/// One frame of a 60 Hz display, the fade's slowest useful step.
const FADE_FRAME: std::time::Duration = std::time::Duration::from_micros(16_667);

const EDITOR_SIZE: (f32, f32) = (1220., 860.);
/// Room for the toolbar and the sidebar's longest option list.
const EDITOR_MIN_SIZE: (f32, f32) = (900., 560.);

/// A view that is an overlay on the Mac and an ordinary window when headless,
/// where Rune draws no overlays (D2 on the plan). This is the only platform branch.
enum Surface<T: 'static> {
    Overlay(OverlayHandle<T>),
    Window(AppHandle<T>),
}

impl<T: 'static> Surface<T> {
    fn show(&self) {
        match self {
            Surface::Overlay(handle) => handle.show(),
            Surface::Window(handle) => handle.show(),
        }
    }
    fn hide(&self) {
        match self {
            Surface::Overlay(handle) => handle.hide(),
            Surface::Window(handle) => handle.hide(),
        }
    }
    fn take_keys(&self) {
        match self {
            Surface::Overlay(handle) => handle.request_keys(),
            Surface::Window(handle) => handle.focus(),
        }
    }
    fn place(&self, display: u32, placement: OverlayPlacement) {
        let target = DisplayTarget::Native(NativeDisplayId::MacOS(display));
        match self {
            Surface::Overlay(handle) => {
                handle.set_display(target);
                handle.set_placement(Some(placement));
            }
            Surface::Window(handle) => handle.place(target, placement),
        }
    }
    /// Headless, a window stays clickable and the view ignores what it should not take.
    fn set_input(&self, input: OverlayInput) {
        if let Surface::Overlay(handle) = self {
            handle.set_input(input);
        }
    }
    fn refresh(&self) {
        match self {
            Surface::Overlay(handle) => handle.update(|_, _| {}),
            Surface::Window(handle) => handle.update(|_, _| {}),
        }
    }
}

type KeyHandler<T> = fn(&mut T, &KeyEvent, &mut Tx) -> bool;

/// A window's scenes, for `--scene NAME`: each builds a screen or hard state on the
/// state `init` made. `scripts/snap.sh` renders them all.
pub(crate) type Scenes<S> = &'static [(&'static str, fn(&mut S))];

/// What `host` shows: a view of its own state, the keys it takes and its scenes.
struct Layer<T: 'static> {
    view: fn(&T, &mut Cx<T>) -> El,
    on_key: Option<KeyHandler<T>>,
    scenes: Scenes<T>,
}

/// Adds `view` to the app `owner` as a surface over the screen, left out of stills.
fn host<O: 'static, T: 'static>(
    owner: &mut App<O>,
    windows: &mut Vec<AnyWindowSpec>,
    key: String,
    layer: Layer<T>,
    options: OverlayOptions,
    state: T,
) -> Surface<T> {
    let Layer {
        view,
        on_key,
        scenes,
    } = layer;
    if rune::tooling::is_headless() {
        let app = App::new(AppSpec {
            on_key,
            scenes,
            ..AppSpec::new("Attn Lens Layer", (64., 64.), || unreachable!(), view)
        })
        .windowless();
        let handle = app.handle();
        let app = app.init(move || state);
        windows.push(
            WindowSpec::from_app(key, app)
                .options(WindowOptions {
                    transparent: true,
                    borderless: true,
                    visible: false,
                    always_on_top: true,
                    min_size: Some((0., 0.)),
                    ..Default::default()
                })
                .into(),
        );
        return Surface::Window(handle);
    }
    let mut spec = OverlaySpec::new("attn-lens-layer", (64., 64.), view)
        .options(options)
        .release_gpu_when_hidden();
    if let Some(on_key) = on_key {
        spec = spec.on_key(on_key);
    }
    Surface::Overlay(owner.overlay(spec, state))
}

fn overlay_input(input: Input) -> OverlayInput {
    match input {
        // The page gets the pointer; the handles are windows of their own.
        Input::None => OverlayInput::None,
        Input::All => OverlayInput::All,
    }
}

/// What the windows show. Applying a change only when it differs matters: showing
/// a window or asking for keys takes the keyboard from the page.
#[derive(Clone, Default, PartialEq)]
struct Shown {
    picking: Option<Vec<DisplayFrame>>,
    area: Option<Area>,
    input: Input,
    /// Each corner handle's window, while the draw key is up and the bar does not
    /// cover it: the handles stand above the bar and would take its clicks.
    handles: [Option<[f32; 4]>; 4],
    resizing: bool,
    bar_rect: Option<[f32; 4]>,
    bar_key_requests: u64,
    tile: Option<(u32, [f32; 4])>,
    pill: Option<(u32, [f32; 4])>,
}

impl Shown {
    fn of(summary: &Summary) -> Self {
        // The editor stands in for the live surfaces while it holds a still.
        let capture = summary
            .capture
            .as_ref()
            .filter(|_| summary.editing.is_none());
        let bar_rect =
            capture.map(|c| bar::placement(c.area.rect, c.display, bar::height(summary), c.bar_at));
        Self {
            picking: summary.picking.clone(),
            area: capture.map(|c| c.area),
            input: summary.input,
            handles: Corner::ALL.map(|corner| {
                capture
                    .filter(|_| summary.input == Input::None)
                    .map(|c| corner.rect(c.area, c.display))
                    .filter(|rect| !bar_rect.is_some_and(|bar| overlaps(*rect, bar)))
            }),
            resizing: summary.resizing,
            bar_rect,
            bar_key_requests: summary.bar_key_requests,
            tile: summary
                .tile
                .as_ref()
                .map(|t| (t.display.id, tile::placement(t.display))),
            pill: summary
                .dictation
                .as_ref()
                .and(summary.dictation_display)
                .map(|display| (display.id, pill::placement(display))),
        }
    }
}

/// Whether two `[x, y, width, height]` rects on one display share any point.
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

struct Surfaces {
    pickers: Vec<Surface<Picker>>,
    ink: Surface<Ink>,
    handles: Vec<Surface<Handle>>,
    bar: AppHandle<Bar>,
    tile: Surface<Tile>,
    pill: Surface<Pill>,
}

impl Surfaces {
    fn apply(&self, lens: &Lens, was: &Shown, now: &Shown) {
        if was.picking != now.picking {
            for (slot, picker) in self.pickers.iter().enumerate() {
                match now.picking.as_ref().and_then(|d| d.get(slot)) {
                    Some(display) => {
                        picker.place(display.id, OverlayPlacement::FullScreen);
                        picker.show();
                    }
                    None => picker.hide(),
                }
            }
            if let Some(displays) = &now.picking {
                let under_pointer = lens.port().pointer_display().map(|area| area.display);
                let slot = displays
                    .iter()
                    .position(|d| Ok(d.id) == under_pointer)
                    .unwrap_or(0);
                if let Some(picker) = self.pickers.get(slot) {
                    lens.port().hold_front();
                    picker.take_keys();
                }
            }
        }
        if was.area != now.area {
            match now.area {
                Some(area) => {
                    let [x, y, width, height] = area.rect.map(|v| v as f32);
                    self.ink.place(
                        area.display,
                        OverlayPlacement::Rect {
                            x,
                            y,
                            width,
                            height,
                        },
                    );
                    if was.area.is_none() {
                        self.ink.show();
                    }
                }
                None => self.ink.hide(),
            }
        }
        if was.input != now.input {
            self.ink.set_input(overlay_input(now.input));
        }
        // A handle that owns a drag must not move under it; they land when it ends.
        let landed = was.resizing && !now.resizing;
        if !now.resizing && (was.handles != now.handles || landed) {
            for (index, handle) in self.handles.iter().enumerate() {
                match (now.area, now.handles[index]) {
                    (Some(area), Some([x, y, width, height])) => {
                        handle.place(
                            area.display,
                            OverlayPlacement::Rect {
                                x,
                                y,
                                width,
                                height,
                            },
                        );
                        // A resize holds the handles, so one it uncovered shows as it lands.
                        if landed || was.handles[index].is_none() {
                            handle.show();
                        }
                    }
                    _ => handle.hide(),
                }
            }
        }
        if let (Some(area), true) = (now.area, was.bar_rect != now.bar_rect)
            && let Some([x, y, width, height]) = now.bar_rect
        {
            self.bar.place(
                DisplayTarget::Native(NativeDisplayId::MacOS(area.display)),
                OverlayPlacement::Rect {
                    x,
                    y,
                    width,
                    height,
                },
            );
        }
        if was.tile != now.tile {
            match now.tile {
                Some((display, [x, y, width, height])) => {
                    self.tile.place(
                        display,
                        OverlayPlacement::Rect {
                            x,
                            y,
                            width,
                            height,
                        },
                    );
                    self.tile.show();
                }
                None => self.tile.hide(),
            }
        }
        if was.pill != now.pill {
            match now.pill {
                Some((display, [x, y, width, height])) => {
                    self.pill.place(
                        display,
                        OverlayPlacement::Rect {
                            x,
                            y,
                            width,
                            height,
                        },
                    );
                    if was.pill.is_none() {
                        self.pill.show();
                        lens.pill_shown();
                    }
                }
                None => self.pill.hide(),
            }
        }
        if now.area.is_none() && was.area.is_some() {
            self.bar.hide();
        }
        if now.area.is_some() && was.bar_key_requests != now.bar_key_requests {
            lens.port().hold_front();
            self.bar.open();
        }
    }

    fn refresh(&self) {
        for picker in &self.pickers {
            picker.refresh();
        }
        self.ink.refresh();
        self.tile.refresh();
        self.pill.refresh();
        for handle in &self.handles {
            handle.refresh();
        }
        self.bar.wake();
    }
}

/// Runs the app until it quits. Returns an error only if it cannot start.
pub fn run() -> Result<(), String> {
    let lens = Lens::new(platform_port::select()?);
    if std::env::args().any(|arg| arg == "--automation") {
        automation::listen(lens.clone(), &automation::socket_path()?)?;
    }
    rune::run_windows(move |displays| {
        lens.port().launch_displays(
            displays
                .iter()
                .enumerate()
                .map(|(index, display)| DisplayFrame {
                    id: index as u32 + 1,
                    frame: [
                        f64::from(display.position.0) / display.scale,
                        f64::from(display.position.1) / display.scale,
                        f64::from(display.size.0) / display.scale,
                        f64::from(display.size.1) / display.scale,
                    ],
                    // A headless display has no menu bar; the fake gives one.
                    menu_bar: 0.,
                    dock: 0.,
                })
                .collect(),
        );
        lens.listen();

        let editor = App::new(AppSpec {
            poll: Some(|s: &mut Editor, now, tx: &mut Tx| s.poll(now, tx)),
            on_key: Some(|s: &mut Editor, event, tx: &mut Tx| s.on_key(event, tx)),
            min_size: Some(EDITOR_MIN_SIZE),
            scenes: editor::scenes::SCENES,
            ..AppSpec::new("Attn Lens", EDITOR_SIZE, || unreachable!(), editor::view)
        })
        .windowless();
        let me = editor.handle();
        let visible = me.clone();
        lens.report_window("editor", move || visible.is_visible());
        let opening = me.clone();
        let port = lens.port().clone();
        let closing = me.clone();
        port.script_button(
            "editor",
            Arc::new(move |()| closing.update(|s: &mut Editor, tx| s.on_window_close(tx))),
        );
        let editing = lens.clone();
        port.script_button("bar-edit", Arc::new(move |()| editing.edit_latest()));
        let copying = lens.clone();
        port.script_button(
            "copy-last-dictation",
            Arc::new(move |()| copying.copy_last_dictation()),
        );
        let picking = lens.clone();
        port.script_button(
            "pick",
            Arc::new(move |()| {
                let _ = picking.pick_pointer_display();
            }),
        );
        let for_editor = lens.clone();
        lens.set_editor(Arc::new(move |call| match call {
            EditorCall::Open { still, document } => {
                opening.update(move |s: &mut Editor, tx| s.open_still(still, *document, tx))
            }
            EditorCall::Leave { send } => {
                opening.update(move |s: &mut Editor, tx| s.leave_for_key(send, tx))
            }
        }));
        let editor = editor::menus::install(editor)
            .init(move || Editor::new(me, port, for_editor))
            .on_focus(|s, focused, _| s.on_window_focus(focused))
            .on_close(|s, tx| s.on_window_close(tx))
            .on_reopen(|s, _, _| {
                s.open_window();
                true
            });

        let mut bar = App::new(AppSpec {
            on_key: Some(bar::bar_key),
            scenes: scenes::BAR_SCENES,
            poll: Some(|s: &mut Bar, now, tx: &mut Tx| {
                let lens = s.lens.clone();
                if let Some(next) = lens.port().tick(now) {
                    tx.wake_after(next);
                }
                if lens.advance_fades() {
                    tx.wake_after(FADE_FRAME);
                }
                if let Some(next) = lens.advance_tile() {
                    tx.wake_after(next);
                }
                if let Some(next) = lens.advance_dictation() {
                    tx.wake_after(next);
                }
                let generation = lens.generation();
                let changed = generation != s.seen;
                s.seen = generation;
                s.follow();
                changed
            }),
            ..AppSpec::new(
                "Attn Lens Bar",
                (bar::WIDTH, 100.),
                || unreachable!(),
                bar::bar_view,
            )
        })
        .windowless();
        let handle = bar.handle();
        let visible = handle.clone();
        lens.report_window("bar", move || visible.is_visible());

        let mut windows: Vec<AnyWindowSpec> = vec![];
        let pickers: Vec<Surface<Picker>> = (0..PICKER_SLOTS)
            .map(|slot| {
                host(
                    &mut bar,
                    &mut windows,
                    format!("picker-{slot}"),
                    Layer {
                        view: select::picker_view,
                        on_key: Some(select::picker_key),
                        scenes: if slot == 0 {
                            scenes::PICKER_SCENES
                        } else {
                            &[]
                        },
                    },
                    OverlayOptions {
                        placement: Some(OverlayPlacement::FullScreen),
                        exclude_from_capture: true,
                        keys: KeyPolicy::OnDemand,
                        input: OverlayInput::All,
                        level: OverlayLevel::AboveStatus,
                        ..Default::default()
                    },
                    Picker {
                        lens: lens.clone(),
                        slot,
                        drag: None,
                    },
                )
            })
            .collect();
        let ink = host(
            &mut bar,
            &mut windows,
            "ink".into(),
            Layer {
                view: ink_view,
                on_key: None,
                scenes: scenes::INK_SCENES,
            },
            OverlayOptions {
                // Only so a recording of a driven check can show the ink.
                exclude_from_capture: !std::env::args().any(|arg| arg == "--ink-in-capture"),
                input: OverlayInput::None,
                ..Default::default()
            },
            Ink::new(lens.clone()),
        );
        // After the ink layer, so they sit above it.
        let handles = Corner::ALL
            .into_iter()
            .map(|corner| {
                host(
                    &mut bar,
                    &mut windows,
                    corner.key().into(),
                    Layer {
                        view: handle_view,
                        on_key: None,
                        scenes: &[],
                    },
                    OverlayOptions {
                        exclude_from_capture: true,
                        input: OverlayInput::All,
                        ..Default::default()
                    },
                    Handle::new(lens.clone(), corner),
                )
            })
            .collect();

        let tile = host(
            &mut bar,
            &mut windows,
            "tile".into(),
            Layer {
                view: tile::tile_view,
                on_key: None,
                scenes: scenes::TILE_SCENES,
            },
            OverlayOptions {
                exclude_from_capture: true,
                input: OverlayInput::Regions,
                ..Default::default()
            },
            Tile { lens: lens.clone() },
        );

        // Never takes the keyboard or a click, so typing stays in the app in front.
        let pill = host(
            &mut bar,
            &mut windows,
            "pill".into(),
            Layer {
                view: pill::pill_view,
                on_key: None,
                scenes: scenes::PILL_SCENES,
            },
            OverlayOptions {
                // Only so a recording of a driven check can show the pill.
                exclude_from_capture: !std::env::args().any(|arg| arg == "--pill-in-capture"),
                input: OverlayInput::None,
                keys: KeyPolicy::None,
                ..Default::default()
            },
            Pill { lens: lens.clone() },
        );

        let surfaces = Surfaces {
            pickers,
            ink,
            handles,
            bar: handle.clone(),
            tile,
            pill,
        };
        let shown = Mutex::new(Shown::default());
        let reader = lens.clone();
        lens.on_change(move || {
            let now = reader.read(Shown::of);
            let mut was = shown.lock().unwrap();
            surfaces.apply(&reader, &was, &now);
            *was = now;
            surfaces.refresh();
        });

        let state = lens.clone();
        let bar = bar
            .init(move || Bar::new(state))
            .on_focus(move |s, focused, _| {
                // Lens hands the keys to the bar itself (a capture opening, ⌘⌥C), so
                // the page still having them means a click on the bar took them.
                s.keys_from_page = (focused
                    && s.lens.read(|summary| summary.keys == KeyOwner::Page))
                .then(|| (rune::time::Instant::now(), s.lens.port().now()));
                s.lens.set_key_owner(if focused {
                    KeyOwner::Bar
                } else {
                    KeyOwner::Page
                });
            });

        let mut specs = vec![
            AnyWindowSpec::from(WindowSpec::from_app("editor", editor)),
            WindowSpec::from_app("bar", bar)
                .configured_options(WindowOptions::launcher_panel())
                .into(),
        ];
        specs.extend(windows);
        specs
    });
    Ok(())
}
