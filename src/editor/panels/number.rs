//! Small native numeric field. Draft text stays local until Enter or blur.
mod input;
use crate::text::Buffer;
use gpui::{prelude::*, *};
use std::ops::Range;
pub(in crate::editor) const FIELDS: [&str; 11] = [
    "Thickness",
    "Font size",
    "Block size",
    "Badge diameter",
    "Number",
    "Next number",
    "Lens diameter",
    "Dim surroundings",
    "Opacity",
    "Intensity",
    "Corner radius",
];

pub(in crate::editor) type Scope = (u64, usize, Option<usize>, u64);
pub(in crate::editor) struct Changed {
    pub value: f32,
    pub scope: Scope,
}
pub(in crate::editor) struct NumberInput {
    label: &'static str,
    unit: &'static str,
    value: f32,
    limits: (f32, f32, f32),
    scope: Scope,
    focus: FocusHandle,
    return_focus: FocusHandle,
    buffer: Buffer,
    editing: bool,
    error: bool,
    input_bounds: Bounds<Pixels>,
    input_line: Option<ShapedLine>,
    _blur: Option<Subscription>,
}
impl EventEmitter<Changed> for NumberInput {}
impl NumberInput {
    pub fn new(label: &'static str, return_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        Self {
            label,
            unit: "",
            value: 0.,
            limits: (0., 1., 1.),
            scope: (0, 0, None, 0),
            focus: cx.focus_handle(),
            return_focus,
            buffer: Buffer::default(),
            editing: false,
            error: false,
            input_bounds: Bounds::default(),
            input_line: None,
            _blur: None,
        }
    }
    pub fn set_value(
        &mut self,
        value: f32,
        unit: &'static str,
        limits: (f32, f32, f32),
        scope: Scope,
        cx: &mut Context<Self>,
    ) {
        if self.value != value || self.scope != scope {
            self.value = value;
            self.scope = scope;
            self.editing = false;
            self.reset();
            cx.notify();
        }
        self.unit = unit;
        self.limits = limits;
    }
    fn reset(&mut self) {
        self.buffer = Buffer::default();
        self.buffer.replace(None, &display(self.value));
        self.error = false;
    }
    pub fn has_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    fn commit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(value) = parse(self.buffer.text(), self.limits) else {
            self.error = true;
            cx.notify();
            return false;
        };
        self.editing = false;
        if value != self.value {
            self.value = value;
            cx.emit(Changed {
                value,
                scope: self.scope,
            });
        }
        self.reset();
        cx.notify();
        true
    }
    fn step(&mut self, more: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (min, max, step) = self.limits;
        let base = if self.editing {
            parse(self.buffer.text(), self.limits).unwrap_or(self.value)
        } else {
            self.value
        };
        let value = (base + if more { step } else { -step }).clamp(min, max);
        self.editing = false;
        if value != self.value {
            self.value = value;
            cx.emit(Changed {
                value,
                scope: self.scope,
            });
        }
        self.reset();
        self.return_focus.focus(window);
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if crate::platform::command_pressed(modifiers) {
            match key {
                "a" => self.buffer.select_all(),
                "c" | "x" => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.buffer.text()[self.buffer.selection()].to_string(),
                    ));
                    if key == "x" {
                        self.buffer.delete(false);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace_text_in_range(None, &text, window, cx);
                    }
                }
                "z" => self.buffer.history(modifiers.shift),
                _ => return,
            }
        } else {
            match key {
                "enter" if self.buffer.marked.is_some() => return,
                "enter" => {
                    if self.commit(cx) {
                        self.return_focus.focus(window);
                    }
                }
                "escape" => {
                    self.editing = false;
                    self.reset();
                    self.return_focus.focus(window);
                }
                "up" | "down" => self.step(key == "up", window, cx),
                "backspace" => self.buffer.delete(false),
                "delete" => self.buffer.delete(true),
                "left" | "right" => self.buffer.move_cursor(key == "right", modifiers.shift),
                "home" => self.buffer.move_to(0, modifiers.shift),
                "end" => self
                    .buffer
                    .move_to(self.buffer.text().len(), modifiers.shift),
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}
fn display(value: f32) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
fn parse(text: &str, (min, max, _): (f32, f32, f32)) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && (min..=max).contains(value))
}
impl Render for NumberInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        if self._blur.is_none() {
            self._blur = Some(cx.on_blur(&self.focus, window, |this, _, cx| {
                if this.editing && !this.commit(cx) {
                    this.editing = false;
                    this.reset();
                    cx.notify();
                }
            }));
        }
        let entity = cx.entity();
        let id = self.label.to_lowercase().replace(' ', "-");
        div()
            .id(SharedString::from(format!("{id}-field")))
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-field")
            })
            .track_focus(&self.focus)
            .key_context("NumberInput")
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|this, _: &crate::menus::Copy, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    this.buffer.text()[this.buffer.selection()].to_string(),
                ));
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Paste, window, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    this.replace_text_in_range(None, &text, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Undo, _, cx| {
                this.buffer.history(false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Redo, _, cx| {
                this.buffer.history(true);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Delete, _, cx| {
                this.buffer.delete(false);
                cx.notify();
            }))
            .h(px(30.))
            .w_full()
            .flex()
            .items_center()
            .rounded_md()
            .bg(rgb(theme.input))
            .border_1()
            .border_color(rgb(if self.error { theme.error } else { theme.input }))
            .child(
                div()
                    .id(SharedString::from(format!("{id}-value")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-value")
                    })
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.editing = true;
                            this.buffer.select_all();
                            this.focus.focus(window);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, cx| input::paint(&entity, bounds, window, cx),
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child(self.unit),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .ml_1()
                    .children([true, false].map(|more| {
                        let name = format!("{id}-{}", if more { "more" } else { "less" });
                        let enabled = if more {
                            self.value < self.limits.1
                        } else {
                            self.value > self.limits.0
                        };
                        div()
                            .id(SharedString::from(name.clone()))
                            .debug_selector(move || name.clone())
                            .w(px(18.))
                            .h(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(rgb(if enabled {
                                theme.secondary
                            } else {
                                theme.disabled
                            }))
                            .child(if more { "▴" } else { "▾" })
                            .when(enabled, |el| {
                                el.cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme.pressed)))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.step(more, window, cx)
                                    }))
                            })
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{display, parse};
    #[test]
    fn numeric_drafts_reject_nonfinite_and_out_of_range_values() {
        for text in ["", "NaN", "inf", "0", "33", "oops"] {
            assert_eq!(parse(text, (1., 32., 1.)), None);
        }
        assert_eq!(parse(" 2.5 ", (1., 32., 1.)), Some(2.5));
        assert_eq!(display(100.), "100");
        assert_eq!(display(7.2), "7.2");
    }
}
