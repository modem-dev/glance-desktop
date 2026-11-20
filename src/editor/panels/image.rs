//! Image tools: resize, smart upscale, rotation and paste.
use super::{
    super::{Editor, actions::Action},
    controls,
};
use crate::enhance;
use rune::{Cx, El, col, row, text, tok};

const SCALES: [(f32, &str); 6] = [
    (0.5, "50%"),
    (1., "100%"),
    (1.5, "150%"),
    (2., "200%"),
    (3., "300%"),
    (4., "400%"),
];

impl Editor {
    pub(super) fn image_controls(&self, cx: &mut Cx<Editor>, layer: &mut controls::Layer) -> El {
        let t = tok::theme(cx.system_dark());
        let note = |content: String, key: &str| {
            text(content)
                .key(key.to_string())
                .wrap()
                .font(tok::type_::LABEL)
                .color(t.text_secondary)
        };
        let source = self.doc.as_ref().map(|doc| doc.base.dimensions());
        let target = source.map(|source| enhance::dimensions(source, self.resize_scale));
        let can_apply = !self.is_busy() && matches!(target, Some(Ok(_))) && self.resize_scale != 1.;
        let working = self
            .operation
            .is_some_and(|(_, op)| op == super::super::Operation::Transform);
        let mut resize = col()
            .gap(8.)
            .child(controls::field(
                cx,
                "Resize",
                controls::choices(
                    self,
                    cx,
                    layer,
                    "scale",
                    "Resize",
                    SCALES.map(|(scale, label)| {
                        (
                            label,
                            Action::SetResizeScale(scale),
                            self.resize_scale == scale,
                        )
                    }),
                ),
            ))
            .child(
                row()
                    .key("smart-upscale")
                    .role("checkbox")
                    .label("Smart upscale")
                    .description("Local adaptive sharpening; no AI model or uploads")
                    .checked(self.resize_smart)
                    .gap(8.)
                    .center_y()
                    .cursor(rune::Cursor::Pointer)
                    .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::ToggleSmartResize, tx))
                    .child(
                        rune::el()
                            .size(16., 16.)
                            .radius(4.)
                            .center()
                            .fill(if self.resize_smart {
                                super::super::live::rgba(super::super::live::ACCENT)
                            } else {
                                t.control_off
                            })
                            .inert()
                            .children(self.resize_smart.then(|| {
                                rune::icon(&super::super::icons::CHECK, 12.).color(t.knob)
                            })),
                    )
                    .child(
                        text("Smart upscale")
                            .font(tok::type_::LABEL)
                            .color(t.text)
                            .inert(),
                    ),
            )
            .child(note(
                "Local adaptive sharpening; no AI model or uploads".into(),
                "smart-note",
            ))
            .child(note(
                "Sharper edges when enlarging. Annotations redraw at full resolution.".into(),
                "resize-note",
            ));
        if let (Some(source), Some(target)) = (source, target) {
            resize = resize.child(note(
                match target {
                    Ok((width, height)) => {
                        format!("{}×{} → {width}×{height}", source.0, source.1)
                    }
                    Err(_) => "Choose a smaller scale".into(),
                },
                "resize-target",
            ));
        }
        col()
            .key("image-tools")
            .gap(14.)
            .child(resize)
            .child(
                row()
                    .gap(8.)
                    .child(controls::button(
                        cx,
                        "apply-resize",
                        if working {
                            "Working…"
                        } else {
                            "Apply resize"
                        },
                        can_apply,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::ApplyResize, tx),
                    ))
                    .child(controls::button(
                        cx,
                        "image-done",
                        "Done",
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::ToggleImageTools, tx),
                    )),
            )
            .child(
                row()
                    .gap(8.)
                    .child(controls::button(
                        cx,
                        "rotate",
                        "Rotate 90°",
                        source.is_some() && !self.is_busy(),
                        |s: &mut Editor, tx| s.dispatch_ui(Action::Rotate, tx),
                    ))
                    .child(controls::button(
                        cx,
                        "paste-image",
                        "Paste image ⌘V",
                        !self.is_busy(),
                        |s: &mut Editor, tx| s.dispatch_ui(Action::PasteImage, tx),
                    )),
            )
    }
}
