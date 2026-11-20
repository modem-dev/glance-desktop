//! The sidebar's controls, on rune-kit components painted by `crate::kit`.
use super::super::{Editor, actions::Action};
use rune::{Cursor, Cx, El, Key, TextStyle, Tx, col, el, row, text, text_input};
use rune_kit::{
    Button, ButtonVariant, Checkbox, Field, Kit, NumberStepper, SegmentedControl, Select,
    SelectAppearance, Slider, Switch,
};

/// The width of a control in the sidebar: the sidebar inside its padding.
pub(crate) const INNER: f32 = super::WIDTH - 28.;

/// Overlays the sidebar's controls open, such as a Select's list, mounted over the
/// window by `rune_kit::root`.
pub(crate) type Layer = Vec<El>;

pub(crate) fn kit(cx: &Cx<Editor>) -> &'static Kit {
    crate::kit::kit(cx.system_dark())
}

/// A label set as the kit's Field sets its own, for a label Lens draws beside a control.
pub(crate) fn label_font(k: &Kit) -> TextStyle {
    TextStyle {
        line_height: 16.0,
        ..k.ty.ui_medium
    }
}

pub(crate) fn slug(label: &str) -> String {
    label.to_lowercase().replace(' ', "-")
}

/// A labelled control across the sidebar's width. The kit's Field starts its control at
/// the left at its own width, so a row that wraps or fills, such as the palettes or
/// Backdrop's Format, takes the row's width here.
pub(crate) fn field(cx: &Cx<Editor>, label: &str, control: El) -> El {
    Field::new()
        .label(label)
        .control(control.w_full())
        .build(kit(cx))
}

pub(crate) fn button(
    cx: &Cx<Editor>,
    key: &str,
    label: &str,
    enabled: bool,
    on_press: impl Fn(&mut Editor, &mut Tx) + 'static,
) -> El {
    let mut button = Button::new(label)
        .variant(ButtonVariant::Neutral)
        .disabled(!enabled);
    if enabled {
        button = button.on_press_tx(on_press);
    }
    button.build(kit(cx)).key(key.to_string())
}

/// A panel's title, with the button that closes it at the right.
pub(crate) fn heading(cx: &Cx<Editor>, title: &str, close: El) -> El {
    let k = kit(cx);
    row()
        .justify_between()
        .center_y()
        .child(
            text(title.to_string())
                .font(k.ty.body_medium)
                .color(k.c.text_base),
        )
        .child(close)
}

/// A line that explains the controls around it.
pub(crate) fn note(cx: &Cx<Editor>, content: impl Into<String>) -> El {
    let k = kit(cx);
    text(content.into())
        .wrap()
        .font(k.ty.ui)
        .color(k.c.text_muted)
}

/// A setting that is on or off, as a switch at the end of its labelled row, keyed `key`.
pub(crate) fn switch(
    cx: &Cx<Editor>,
    key: &str,
    label: &str,
    on: bool,
    toggle: impl Fn(&mut Editor, &mut Tx) + 'static,
) -> El {
    let k = kit(cx);
    row()
        .w_full()
        .justify_between()
        .center_y()
        .child(
            text(label.to_string())
                .font(label_font(k))
                .color(k.c.text_base)
                .a11y_hidden(),
        )
        .child(
            Switch::new(on)
                .label(label)
                .hide_label()
                .on_toggle_tx(toggle)
                .build(k)
                .key(key.to_string()),
        )
}

/// An option that is on or off, keyed `key`. The kit's checkbox sets its description on
/// one line, so a screen reader reads `description` and the panel shows it as a note.
pub(crate) fn checkbox(
    cx: &Cx<Editor>,
    key: &str,
    label: &str,
    description: &str,
    on: bool,
    toggle: impl Fn(&mut Editor, &mut Tx) + 'static,
) -> El {
    Checkbox::new(on)
        .label(label)
        .on_toggle_tx(toggle)
        .build(kit(cx))
        .description(description.to_string())
        .key(key.to_string())
}

/// One choice among several, named `label`. Three or fewer short options are a
/// segmented control, keyed `{group}-choices` with its segments `segment{i}`; more are
/// a Select, as [`select`] keys it.
pub(crate) fn choices<T>(
    s: &Editor,
    cx: &Cx<Editor>,
    layer: &mut Layer,
    group: &'static str,
    label: &str,
    options: impl IntoIterator<Item = (T, Action, bool)>,
) -> El
where
    T: Into<String>,
{
    let options: Vec<(String, Action, bool)> = options
        .into_iter()
        .map(|(label, action, on)| (label.into(), action, on))
        .collect();
    if options.len() > 3 {
        return select(s, cx, layer, group, label, options);
    }
    let labels: Vec<&str> = options.iter().map(|(l, ..)| l.as_str()).collect();
    let current = options.iter().position(|(.., on)| *on);
    let actions: Vec<Action> = options.iter().map(|(_, a, _)| a.clone()).collect();
    SegmentedControl::new(&labels, current)
        .label(label)
        .width(None)
        .on_pick(move |s: &mut Editor, i, tx| s.dispatch_ui(actions[i].clone(), tx))
        .build(kit(cx))
        .key(format!("{group}-choices"))
}

/// One choice among several in a list that opens under the field: its trigger
/// `{group}-trigger` and its options `{group}-content/…/option{i}`.
pub(crate) fn select<T>(
    s: &Editor,
    cx: &Cx<Editor>,
    layer: &mut Layer,
    group: &'static str,
    label: &str,
    options: impl IntoIterator<Item = (T, Action, bool)>,
) -> El
where
    T: Into<String>,
{
    let options: Vec<(String, Action, bool)> = options
        .into_iter()
        .map(|(label, action, on)| (label.into(), action, on))
        .collect();
    let labels: Vec<&str> = options.iter().map(|(l, ..)| l.as_str()).collect();
    let current = options.iter().position(|(.., on)| *on);
    let actions: Vec<Action> = options.iter().map(|(_, a, _)| a.clone()).collect();
    let names: Vec<String> = labels.iter().map(|l| l.to_string()).collect();
    Select::new(group, &labels)
        .label(label)
        .appearance(SelectAppearance::Field)
        .width(INNER)
        .current(current.map(|i| labels[i]))
        .open(s.choices_open == Some(group))
        .on_toggle(move |s: &mut Editor| s.toggle_choices(group))
        .on_close(|s: &mut Editor| s.choices_open = None)
        .on_choose(move |s: &mut Editor, chosen: &str, tx| {
            s.choices_open = None;
            if let Some(i) = names.iter().position(|n| n == chosen) {
                s.dispatch_ui(actions[i].clone(), tx);
            }
        })
        .build(kit(cx), layer)
}

impl Editor {
    /// Opens the list of the choice `group`, or closes it if it is open.
    pub(crate) fn toggle_choices(&mut self, group: &'static str) {
        self.choices_open = (self.choices_open != Some(group)).then_some(group);
    }
}

/// The kit stepper's height, and the side of each of its step buttons.
const STEPPER_H: f32 = 24.;

/// A number with step buttons; a press on the number opens it for typing.
pub(crate) fn number(
    s: &Editor,
    cx: &Cx<Editor>,
    label: &'static str,
    value: f32,
    unit: &'static str,
) -> El {
    let k = kit(cx);
    let (min, max, step) = super::number_limits(label);
    let key = slug(label);
    let shown = super::shown_number(value);
    let reading = if unit.is_empty() {
        shown.clone()
    } else {
        format!("{shown} {unit}")
    };
    let editing = s.number.as_ref().is_some_and(|d| d.label == label);
    let control: El = if editing {
        text_input(&s.number_edit, k.ty.ui)
            .key(format!("{key}-input"))
            .label(label)
            .single_line()
            .w(INNER)
            .h(STEPPER_H)
            .px(8.)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .on_edit(|s: &mut Editor| &mut s.number_edit)
            .on_submit(|s: &mut Editor, tx| {
                s.commit_number(tx);
                tx.blur();
            })
            .on_input_key(|s: &mut Editor, key, tx| {
                if key.key == Key::Escape {
                    s.number = None;
                    tx.blur();
                    return true;
                }
                false
            })
    } else {
        // The kit's stepper has no typed entry, so Lens lays a press over its number
        // that opens the number for typing, between the step buttons.
        el().key(format!("{key}-number"))
            .size(INNER, STEPPER_H)
            .child(
                NumberStepper::new(label, value as f64)
                    .range(min as f64, max as f64)
                    .step(step as f64)
                    .reading(reading)
                    .width(INNER)
                    .on_change(move |s: &mut Editor, next, tx| {
                        let action = s.number_action(label, next as f32);
                        s.number = None;
                        s.dispatch_ui(action, tx);
                    })
                    .build(k)
                    .key(format!("{key}-field")),
            )
            .child(
                el().key(format!("{key}-value"))
                    .role("button")
                    .label(format!("Type {label}"))
                    .absolute(STEPPER_H, 0.)
                    .size(INNER - 2. * STEPPER_H, STEPPER_H)
                    .cursor(Cursor::Text)
                    .on_press_tx(move |s: &mut Editor, tx| s.begin_number(label, value, tx)),
            )
    };
    field(cx, label, control)
}

/// What a slider asks for: the value, and whether it starts a new undo step.
pub(crate) type SliderAction = fn(u32, bool) -> Action;

/// A value along a track, with its reading beside the label. A drag is one undo step;
/// arrows, Page Up and Down, Home and End step it as a native slider does.
pub(crate) fn slider(
    cx: &Cx<Editor>,
    label: &str,
    value: u32,
    (min, max, step): (u32, u32, u32),
    shown: String,
    action: SliderAction,
) -> El {
    let k = kit(cx);
    let key = slug(label);
    col()
        .key(format!("{key}-field"))
        .w_full()
        .gap(8.)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(
                    text(label.to_string())
                        .font(label_font(k))
                        .color(k.c.text_base),
                )
                .child(
                    text(shown.clone())
                        .key(format!("{key}-reading"))
                        .font(k.ty.ui)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            Slider::new(label, value as f64, min as f64..=max as f64)
                .step(step as f64)
                .reading(shown)
                .on_change(move |s: &mut Editor, change, tx| {
                    s.dispatch_ui(action(change.value.round() as u32, !change.continues), tx)
                })
                .build(k)
                .key(format!("{key}-slider")),
        )
}
