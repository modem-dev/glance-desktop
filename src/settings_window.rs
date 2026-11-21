//! The Settings window: Lens's settings as rows grouped in sections, each a label,
//! one line saying what it does, and its control. A change saves at once through
//! `Lens::set_settings`; the window only reads `Lens` state.
use crate::{lens::Lens, settings::Settings};
use rune::{Cx, El, col, row, text};
use rune_kit::{Divider, Kit, Switch};

/// The window's scenes, for `--window settings --scene NAME`: each section as Lens
/// opens it, and with its settings changed through the window's own controls.
pub(crate) const SCENES: crate::app::Scenes<SettingsWindow> = &[
    ("dictation", |_| {}),
    ("dictation-off", |w| {
        w.change(|s| s.screen_words = !s.screen_words)
    }),
    ("dictation-no-screen-recording", |w| {
        w.lens
            .port()
            .scene_mark("screen-recording:off")
            .unwrap_or_else(|error| panic!("{error}"))
    }),
];
use std::sync::Arc;

/// The window's width; its height follows the rows.
pub const WIDTH: f32 = 480.;

pub struct SettingsWindow {
    pub lens: Arc<Lens>,
}

impl SettingsWindow {
    /// Applies `change` to the current settings and saves them.
    fn change(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.lens.read(|s| s.settings.clone());
        change(&mut settings);
        self.lens.set_settings(settings);
    }
}

struct Row {
    key: &'static str,
    label: &'static str,
    about: &'static str,
    /// Why the setting does not work now.
    note: Option<&'static str>,
    control: El,
}

struct Section {
    key: &'static str,
    title: &'static str,
    rows: Vec<Row>,
}

fn sections(settings: &Settings, screen_recording: bool, k: &Kit) -> Vec<Section> {
    vec![Section {
        key: "dictation",
        title: "Dictation",
        rows: vec![Row {
            key: "screen-words",
            label: "Use words on screen",
            about: "When you dictate, Lens reads the window in front and sends the names and \
                    identifiers on it that you said with your words to the refiner, so they \
                    come out spelled right.",
            note: (settings.screen_words && !screen_recording).then_some(
                "Screen Recording is not allowed for Attn Lens, so dictation does not read \
                 the screen. Allow it in System Settings › Privacy & Security.",
            ),
            control: Switch::new(settings.screen_words)
                .label("Use words on screen")
                .hide_label()
                .on_toggle(|w: &mut SettingsWindow| w.change(|s| s.screen_words = !s.screen_words))
                .build(k),
        }],
    }]
}

pub fn view(s: &SettingsWindow, cx: &mut Cx<SettingsWindow>) -> El {
    let k = crate::kit::kit(cx.system_dark());
    let (settings, error) = s
        .lens
        .read(|summary| (summary.settings.clone(), summary.settings_error.clone()));
    let screen_recording = s.lens.screen_recording_allowed();
    let mut page = col()
        .key("settings")
        .fill(k.c.bg_base)
        .px(20.)
        .pt(16.)
        .pb(20.)
        .gap(20.)
        .children(
            sections(&settings, screen_recording, k)
                .into_iter()
                .map(|s| section(s, k)),
        );
    if let Some(error) = error {
        page = page.child(
            text(format!("Not saved: {error}"))
                .key("settings-error")
                .font(k.ty.caption())
                .color(k.c.danger.fg)
                .px(12.)
                .wrap()
                .w_full(),
        );
    }
    page
}

fn section(section: Section, k: &Kit) -> El {
    // Darker than the window, as macOS draws grouped rows; the kit's off switch is
    // white in light mode and needs that ground to show.
    let mut group = col()
        .fill(k.c.page)
        .radius(k.radius.md)
        .border(1., k.c.border_weak);
    for (index, row) in section.rows.into_iter().enumerate() {
        if index > 0 {
            group = group.child(Divider::horizontal().build(k));
        }
        group = group.child(setting_row(row, k));
    }
    col()
        .key(format!("{}-section", section.key))
        .gap(6.)
        .child(
            text(section.title)
                .key(format!("{}-title", section.key))
                .font(k.ty.small_medium)
                .color(k.c.text_muted)
                .px(12.),
        )
        .child(group)
}

fn setting_row(row_: Row, k: &Kit) -> El {
    row()
        .key(row_.key)
        .px(12.)
        .py(10.)
        .gap(16.)
        .center_y()
        .child(
            col()
                .grow(1.)
                .shrink()
                .gap(2.)
                .child(
                    text(row_.label)
                        .key(format!("{}-label", row_.key))
                        .font(k.ty.ui)
                        .color(k.c.text_base),
                )
                .child(
                    text(row_.about)
                        .key(format!("{}-about", row_.key))
                        .font(k.ty.caption())
                        .color(k.c.text_muted)
                        .wrap()
                        .w_full(),
                )
                .children(row_.note.map(|note| {
                    text(note)
                        .key(format!("{}-note", row_.key))
                        .font(k.ty.caption())
                        .color(k.c.warning.fg)
                        .wrap()
                        .w_full()
                })),
        )
        .child(row_.control.key(format!("{}-control", row_.key)))
}
