//! The Backdrop panel: format, padding, corners, shadow, the background's fill and
//! motion, its palette and colours, and the motion's cycle.
use super::{
    super::{Editor, actions::Action, live::rgba},
    controls,
};
use crate::{
    animation::Motion,
    backdrop::{Control, Format, PRESETS},
};
use rune::{Cursor, Cx, El, PopupMenu, col, el, row};
use rune_kit::{Select, SelectAppearance};

/// A stable id for a format in a pop-up menu.
fn format_id(format: Format) -> String {
    format!("{format:?}").to_lowercase()
}

fn control_slider(cx: &Cx<Editor>, control: Control, value: u32) -> El {
    let unit = if control == Control::Duration {
        "s"
    } else {
        "px"
    };
    let action: controls::SliderAction = match control {
        Control::Padding => |value, remember| Action::SetBackdropControl {
            control: Control::Padding,
            value,
            remember,
        },
        Control::InsidePadding => |value, remember| Action::SetBackdropControl {
            control: Control::InsidePadding,
            value,
            remember,
        },
        Control::InnerRadius => |value, remember| Action::SetBackdropControl {
            control: Control::InnerRadius,
            value,
            remember,
        },
        Control::Shadow => |value, remember| Action::SetBackdropControl {
            control: Control::Shadow,
            value,
            remember,
        },
        Control::Duration => |value, remember| Action::SetBackdropControl {
            control: Control::Duration,
            value,
            remember,
        },
    };
    controls::slider(
        cx,
        control.label(),
        value,
        (control.min(), control.max(), 1),
        format!("{value} {unit}"),
        action,
    )
}

impl Editor {
    pub(super) fn backdrop_controls(&self, cx: &mut Cx<Editor>, layer: &mut controls::Layer) -> El {
        let k = controls::kit(cx);
        let enabled = self.doc.as_ref().is_some_and(|d| d.backdrop.is_some());
        let b = self.current_backdrop();
        let moving = b.motion != Motion::Still;
        // Format keeps the native pop-up menu: the kit's field is its trigger.
        let labels = Format::ALL.map(|f| f.short_label());
        let format = Select::new("backdrop-format", &labels)
            .label("Format")
            .appearance(SelectAppearance::Field)
            .width(controls::INNER)
            .current(Some(b.format.short_label()))
            .trigger(k)
            .key("backdrop-format")
            .a11y_value(b.format.label())
            .cursor(Cursor::Pointer)
            .on_press_tx(move |_: &mut Editor, tx| {
                let menu = PopupMenu::picker(
                    Format::ALL.map(|f| (format_id(f), f.label())),
                    &format_id(b.format),
                );
                tx.popup_menu(menu, "backdrop-format", |s: &mut Editor, picked, tx| {
                    if let Some(format) = Format::ALL
                        .into_iter()
                        .find(|f| Some(format_id(*f).as_str()) == picked)
                    {
                        s.dispatch_ui(Action::SetBackdropFormat(format), tx);
                    }
                });
            });
        let mut panel = col()
            .key("backdrop-panel")
            .gap(14.)
            .child(controls::heading(
                cx,
                "Backdrop",
                controls::button(cx, "backdrop-done", "Done", true, |s: &mut Editor, tx| {
                    s.dispatch_ui(Action::ClosePanel, tx)
                }),
            ))
            .child(controls::field(cx, "Format", format))
            .child(control_slider(cx, Control::Padding, b.padding))
            .child(control_slider(cx, Control::InsidePadding, b.inside_padding))
            .child(control_slider(cx, Control::InnerRadius, b.inner_radius))
            .child(control_slider(cx, Control::Shadow, b.shadow))
            .child(controls::field(
                cx,
                "Background",
                controls::choices(
                    self,
                    cx,
                    layer,
                    "background",
                    "Background",
                    [
                        (
                            "Solid",
                            Action::SetBackdropFill { gradient: false },
                            !moving && !b.gradient,
                        ),
                        (
                            "Gradient",
                            Action::SetBackdropFill { gradient: true },
                            !moving && b.gradient,
                        ),
                        (
                            "Motion",
                            Action::SelectMotion(if moving { b.motion } else { Motion::Flow }),
                            moving,
                        ),
                    ],
                ),
            ));
        if moving {
            panel = panel.child(
                col()
                    .gap(6.)
                    .child(
                        row()
                            .justify_between()
                            .center_y()
                            .child(
                                rune::text("Effect")
                                    .font(controls::label_font(k))
                                    .color(k.c.text_base),
                            )
                            .child(
                                controls::button(
                                    cx,
                                    "randomize",
                                    "Randomize",
                                    true,
                                    |s: &mut Editor, tx| {
                                        s.dispatch_ui(Action::RandomizeMotion(None), tx)
                                    },
                                )
                                .help("Try another looping variation. Undo restores the previous one."),
                            ),
                    )
                    .child(controls::choices(
                        self,
                        cx,
                        layer,
                        "motion",
                        "Effect",
                        Motion::EFFECTS
                            .map(|m| (m.label(), Action::SelectMotion(m), b.motion == m)),
                    )),
            );
        }
        // Eight presets of two 9-point halves, each inside a 2-point border and padding, are
        // 208 points: one row across the sidebar's 244.
        let mut palettes = row().key("palettes").w_full().gap(4.).justify_between();
        for (preset, (name, from, to)) in PRESETS.into_iter().enumerate() {
            let current = b.colors.is_none() && b.preset == preset;
            let swatch = |hex: u32| {
                el().w(9.)
                    .h(20.)
                    .fill(rgba([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255]))
                    .inert()
            };
            palettes = palettes.child(
                row()
                    .key(format!("palette-{}", controls::slug(name)))
                    .role("radio")
                    .label(format!("Palette {name}"))
                    .help(name)
                    .checked(current)
                    .padding(2.)
                    .radius(k.radius.md)
                    .border(
                        2.,
                        if current {
                            k.c.border_focus
                        } else {
                            k.c.border_muted
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut Editor, tx| {
                        s.dispatch_ui(Action::SetBackdropPreset(preset), tx)
                    })
                    .child(swatch(from))
                    .child(swatch(to)),
            );
        }
        panel = panel.child(controls::field(cx, "Palette", palettes));
        let stops = b.stops();
        let mut colors = row().key("backdrop-colors").gap(6.);
        for (stop, hex) in b.colors().into_iter().enumerate().take(stops) {
            let [r, g, bl] = [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8];
            let editing = self.picker.open && self.picker.stop == Some(stop);
            colors = colors.child(
                el().key(format!("backdrop-color-{}", stop + 1))
                    .role("button")
                    .label(if stop == 0 {
                        "Backdrop color".to_string()
                    } else {
                        format!("Backdrop color {}", stop + 1)
                    })
                    .a11y_value(format!("#{r:02X}{g:02X}{bl:02X}"))
                    .checked(editing)
                    .size(28., 28.)
                    .radius(k.radius.md)
                    .fill(rgba([r, g, bl, 255]))
                    .border(
                        2.,
                        if editing {
                            k.c.border_focus
                        } else {
                            k.c.border_strong
                        },
                    )
                    .cursor(Cursor::Pointer)
                    .on_press_tx(move |s: &mut Editor, tx| {
                        s.dispatch_ui(Action::EditBackdropColor(stop), tx)
                    }),
            );
        }
        // Pick from screen sets the colour being edited, otherwise the first.
        let stop = self.picker.stop.filter(|_| self.picker.open).unwrap_or(0);
        panel = panel
            .child(controls::field(
                cx,
                if stops == 2 { "Colors" } else { "Color" },
                colors,
            ))
            .child(controls::button(
                cx,
                "pick-screen",
                "Pick from screen",
                true,
                move |s: &mut Editor, tx| s.dispatch_ui(Action::PickBackdropScreenColor(stop), tx),
            ));
        if self.picker.open && self.picker.stop.is_some() {
            panel = panel.child(self.color_picker_view(cx));
        }
        if moving {
            panel = panel
                .child(control_slider(cx, Control::Duration, b.seconds))
                .child(controls::button(
                    cx,
                    "playback",
                    if self.playback.playing {
                        "Pause"
                    } else {
                        "Play"
                    },
                    true,
                    |s: &mut Editor, tx| s.dispatch_ui(Action::TogglePlayback, tx),
                ));
        }
        panel.child(controls::switch(
            cx,
            "backdrop-enabled",
            "Enable backdrop",
            enabled,
            |s: &mut Editor, tx| s.dispatch_ui(Action::ToggleBackdropEnabled, tx),
        ))
    }
}
