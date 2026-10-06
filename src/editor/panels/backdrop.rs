use super::super::{
    Editor,
    actions::{Action, AnimationFormat, Panel},
    view::{HoverLabel, icon},
};
use crate::{
    animation::Motion,
    backdrop::{Backdrop, Control, Format, PRESETS},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Popup {
    Format,
    Export,
}

impl Editor {
    pub(in crate::editor) fn motion_button(
        &self,
        motion: Motion,
        active: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let name = match motion {
            Motion::Still => "square",
            Motion::Flow => "motion-flow",
            Motion::Nebula => "motion-starfield",
            Motion::Aurora => "motion-aurora",
            Motion::Contours => "motion-contours",
            Motion::Paint => "motion-painterly",
            Motion::Prism => "motion-prism",
            Motion::Liquid => "motion-liquid",
            Motion::Lava => "motion-lava",
        };
        let button = div()
            .id(motion.label())
            .debug_selector(move || format!("backdrop-motion-{}", motion.label()))
            .w(px(110.))
            .h(px(32.))
            .px_2()
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_md()
            .cursor_pointer()
            .text_sm()
            .bg(rgb(if active {
                theme.accent_background
            } else {
                theme.surface
            }))
            .text_color(rgb(if active {
                theme.accent
            } else {
                theme.secondary
            }))
            .hover(|s| s.bg(rgb(theme.hover)))
            .active(|s| s.bg(rgb(theme.pressed)))
            .child(
                svg()
                    .path(format!("icons/{name}.svg"))
                    .size(px(16.))
                    .flex_shrink_0()
                    .text_color(rgb(if active {
                        theme.accent
                    } else {
                        theme.secondary
                    })),
            )
            .child(motion.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch_ui(Action::SelectMotion { motion }, cx)
            }));
        self.accessible_button(
            motion.label(),
            true,
            Action::SelectMotion { motion },
            button,
        )
    }

    fn popup_count(&self, popup: Popup) -> usize {
        match popup {
            Popup::Format => Format::ALL.len(),
            Popup::Export if self.video_export.progress.is_some() => 1,
            Popup::Export => 3 + usize::from(self.video_export.last_video.is_some()),
        }
    }

    pub(in crate::editor) fn backdrop_slider(
        &self,
        control: Control,
        b: Backdrop,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let painted_bounds = bounds.clone();
        let value = control.value(b);
        let element = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .text_xs()
                    .child(div().whitespace_nowrap().child(control.label()))
                    .child(div().text_color(rgb(theme.muted)).child(format!(
                        "{value} {}",
                        if control == Control::Duration {
                            "s"
                        } else {
                            "px"
                        }
                    ))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("backdrop-{}", control.label())))
                    .debug_selector(move || format!("backdrop-{}", control.label()))
                    .h(px(24.))
                    .w_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let track = bounds.get();
                            this.dispatch_ui(
                                Action::BeginBackdropAdjustment {
                                    control,
                                    track: (
                                        f32::from(track.origin.x),
                                        f32::from(track.origin.y),
                                        f32::from(track.size.width),
                                        f32::from(track.size.height),
                                    ),
                                    position: (f32::from(e.position.x), f32::from(e.position.y)),
                                },
                                cx,
                            );
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                painted_bounds.set(bounds);
                                bounds
                            },
                            move |bounds, _, window, _| {
                                let y = bounds.center().y;
                                let x = bounds.left()
                                    + bounds.size.width
                                        * ((value - control.min()) as f32
                                            / (control.max() - control.min()) as f32);
                                let track = Bounds::new(
                                    point(bounds.left(), y - px(2.)),
                                    size(bounds.size.width, px(4.)),
                                );
                                window.paint_quad(quad(
                                    track,
                                    px(2.),
                                    rgb(theme.track),
                                    px(0.),
                                    rgb(theme.track),
                                    Default::default(),
                                ));
                                window.paint_quad(quad(
                                    Bounds::new(track.origin, size(x - bounds.left(), px(4.))),
                                    px(2.),
                                    rgb(theme.slider_fill),
                                    px(0.),
                                    rgb(theme.slider_fill),
                                    Default::default(),
                                ));
                                window.paint_quad(quad(
                                    Bounds::new(
                                        point(x - px(7.), y - px(7.)),
                                        size(px(14.), px(14.)),
                                    ),
                                    px(7.),
                                    rgb(theme.knob),
                                    px(1.),
                                    rgb(theme.knob_border),
                                    Default::default(),
                                ));
                            },
                        )
                        .size_full(),
                    ),
            );
        self.accessible_slider(
            control.label(),
            value as f64,
            (control.min() as f64, control.max() as f64, 1.),
            move |value| Action::SetBackdropControl {
                control,
                value: value.round() as u32,
            },
            element,
        )
    }
    pub(in crate::editor) fn open_popup(&mut self, popup: Popup, cx: &mut Context<Self>) {
        if self.is_busy() && self.video_export.progress.is_none() {
            return;
        }
        self.dispatch_ui(Action::CommitText, cx);
        self.panels.popup = if self.panels.popup == Some(popup) {
            None
        } else {
            Some(popup)
        };
        self.panels.popup_index = match popup {
            Popup::Format => Format::ALL
                .iter()
                .position(|f| {
                    *f == self
                        .document
                        .backdrop
                        .or(self.panels.backdrop_disabled)
                        .unwrap_or_default()
                        .format
                })
                .unwrap_or(0),
            Popup::Export => 0,
        };
        cx.notify();
    }
    pub(in crate::editor) fn choose_popup(
        &mut self,
        popup: Popup,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.panels.popup = None;
        match popup {
            Popup::Format => self.dispatch_ui(
                Action::SetBackdropFormat {
                    format: Format::ALL[index],
                },
                cx,
            ),
            Popup::Export => match index {
                0 if self.video_export.progress.is_some() => {
                    self.dispatch_ui(Action::CancelExport, cx)
                }
                0 => self.dispatch_ui(Action::SaveImage, cx),
                1 => self.dispatch_ui(
                    Action::ExportAnimation {
                        format: AnimationFormat::Mp4,
                    },
                    cx,
                ),
                2 => self.dispatch_ui(
                    Action::ExportAnimation {
                        format: AnimationFormat::Gif,
                    },
                    cx,
                ),
                _ => self.dispatch_ui(Action::RevealExport, cx),
            },
        }
        cx.notify();
    }
    pub(in crate::editor) fn popup_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(popup) = self.panels.popup else {
            return false;
        };
        match key {
            "escape" => self.panels.popup = None,
            "up" => {
                self.panels.popup_index = (self.panels.popup_index + self.popup_count(popup) - 1)
                    % self.popup_count(popup)
            }
            "down" => {
                self.panels.popup_index = (self.panels.popup_index + 1) % self.popup_count(popup)
            }
            "enter" => self.choose_popup(popup, self.panels.popup_index, cx),
            _ => {
                self.panels.popup = None;
                cx.notify();
                return false;
            }
        }
        cx.notify();
        true
    }
    fn accessible_popup_node(
        &self,
        popup: Popup,
        index: Option<usize>,
        label: String,
    ) -> crate::accessibility::Node {
        let sender = self.sender.clone();
        let scope = self.tool_scope();
        crate::accessibility::Node::button(
            label,
            true,
            Rc::new(move |request| {
                if matches!(request, crate::accessibility::Request::Press) {
                    let _ =
                        sender.try_send(crate::Message::AccessibilityPopup(scope, popup, index));
                }
            }),
        )
    }
    fn dropdown(&self, popup: Popup, trigger: AnyElement, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let trigger_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let painted_bounds = trigger_bounds.clone();
        let b = self
            .document
            .backdrop
            .or(self.panels.backdrop_disabled)
            .unwrap_or_default();
        let trigger = self.accessibility.element(
            trigger,
            self.accessible_popup_node(
                popup,
                None,
                if popup == Popup::Format {
                    format!("Format: {}", b.format.short_label())
                } else {
                    "Export".into()
                },
            ),
        );
        div()
            .relative()
            .flex_1()
            .flex_shrink_0()
            .child(trigger)
            .child(
                canvas(
                    move |bounds, _, _| {
                        painted_bounds.set(bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(self.panels.popup == Some(popup), |el| {
                el.child(
                    deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .position(point(px(0.), px(34.)))
                            .snap_to_window()
                            .child(
                                div()
                                    .id("backdrop-popup")
                                    .debug_selector(|| "backdrop-popup".into())
                                    .occlude()
                                    .w(px(230.))
                                    .p_1()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .shadow_md()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                                    )
                                    .on_mouse_down_out(cx.listener(
                                        move |this, e: &MouseDownEvent, _, cx| {
                                            if !trigger_bounds.get().contains(&e.position)
                                                && this.panels.popup == Some(popup)
                                            {
                                                this.panels.popup = None;
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .children((0..self.popup_count(popup)).map(|index| {
                                        let label = match popup {
                                            Popup::Format => Format::ALL[index].label(),
                                            Popup::Export => match index {
                                                0 if self.video_export.progress.is_some() => {
                                                    "Cancel export"
                                                }
                                                0 => "Save PNG…",
                                                1 => "Export MP4…",
                                                2 => "Export GIF…",
                                                _ => "Show exported animation",
                                            },
                                        };
                                        div()
                                            .child(
                                                self.accessibility.element(
                                                    div()
                                                        .id(("popup-option", index))
                                                        .debug_selector(move || {
                                                            format!("popup-option-{index}")
                                                        })
                                                        .h(px(28.))
                                                        .px_2()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .rounded_md()
                                                        .text_xs()
                                                        .cursor_pointer()
                                                        .bg(rgb(
                                                            if self.panels.popup_index == index {
                                                                theme.positive_background
                                                            } else {
                                                                theme.surface
                                                            },
                                                        ))
                                                        .hover(|s| {
                                                            s.bg(rgb(theme.positive_background))
                                                        })
                                                        .child(div().w(px(12.)).child(
                                                            if popup == Popup::Format
                                                                && b.format == Format::ALL[index]
                                                            {
                                                                "✓"
                                                            } else {
                                                                ""
                                                            },
                                                        ))
                                                        .child(label)
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.choose_popup(popup, index, cx)
                                                            },
                                                        )),
                                                    self.accessible_popup_node(
                                                        popup,
                                                        Some(index),
                                                        label.into(),
                                                    ),
                                                ),
                                            )
                                            .when(popup == Popup::Format && index == 6, |el| {
                                                el.child(
                                                    div().h(px(1.)).my_1().bg(rgb(theme.divider)),
                                                )
                                            })
                                    })),
                            ),
                    )
                    .with_priority(2),
                )
            })
    }
    pub(in crate::editor) fn export_menu(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        // A single toolbar control replaces the separate sidebar export buttons.
        div()
            .id("export-menu")
            .debug_selector(|| "export-menu".into())
            .flex_shrink_0()
            .w(px(40.))
            .child(
                self.dropdown(
                    Popup::Export,
                    div()
                        .id("export-trigger")
                        .debug_selector(|| "export-trigger".into())
                        .h(px(30.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_1()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme.hover)))
                        .child(icon(
                            if self.video_export.progress.is_some() {
                                "square"
                            } else {
                                "save"
                            },
                            theme.secondary,
                        ))
                        .child("▾")
                        .tooltip(|_, cx| {
                            cx.new(|_| HoverLabel("Export PNG / MP4 / GIF · ⌘S saves PNG".into()))
                                .into()
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.dispatch_ui(Action::CommitText, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.open_popup(Popup::Export, cx)))
                        .into_any_element(),
                    cx,
                ),
            )
    }
    pub(in crate::editor) fn backdrop_controls(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let b = self
            .document
            .backdrop
            .or(self.panels.backdrop_disabled)
            .unwrap_or_default();
        for (picker, color) in self.color_pickers.iter().zip(b.colors()) {
            let color = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
            picker.update(cx, |picker, cx| picker.set_value(color, cx));
        }
        super::controls::panel("backdrop-panel", cx)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Backdrop"),
                    )
                    .child(self.button(
                        "Done",
                        false,
                        cx,
                        Action::ClosePanel {
                            panel: Panel::Backdrop,
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .child("Format")
                    .child(
                        self.dropdown(
                            Popup::Format,
                            div()
                                .id("backdrop-format")
                                .debug_selector(|| "backdrop-format".into())
                                .h(px(32.))
                                .px_2()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_1()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .bg(rgb(theme.surface))
                                .cursor_pointer()
                                .child(b.format.short_label())
                                .child("▾")
                                .on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.open_popup(Popup::Format, cx)
                                    }),
                                )
                                .into_any_element(),
                            cx,
                        ),
                    ),
            )
            .child(
                div()
                    .id("backdrop-effects")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(
                        [
                            [Control::Padding, Control::InsidePadding],
                            [Control::InnerRadius, Control::Shadow],
                        ]
                        .into_iter()
                        .map(|row| {
                            div()
                                .flex()
                                .gap_3()
                                .children(row.into_iter().map(|control| {
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .child(self.backdrop_slider(control, b, cx))
                                }))
                        }),
                    ),
            )
            .child(div().h(px(1.)).bg(rgb(theme.divider)))
            .child(div().text_xs().child("Background"))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(theme.workspace))
                    .children(
                        [("Solid", false), ("Gradient", true), ("Motion", true)]
                            .into_iter()
                            .map(|(label, gradient)| {
                                let moving = label == "Motion";
                                let active = if moving {
                                    b.motion != Motion::Still
                                } else {
                                    b.motion == Motion::Still && b.gradient == gradient
                                };
                                let action = if moving { Action::SelectMotion { motion: if b.motion == Motion::Still { Motion::Flow } else { b.motion } } } else { Action::SetBackdropFill { gradient } };
                                let button = div()
                                    .id(label)
                                    .debug_selector(move || format!("backdrop-mode-{label}"))
                                    .flex_1()
                                    .h(px(30.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_md()
                                    .text_xs()
                                    .cursor_pointer()
                                    .bg(rgb(if active { theme.surface } else { theme.workspace }))
                                    .text_color(rgb(if active { theme.text } else { theme.muted }))
                                    .on_click(cx.listener({ let action = action.clone(); move |this, _, _, cx| this.dispatch_ui(action.clone(), cx) }))
                                    .child(label);
                                self.accessible_button(label, true, action, button)
                            }),
                    ),
            )
            .when(b.motion != Motion::Still, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_xs().child("Effect"))
                        .child(
                            div()
                                .id("backdrop-randomize")
                                .debug_selector(|| "backdrop-randomize".into())
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .tooltip(|_, cx| {
                                    cx.new(|_| {
                                        HoverLabel(
                                            "Try another looping variation. Undo restores the previous one.".into(),
                                        )
                                    })
                                    .into()
                                })
                                .child(self.button(
                                    "Randomize",
                                    false,
                                    cx,
                                    Action::RandomizeMotion { seed: None },
                                )),
                        ),
                )
                .child(
                    div()
                        .id("backdrop-motion-effects")
                        .debug_selector(|| "backdrop-motion-effects".into())
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(
                            Motion::EFFECTS
                                .into_iter()
                                .map(|motion| self.motion_button(motion, b.motion == motion, cx)),
                        ),
                )
            })
            .child(
                div()
                    .id("backdrop-presets")
                    .debug_selector(|| "backdrop-presets".into())
                    .flex()
                    .w_full()
                    .gap_1()
                    .children(PRESETS.iter().enumerate().map(|(i, &(name, _, _))| {
                        let style = Backdrop {
                            preset: i,
                            colors: None,
                            ..b
                        };
                        let button = div()
                            .id(("backdrop-preset", i))
                            .debug_selector(move || format!("backdrop-preset-{i}"))
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(24.))
                            .rounded_md()
                            .border_2()
                            .border_color(rgb(if i == b.preset && b.colors.is_none() {
                                theme.positive
                            } else {
                                theme.divider
                            }))
                            .bg(style.background())
                            .cursor_pointer()
                            .tooltip(move |_, cx| cx.new(|_| HoverLabel(name.into())).into())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.dispatch_ui(Action::SetBackdropPreset { preset: i }, cx)
                            }));
                        self.accessible_button(format!("Palette {name}"), true, Action::SetBackdropPreset { preset: i }, button)
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .child(if b.gradient || b.motion != Motion::Still {
                        "Colors"
                    } else {
                        "Color"
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.accessible_color("Backdrop color", b.colors()[0].to_be_bytes()[1..].try_into().unwrap(), |rgb| Action::SetBackdropColor { stop: 0, rgb }, self.color_pickers[0].clone()))
                            .when(b.gradient || b.motion != Motion::Still, |el| {
                                el.child(self.accessible_color("Backdrop color 2", b.colors()[1].to_be_bytes()[1..].try_into().unwrap(), |rgb| Action::SetBackdropColor { stop: 1, rgb }, self.color_pickers[1].clone()))
                            }),
                    ),
            )
            .when(b.motion != Motion::Still, |el| {
                el.child(
                    div()
                        .id("backdrop-duration")
                        .debug_selector(|| "backdrop-duration".into())
                        .flex()
                        .items_end()
                        .gap_2()
                        .child(div().flex_1().pb_1().child(self.backdrop_slider(
                            Control::Duration,
                            b,
                            cx,
                        )))
                        .child(
                            div()
                                .id("backdrop-playback")
                                .debug_selector(|| "backdrop-playback".into())
                                .flex_shrink_0()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(theme.knob_border))
                                .child(self.compact_button(
                                    if self.playback.paused {
                                        "Play"
                                    } else {
                                        "Pause"
                                    },
                                    if self.playback.paused {
                                        "play"
                                    } else {
                                        "pause"
                                    },
                                    false,
                                    cx,
                                    Action::TogglePlayback,
                                )),
                        ),
                )
            })
            .child(div().h(px(1.)).bg(rgb(theme.divider)))
            .child(
                self.accessible_button("Enable backdrop", true, Action::ToggleBackdropEnabled, div()
                    .id("backdrop-enabled")
                    .debug_selector(|| "backdrop-enabled".into())
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(14.))
                            .rounded_sm()
                            .border_1()
                            .border_color(rgb(theme.positive))
                            .bg(rgb(if self.document.backdrop.is_some() {
                                theme.positive_fill
                            } else {
                                theme.surface
                            }))
                            .text_color(rgb(theme.on_fill))
                            .child(if self.document.backdrop.is_some() {
                                "✓"
                            } else {
                                ""
                            }),
                    )
                    .child("Enable backdrop")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dispatch_ui(Action::ToggleBackdropEnabled, cx);
                    }))),
            )
    }
}
