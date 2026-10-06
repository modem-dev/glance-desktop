//! Reusable opaque RGB picker. Consumers subscribe to semantic events; the
//! component owns its popup, native hex input, wheel and brightness gesture.
mod input;
use crate::text::Buffer;
use gpui::{prelude::*, *};
use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

pub type Rgb = [u8; 3];
pub enum ColorPickerEvent {
    Changed(Rgb),
    PickScreen,
}
impl EventEmitter<ColorPickerEvent> for ColorPicker {}

struct PickerLabel(SharedString);
impl Render for PickerLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .bg(rgb(theme.tooltip))
            .text_color(rgb(theme.tooltip_text))
            .child(self.0.clone())
    }
}

pub fn parse_hex(text: &str) -> Option<Rgb> {
    let text = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !text.is_ascii() || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match text.len() {
        3 => Some(std::array::from_fn(|i| {
            u8::from_str_radix(&text[i..i + 1], 16).unwrap() * 17
        })),
        6 => Some(std::array::from_fn(|i| {
            u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()
        })),
        _ => None,
    }
}
fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}
fn packed([r, g, b]: Rgb) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}
fn hsv([r, g, b]: Rgb) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|v| v as f32 / 255.);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0. {
        0.
    } else if max == r {
        ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    } / 6.;
    [h, if max == 0. { 0. } else { d / max }, max]
}
fn from_hsv([h, s, v]: [f32; 3]) -> Rgb {
    let h = h.rem_euclid(1.) * 6.;
    let c = v * s;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let rgb = match h as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    rgb.map(|a| ((a + v - c) * 255.).round() as u8)
}
fn wheel_image() -> Arc<RenderImage> {
    let image = image::RgbaImage::from_fn(384, 384, |x, y| {
        let dx = (x as f32 + 0.5 - 192.) / 190.;
        let dy = (y as f32 + 0.5 - 192.) / 190.;
        let radius = dx.hypot(dy);
        let [r, g, b] = from_hsv([dy.atan2(dx) / std::f32::consts::TAU, radius.min(1.), 1.]);
        image::Rgba([
            b,
            g,
            r,
            ((1. - radius) * 190. + 0.5)
                .clamp(0., 1.)
                .mul_add(255., 0.)
                .round() as u8,
        ])
    });
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
        image
    )]))
}
#[derive(Clone, Copy)]
enum Drag {
    Wheel(Bounds<Pixels>),
    Brightness(Bounds<Pixels>),
}

pub struct ColorPicker {
    label: &'static str,
    external: Rgb,
    hsv: [f32; 3],
    open: bool,
    editing: bool,
    focus: FocusHandle,
    return_focus: FocusHandle,
    buffer: Buffer,
    input_bounds: Bounds<Pixels>,
    input_line: Option<ShapedLine>,
    error: bool,
    drag: Option<Drag>,
    wheel: Option<Arc<RenderImage>>,
}
impl ColorPicker {
    pub fn new(label: &'static str, return_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        let task = cx.background_executor().spawn(async { wheel_image() });
        cx.spawn(async move |entity, cx| {
            let wheel = task.await;
            let _ = entity.update(cx, |this, cx| {
                this.wheel = Some(wheel);
                cx.notify();
            });
        })
        .detach();
        let mut buffer = Buffer::default();
        buffer.replace(None, "#000000");
        Self {
            label,
            external: [0; 3],
            hsv: [0.; 3],
            open: false,
            editing: false,
            focus: cx.focus_handle(),
            return_focus,
            buffer,
            input_bounds: Bounds::default(),
            input_line: None,
            error: false,
            drag: None,
            wheel: None,
        }
    }
    /// Sync model changes without resetting an uncommitted wheel or hex gesture.
    pub fn set_value(&mut self, value: Rgb, cx: &mut Context<Self>) {
        if self.external != value {
            self.external = value;
            self.hsv = hsv(value);
            self.reset_hex();
            self.drag = None;
            cx.notify();
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn has_focus(&self, window: &Window) -> bool {
        self.open && self.focus.is_focused(window)
    }
    pub fn close_if_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open() {
            self.close(window, cx);
        }
    }
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.drag = None;
        self.hsv = hsv(self.external);
        self.reset_hex();
        if self.focus.is_focused(window) {
            self.return_focus.focus(window);
        }
        cx.notify();
    }
    fn reset_hex(&mut self) {
        self.buffer = Buffer::default();
        self.buffer.replace(None, &hex(from_hsv(self.hsv)));
        self.error = false;
    }
    fn commit(&mut self, cx: &mut Context<Self>) {
        let value = from_hsv(self.hsv);
        if self.external != value {
            self.external = value;
            cx.emit(ColorPickerEvent::Changed(value));
        }
        cx.notify();
    }
    fn apply_hex(&mut self, cx: &mut Context<Self>) {
        if let Some(value) = parse_hex(self.buffer.text()) {
            self.hsv = hsv(value);
            self.reset_hex();
            self.commit(cx);
        } else {
            self.error = true;
            cx.notify();
        }
    }
    fn move_drag(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Wheel(bounds)) => {
                let radius = f32::from(bounds.size.width) * 190. / 384.;
                let dx = f32::from(p.x - bounds.center().x) / radius;
                let dy = f32::from(p.y - bounds.center().y) / radius;
                self.hsv[0] = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.);
                self.hsv[1] = dx.hypot(dy).min(1.);
            }
            Some(Drag::Brightness(bounds)) => {
                self.hsv[2] =
                    (f32::from(p.x - bounds.left()) / f32::from(bounds.size.width)).clamp(0., 1.)
            }
            None => return,
        }
        self.reset_hex();
        cx.notify();
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            cx.stop_propagation();
            self.close(window, cx);
            return;
        }
        if key == "tab" {
            cx.stop_propagation();
            self.editing = true;
            self.buffer.select_all();
            cx.notify();
            return;
        }
        if !self.editing {
            cx.stop_propagation();
            return;
        }
        self.error = false;
        let command = crate::platform::command_pressed(event.keystroke.modifiers);
        if command {
            cx.stop_propagation();
            match key {
                "a" => self.buffer.select_all(),
                "c" | "x" => {
                    let text = self.buffer.text()[self.buffer.selection()].to_string();
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    if key == "x" {
                        self.buffer.delete(false);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace_text_in_range(None, &text, window, cx);
                    }
                }
                "z" => self.buffer.history(event.keystroke.modifiers.shift),
                _ => {}
            }
        } else {
            match key {
                "enter" => self.apply_hex(cx),
                "backspace" => self.buffer.delete(false),
                "delete" => self.buffer.delete(true),
                "left" => self
                    .buffer
                    .move_cursor(false, event.keystroke.modifiers.shift),
                "right" => self
                    .buffer
                    .move_cursor(true, event.keystroke.modifiers.shift),
                "home" => self.buffer.move_to(0, event.keystroke.modifiers.shift),
                "end" => self
                    .buffer
                    .move_to(self.buffer.text().len(), event.keystroke.modifiers.shift),
                _ => return,
            }
            cx.stop_propagation();
        }
        cx.notify();
    }
    fn surface(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let wheel_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let wheel_paint_bounds = wheel_bounds.clone();
        let brightness_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let brightness_paint_bounds = brightness_bounds.clone();
        let wheel = self.wheel.clone();
        let hsv = self.hsv;
        let entity = cx.entity();
        let drag_entity = cx.entity();
        div()
            .relative()
            .track_focus(&self.focus)
            .key_context("ColorPicker")
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|this, _: &crate::menus::Copy, _, cx| {
                if this.editing {
                    let text = this.buffer.text()[this.buffer.selection()].to_string();
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Paste, window, cx| {
                if this.editing
                    && let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                {
                    this.replace_text_in_range(None, &text, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Undo, _, cx| {
                if this.editing {
                    this.buffer.history(false);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Redo, _, cx| {
                if this.editing {
                    this.buffer.history(true);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Delete, _, cx| {
                if this.editing {
                    this.buffer.delete(false);
                    cx.notify();
                }
            }))
            .id("color-picker-popup")
            .debug_selector(|| "color-picker-popup".into())
            .occlude()
            .w(px(240.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .shadow_md()
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let entity = drag_entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                entity.update(cx, |this, cx| {
                                    if this.drag.is_some() {
                                        this.move_drag(event.position, cx);
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        let entity = drag_entity.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                entity.update(cx, |this, cx| {
                                    if this.drag.is_some() {
                                        this.move_drag(event.position, cx);
                                        this.drag = None;
                                        this.commit(cx);
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_sm()
                    .child(self.label)
                    .child(
                        div()
                            .id("color-picker-close")
                            .cursor_pointer()
                            .child("✕")
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("color-picker-wheel")
                    .debug_selector(|| "color-picker-wheel".into())
                    .size(px(208.))
                    .cursor(CursorStyle::Crosshair)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            this.editing = false;
                            this.focus.focus(window);
                            this.drag = Some(Drag::Wheel(wheel_bounds.get()));
                            this.move_drag(e.position, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                wheel_paint_bounds.set(bounds);
                                bounds
                            },
                            move |bounds, _, window, _| {
                                if let Some(wheel) = wheel {
                                    let _ = window.paint_image(
                                        bounds,
                                        Corners::default(),
                                        wheel,
                                        0,
                                        false,
                                    );
                                }
                                let radius = bounds.size.width * (190. / 384.);
                                let angle = hsv[0] * std::f32::consts::TAU;
                                let p = bounds.center()
                                    + point(
                                        radius * (hsv[1] * angle.cos()),
                                        radius * (hsv[1] * angle.sin()),
                                    );
                                window.paint_quad(quad(
                                    Bounds::new(p - point(px(6.), px(6.)), size(px(12.), px(12.))),
                                    px(6.),
                                    rgb(packed(from_hsv([hsv[0], hsv[1], 1.]))),
                                    px(2.),
                                    rgb(theme.knob),
                                    Default::default(),
                                ));
                                window.paint_quad(quad(
                                    Bounds::new(p - point(px(7.), px(7.)), size(px(14.), px(14.))),
                                    px(7.),
                                    transparent_black(),
                                    px(1.),
                                    rgb(0x333333),
                                    Default::default(),
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(div().text_xs().child("Brightness"))
            .child(
                div()
                    .id("color-picker-brightness")
                    .debug_selector(|| "color-picker-brightness".into())
                    .h(px(20.))
                    .w_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            this.editing = false;
                            this.focus.focus(window);
                            this.drag = Some(Drag::Brightness(brightness_bounds.get()));
                            this.move_drag(e.position, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                brightness_paint_bounds.set(bounds);
                                bounds
                            },
                            move |bounds, _, window, _| {
                                let track = Bounds::new(
                                    point(bounds.left(), bounds.center().y - px(4.)),
                                    size(bounds.size.width, px(8.)),
                                );
                                window.paint_quad(quad(
                                    track,
                                    px(4.),
                                    linear_gradient(
                                        90.,
                                        linear_color_stop(rgb(0), 0.),
                                        linear_color_stop(
                                            rgb(packed(from_hsv([hsv[0], hsv[1], 1.]))),
                                            1.,
                                        ),
                                    ),
                                    px(0.),
                                    rgb(0),
                                    Default::default(),
                                ));
                                window.paint_quad(quad(
                                    Bounds::new(
                                        point(
                                            bounds.left() + bounds.size.width * hsv[2] - px(6.),
                                            bounds.center().y - px(6.),
                                        ),
                                        size(px(12.), px(12.)),
                                    ),
                                    px(6.),
                                    rgb(theme.knob),
                                    px(1.),
                                    rgb(0x777777),
                                    Default::default(),
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(24.))
                            .rounded_sm()
                            .bg(rgb(packed(from_hsv(self.hsv)))),
                    )
                    .child(
                        div()
                            .id("color-picker-hex")
                            .debug_selector(|| "color-picker-hex".into())
                            .flex_1()
                            .h(px(30.))
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if self.error {
                                theme.error
                            } else {
                                theme.border
                            }))
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
                                    move |bounds, _, window, cx| {
                                        input::paint(&entity, bounds, window, cx)
                                    },
                                )
                                .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .id("color-picker-apply")
                            .debug_selector(|| "color-picker-apply".into())
                            .text_xs()
                            .cursor_pointer()
                            .child("Apply")
                            .on_click(cx.listener(|this, _, _, cx| this.apply_hex(cx))),
                    ),
            )
            .when(self.error, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(rgb(theme.error))
                        .child("Enter #RGB or #RRGGBB"),
                )
            })
            .child(
                div()
                    .id("Pick from screen")
                    .debug_selector(|| "Pick from screen".into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p_2()
                    .text_xs()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path("icons/pipette.svg")
                            .size(px(16.))
                            .flex_shrink_0()
                            .text_color(rgb(theme.text)),
                    )
                    .child("Pick from screen")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close(window, cx);
                        cx.emit(ColorPickerEvent::PickScreen);
                    })),
            )
    }
}
impl Render for ColorPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let label = self.label;
        let selector = format!(
            "color-picker-{}",
            self.label.replace(' ', "-").to_lowercase()
        );
        let trigger_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let painted_bounds = trigger_bounds.clone();
        div()
            .relative()
            .when(!self.open, |el| {
                el.track_focus(&self.focus)
                    .on_key_down(cx.listener(Self::key))
            })
            .child(
                div()
                    .id("color-picker-trigger")
                    .debug_selector(move || selector.clone())
                    .size(px(34.))
                    .p(px(3.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if self.open {
                        theme.positive
                    } else {
                        theme.border
                    }))
                    .bg(rgb(theme.surface))
                    .hover(|style| style.border_color(rgb(theme.knob_border)))
                    .cursor_pointer()
                    .tooltip(move |_, cx| {
                        cx.new(|_| PickerLabel(format!("Edit {label}").into()))
                            .into()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.open {
                            this.close(window, cx);
                        } else {
                            this.open = true;
                            this.editing = false;
                            this.focus.focus(window);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .size_full()
                            .rounded_sm()
                            .bg(rgb(packed(self.external)))
                            .border_1()
                            .border_color(rgb(theme.border)),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| painted_bounds.set(bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
            .when(self.open, |el| {
                el.child(
                    deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .position(point(px(0.), px(38.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(
                                div()
                                    .on_mouse_down_out(cx.listener(
                                        move |this, e: &MouseDownEvent, window, cx| {
                                            if !trigger_bounds.get().contains(&e.position)
                                                && this.drag.is_none()
                                            {
                                                this.close(window, cx);
                                            }
                                        },
                                    ))
                                    .child(self.surface(cx)),
                            ),
                    )
                    .with_priority(3),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{from_hsv, hsv, parse_hex};
    #[gpui::test]
    fn native_hex_preedit_replaces_marked_range_and_rejects_invalid_colors(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::EntityInputHandler;
        let view = cx.add_window(|window, cx| {
            let parent = cx.focus_handle();
            let mut picker = super::ColorPicker::new("Color", parent, cx);
            picker.open = true;
            picker.editing = true;
            picker.focus.focus(window);
            picker
        });
        view.update(cx, |picker, window, cx| {
            picker.buffer.select_all();
            picker.replace_and_mark_text_in_range(None, "é", Some(1..1), window, cx);
            assert_eq!(picker.buffer.marked, Some(0..2));
            picker.replace_and_mark_text_in_range(None, "éa", Some(2..2), window, cx);
            assert_eq!(picker.buffer.text(), "éa");
            assert_eq!(picker.buffer.marked, Some(0..3));
            picker.apply_hex(cx);
            assert!(picker.error);
            assert_eq!(picker.external, [0; 3]);
            picker.replace_text_in_range(Some(0..2), "#FF0080", window, cx);
            assert!(picker.buffer.marked.is_none());
            picker.apply_hex(cx);
            assert_eq!(picker.external, [255, 0, 128]);
        })
        .unwrap();
    }
    #[test]
    fn hex_validation_and_hsv_round_trip() {
        assert_eq!(parse_hex(" #0aF "), Some([0, 170, 255]));
        assert_eq!(parse_hex("abcdef"), Some([171, 205, 239]));
        for text in ["", "#", "#12", "#abcd", "#11223344", "＃123456", "#xyzxyz"] {
            assert_eq!(parse_hex(text), None);
        }
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    assert_eq!(from_hsv(hsv([r, g, b])), [r, g, b]);
                }
            }
        }
    }
}
