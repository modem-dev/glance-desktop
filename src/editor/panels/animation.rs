//! The Animation panel: the image's entrance, its timing, Hold or Exit, and playback.
use super::{
    super::{Editor, actions::Action},
    controls,
};
use crate::animation::{AnimationControl, Entrance};
use rune::{Cx, El, col, row, text, tok};

fn control_slider(cx: &Cx<Editor>, control: AnimationControl, editor: &Editor) -> El {
    let a = editor
        .doc
        .as_ref()
        .map(|d| d.image_animation)
        .unwrap_or_default();
    let value = control.value(a, editor.playback.position);
    let action: controls::SliderAction = match control {
        AnimationControl::Duration => |value, remember| Action::SetAnimationControl {
            control: AnimationControl::Duration,
            value,
            remember,
        },
        AnimationControl::Delay => |value, remember| Action::SetAnimationControl {
            control: AnimationControl::Delay,
            value,
            remember,
        },
        AnimationControl::Length => |value, remember| Action::SetAnimationControl {
            control: AnimationControl::Length,
            value,
            remember,
        },
        AnimationControl::Time => |value, remember| Action::SetAnimationControl {
            control: AnimationControl::Time,
            value,
            remember,
        },
    };
    controls::slider(
        cx,
        control.label(),
        value,
        control.range(a),
        control.display(value),
        action,
    )
}

impl Editor {
    pub(super) fn animation_controls(
        &self,
        cx: &mut Cx<Editor>,
        layer: &mut controls::Layer,
    ) -> El {
        let t = tok::theme(cx.system_dark());
        let a = self
            .doc
            .as_ref()
            .map(|d| d.image_animation)
            .unwrap_or_default();
        let finished = self.playback.position >= self.clip_seconds();
        let playing = self.playback.playing && !finished;
        let mut panel = col()
            .key("animation-panel")
            .gap(14.)
            .child(
                row()
                    .justify_between()
                    .center_y()
                    .child(text("Animation").font(tok::type_::SECTION).color(t.text))
                    .child(controls::button(
                        cx,
                        "animation-done",
                        "Done",
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::ClosePanel, tx),
                    )),
            )
            .child(controls::field(
                cx,
                "Image entrance",
                controls::choices(
                    self,
                    cx,
                    layer,
                    "entrance",
                    "Image entrance",
                    Entrance::ALL.map(|e| (e.label(), Action::SelectEntrance(e), a.effect == e)),
                ),
            ))
            .child(
                row()
                    .gap(6.)
                    .child(controls::button(
                        cx,
                        "replay",
                        "Replay",
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::ReplayAnimation, tx),
                    ))
                    .child(controls::button(
                        cx,
                        "playback",
                        if playing { "Pause" } else { "Play" },
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::TogglePlayback, tx),
                    )),
            )
            .child(control_slider(cx, AnimationControl::Time, self))
            .child(
                text(format!(
                    "{} / {} s",
                    AnimationControl::Time
                        .display(
                            AnimationControl::Time
                                .value(a, self.playback.position.min(self.clip_seconds()))
                        )
                        .trim_end_matches(" s"),
                    self.clip_seconds()
                ))
                .key("clip-time")
                .font(tok::type_::LABEL)
                .color(t.text_secondary),
            );
        if self.preparing() {
            panel = panel.child(
                text("Preparing preview…")
                    .key("preparing")
                    .font(tok::type_::LABEL)
                    .color(t.text_secondary)
                    .live_region(rune::LiveRegion::Polite),
            );
        }
        panel
            .child(control_slider(cx, AnimationControl::Duration, self))
            .child(control_slider(cx, AnimationControl::Delay, self))
            .child(control_slider(cx, AnimationControl::Length, self))
            .child(controls::field(
                cx,
                "After entrance",
                controls::choices(
                    self,
                    cx,
                    layer,
                    "after",
                    "After entrance",
                    [
                        ("Hold", Action::SetAnimationExit(false), !a.exit),
                        ("Exit", Action::SetAnimationExit(true), a.exit),
                    ],
                ),
            ))
            .child(
                text("Configure the moving background in Backdrop.")
                    .wrap()
                    .font(tok::type_::LABEL)
                    .color(t.text_secondary),
            )
    }
}
