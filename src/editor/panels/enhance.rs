use super::super::Editor;
use super::super::actions::{Action, Panel};
use super::super::view::{HoverLabel, icon};
use gpui::{prelude::*, *};
impl Editor {
    pub(in crate::editor) fn enhance_controls(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let target =
            crate::enhance::dimensions(self.document.base.dimensions(), self.panels.resize_scale);
        super::controls::panel("image-panel", cx)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icon("sparkles", theme.accent_fill))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Image tools"),
                            ),
                    )
                    .child(self.button(
                        "Done",
                        false,
                        cx,
                        Action::ClosePanel {
                            panel: Panel::Enhance,
                        },
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Resize"),
            )
            .child(div().flex().flex_wrap().gap_1().children(
                [0.5_f32, 1., 1.5, 2., 3., 4.].into_iter().map(|scale| {
                    let label = format!("{}%", (scale * 100.) as u32);
                    div().w(px(72.)).child(self.choice(
                        format!("resize-scale-{}", (scale * 100.) as u32),
                        label.clone(),
                        div().child(label).into_any_element(),
                        (self.panels.resize_scale == scale, true),
                        Action::SetResizeScale { scale },
                        cx,
                    ))
                }),
            ))
            .child(
                self.accessible_button(
                    "Smart upscale",
                    true,
                    Action::ToggleSmartResize,
                    div()
                        .id("smart-upscale")
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .text_xs()
                        .child(
                            div()
                                .size(px(16.))
                                .rounded_sm()
                                .border_1()
                                .border_color(rgb(theme.border))
                                .bg(rgb(if self.panels.resize_smart {
                                    theme.accent_fill
                                } else {
                                    theme.surface
                                }))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(theme.on_fill))
                                .child(if self.panels.resize_smart { "✓" } else { "" }),
                        )
                        .child("Smart upscale")
                        .tooltip(|_, cx| {
                            cx.new(|_| {
                                HoverLabel(
                                    "Local adaptive sharpening; no AI model or uploads".into(),
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dispatch_ui(Action::ToggleSmartResize, cx);
                        })),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.muted))
                    .child("Sharper edges when enlarging. Annotations redraw at full resolution."),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme.secondary))
                    .child(match target {
                        Ok((w, h)) => format!(
                            "{}×{} → {w}×{h}",
                            self.document.base.width(),
                            self.document.base.height()
                        ),
                        Err(_) => "Choose a smaller scale".into(),
                    }),
            )
            .child(self.button(
                if self.is_busy() {
                    "Working…"
                } else {
                    "Apply resize"
                },
                true,
                cx,
                Action::ApplyResize,
            ))
            .child(div().h(px(1.)).bg(rgb(theme.divider)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(self.button("Rotate 90°", false, cx, Action::Rotate))
                    .child(self.compact_button(
                        "Paste image  ⌘V",
                        "clipboard-paste",
                        false,
                        cx,
                        Action::PasteImage,
                    )),
            )
    }
}
