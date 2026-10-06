use super::Editor;
use super::actions::Action;
use super::feedback::CopyFeedback;
use crate::{document::Tool, menus};
use gpui::{prelude::*, *};
pub(super) struct HoverLabel(pub(super) SharedString);
impl Render for HoverLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(theme.tooltip))
            .text_color(rgb(theme.tooltip_text))
            .text_xs()
            .shadow_md()
            .child(crate::platform::shortcut_label(&self.0))
    }
}
pub(super) fn icon(name: &'static str, color: u32) -> impl IntoElement {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(18.))
        .flex_shrink_0()
        .text_color(rgb(color))
}
impl Editor {
    pub(super) fn tool_button(&self, tool: Tool, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let (name, key) = match tool {
            Tool::Select => ("mouse-pointer-2", "V"),
            Tool::Pen => ("pen-line", "P"),
            Tool::Arrow => ("arrow-up-right", "A"),
            Tool::Rectangle => ("square", "R"),
            Tool::Text => ("type", "T"),
            Tool::Highlight => ("highlighter", "H"),
            Tool::Pixelate => ("grid-2x2", "B"),
            Tool::Crop => ("crop", "X"),
            Tool::Counter => ("list-ordered", "N"),
            Tool::Spotlight => ("scan", "S"),
            Tool::Magnifier => ("search", "M"),
        };
        let active = self.interaction.tool == tool;
        let label: SharedString = match tool {
            Tool::Spotlight => "Spotlight · S · drag a focus area".into(),
            Tool::Magnifier => "Magnifier · M · drag from detail to lens".into(),
            _ => format!("{} · {}", tool.label(), key).into(),
        };
        let button =
            div()
                .id(SharedString::from(format!("tool-{name}")))
                .debug_selector(move || format!("tool-{name}"))
                .size(px(30.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .cursor_pointer()
                .bg(rgb(if active {
                    theme.accent_background
                } else {
                    theme.chrome
                }))
                .hover(|s| s.bg(rgb(theme.hover)))
                .active(|s| s.bg(rgb(theme.pressed)))
                .child(icon(
                    name,
                    if active {
                        theme.accent
                    } else {
                        theme.secondary
                    },
                ))
                .tooltip(move |_, cx| cx.new(|_| HoverLabel(label.clone())).into())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.dispatch_ui(Action::SelectTool { tool }, cx)
                }));
        self.accessible_button(tool.label(), true, Action::SelectTool { tool }, button)
    }
    pub(super) fn compact_button(
        &self,
        label: &'static str,
        name: &'static str,
        active: bool,
        cx: &Context<Self>,
        action: Action,
    ) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let button = div()
            .id(label)
            .size(px(30.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .bg(rgb(if active {
                theme.accent_background
            } else {
                theme.chrome
            }))
            .hover(|s| s.bg(rgb(theme.hover)))
            .child(icon(
                name,
                if active {
                    theme.accent
                } else {
                    theme.secondary
                },
            ))
            .tooltip(move |_, cx| cx.new(|_| HoverLabel(label.into())).into())
            .on_click(cx.listener({
                let action = action.clone();
                move |this, _, _, cx| this.dispatch_ui(action.clone(), cx)
            }));
        self.accessible_button(crate::platform::shortcut_label(label), true, action, button)
    }
    pub(super) fn button(
        &self,
        label: &str,
        active: bool,
        cx: &Context<Self>,
        action: Action,
    ) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let icon_name = if label.starts_with("Area") {
            Some("scan")
        } else if label.starts_with("Screen") {
            Some("monitor")
        } else if label == "Open" {
            Some("folder-open")
        } else if label == "Backdrop" {
            Some("square")
        } else if label == "Image tools" {
            Some("sparkles")
        } else if label.starts_with("Paste") {
            Some("clipboard-paste")
        } else if label.starts_with("Rotate") {
            Some("rotate-cw")
        } else if label.starts_with("Copy") {
            Some("copy")
        } else if label.starts_with("Export MP4") {
            Some("video")
        } else if label == "Cancel export" {
            Some("square")
        } else if label.starts_with("Save") {
            Some("save")
        } else {
            None
        };
        let button = div()
            .id(SharedString::from(label.to_string()))
            .px_3()
            .py_1()
            .h(px(32.))
            .flex()
            .items_center()
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
            .gap_2()
            .when_some(icon_name, |el, name| {
                el.child(icon(
                    name,
                    if active {
                        theme.accent
                    } else {
                        theme.secondary
                    },
                ))
            })
            .child(crate::platform::shortcut_label(label))
            .on_click(cx.listener({
                let action = action.clone();
                move |this, _, _, cx| this.dispatch_ui(action.clone(), cx)
            }));
        self.accessible_button(crate::platform::shortcut_label(label), true, action, button)
    }
    fn export_progress(&self, progress: u32, cx: &Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let progress = progress.min(100);
        let label = format!("Exporting animation… {progress}%");
        div()
            .id("export-progress")
            .debug_selector(|| "export-progress".into())
            .h(px(56.))
            .flex_shrink_0()
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().child(label))
                    .child(
                        div()
                            .id("export-progress-track")
                            .debug_selector(|| "export-progress-track".into())
                            .w_full()
                            .h(px(6.))
                            .rounded_full()
                            .overflow_hidden()
                            .bg(rgb(theme.pressed))
                            .child(
                                div()
                                    .id("export-progress-fill")
                                    .debug_selector(|| "export-progress-fill".into())
                                    .h_full()
                                    .w(relative(progress as f32 / 100.))
                                    .bg(rgb(theme.positive_fill)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("export-cancel")
                    .debug_selector(|| "export-cancel".into())
                    .child(self.button("Cancel export", false, cx, Action::CancelExport)),
            )
    }
}
impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::Theme::get(cx);
        let tool_target = (
            self.options_tool(),
            self.interaction.selected,
            self.interaction.text_session,
        );
        if self.tool_picker_target != Some(tool_target)
            || self.panels.backdrop
            || self.panels.enhance
            || self.panels.animation
        {
            self.tool_color_picker
                .update(cx, |picker, cx| picker.close_if_open(window, cx));
        }
        self.tool_picker_target = Some(tool_target);
        if !self.panels.backdrop {
            for picker in &self.color_pickers {
                picker.update(cx, |picker, cx| picker.close_if_open(window, cx));
            }
        }
        if !window.is_window_active() {
            self.viewport.space_down = false;
            self.viewport.zoom_down = false;
            self.end_pan();
        }
        for image in self.preview.retired.drain(..) {
            let _ = window.drop_image(image);
        }
        let dimensions = self.document.base.dimensions();
        let output_dimensions = self
            .document
            .backdrop
            .map_or(dimensions, |b| b.dimensions(dimensions));
        let viewport = window.viewport_size();
        let export_height = if self.video_export.progress.is_some() {
            56.
        } else {
            0.
        };
        let fit_zoom = ((f32::from(viewport.width) - 260. - 80.) / output_dimensions.0 as f32)
            .min(
                (f32::from(viewport.height) - 48. - export_height - 70.)
                    / output_dimensions.1 as f32,
            )
            .clamp(0.01, 1.);
        let zoom_label = format!("{:.0}%", self.viewport.zoom.unwrap_or(fit_zoom) * 100.);
        let tools = [
            Tool::Select,
            Tool::Pen,
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Text,
            Tool::Highlight,
            Tool::Pixelate,
            Tool::Crop,
            Tool::Counter,
            Tool::Spotlight,
            Tool::Magnifier,
        ];
        let contents = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(theme.chrome))
            .text_color(rgb(theme.text))
            .font_family(crate::platform::UI_FONT)
            .track_focus(&self.focus)
            .key_context(if self.interaction.text_edit.is_some() {
                "GlanceText"
            } else {
                "GlanceCanvas"
            })
            .on_action(
                cx.listener(|this, _: &menus::Open, _, cx| this.dispatch_ui(Action::OpenImage, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menus::Save, _, cx| this.dispatch_ui(Action::SaveImage, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menus::Copy, _, cx| this.dispatch_ui(Action::Copy, cx)),
            )
            .on_action(cx.listener(|this, _: &menus::CopyRemote, _, cx| {
                this.dispatch_ui(Action::CopyRemote, cx)
            }))
            .on_action(
                cx.listener(|this, _: &menus::Paste, _, cx| this.dispatch_ui(Action::Paste, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menus::Undo, _, cx| this.dispatch_ui(Action::Undo, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menus::Redo, _, cx| this.dispatch_ui(Action::Redo, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menus::Delete, _, cx| this.dispatch_ui(Action::Delete, cx)),
            )
            .on_action(cx.listener(|this, _: &menus::CaptureArea, _, cx| {
                this.dispatch_ui(Action::Capture { area: true }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::CaptureScreen, _, cx| {
                this.dispatch_ui(Action::Capture { area: false }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Fit, _, cx| this.dispatch_ui(Action::Fit, cx)))
            .on_action(cx.listener(|this, _: &menus::ActualSize, _, cx| {
                this.dispatch_ui(Action::ActualSize, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::ZoomIn, _, cx| {
                this.dispatch_ui(Action::Zoom { factor: 1.25 }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::ZoomOut, _, cx| {
                this.dispatch_ui(Action::Zoom { factor: 0.8 }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Select, _, cx| {
                this.dispatch_ui(Action::SelectTool { tool: Tool::Select }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Pen, _, cx| {
                this.dispatch_ui(Action::SelectTool { tool: Tool::Pen }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Arrow, _, cx| {
                this.dispatch_ui(Action::SelectTool { tool: Tool::Arrow }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Rectangle, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Rectangle,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Text, _, cx| {
                this.dispatch_ui(Action::SelectTool { tool: Tool::Text }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Highlight, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Highlight,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Pixelate, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Pixelate,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Crop, _, cx| {
                this.dispatch_ui(Action::SelectTool { tool: Tool::Crop }, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Counter, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Counter,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Spotlight, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Spotlight,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Magnifier, _, cx| {
                this.dispatch_ui(
                    Action::SelectTool {
                        tool: Tool::Magnifier,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::ExportVideo, _, cx| {
                this.dispatch_ui(
                    Action::ExportAnimation {
                        format: super::actions::AnimationFormat::Mp4,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::ExportGif, _, cx| {
                this.dispatch_ui(
                    Action::ExportAnimation {
                        format: super::actions::AnimationFormat::Gif,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &menus::Backdrop, _, cx| {
                this.dispatch_ui(Action::ToggleBackdrop, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::ImageTools, _, cx| {
                this.dispatch_ui(Action::ToggleEnhance, cx)
            }))
            .on_action(
                cx.listener(|this, _: &menus::Help, _, cx| this.dispatch_ui(Action::Help, cx)),
            )
            .on_action(cx.listener(|this, _: &menus::SelectAll, _, cx| {
                this.dispatch_ui(Action::SelectAll, cx)
            }))
            .on_action(cx.listener(|this, _: &menus::Duplicate, _, cx| {
                this.dispatch_ui(Action::DuplicateSelection, cx)
            }))
            .on_key_down(cx.listener(Self::key))
            .on_key_up(cx.listener(|this, e: &KeyUpEvent, _, cx| {
                match e.keystroke.key.as_str() {
                    "space" => {
                        this.viewport.space_down = false;
                        this.end_pan();
                    }
                    "z" => this.viewport.zoom_down = false,
                    _ => {}
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e, _, cx| this.outside_text(e, cx)),
            )
            .on_mouse_move(cx.listener(|this, e, _, cx| this.motion(e, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, e, _, cx| this.finish(e, cx)),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, _, _, _| this.end_pan()),
            )
            .child(
                div()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(rgb(theme.divider))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(theme.accent_fill))
                            .mr_2()
                            .child("glance"),
                    )
                    .child(self.compact_button(
                        "Area · ⌘⌥2",
                        "scan",
                        false,
                        cx,
                        Action::Capture { area: true },
                    ))
                    .child(self.compact_button(
                        "Screen · ⌘⌥3",
                        "monitor",
                        false,
                        cx,
                        Action::Capture { area: false },
                    ))
                    .child(self.compact_button(
                        "Open · ⌘O",
                        "folder-open",
                        false,
                        cx,
                        Action::OpenImage,
                    ))
                    .child(div().w(px(1.)).h(px(18.)).mx_1().bg(rgb(theme.divider)))
                    .children(tools.into_iter().map(|tool| self.tool_button(tool, cx)))
                    .child(div().w(px(1.)).h(px(18.)).mx_1().bg(rgb(theme.divider)))
                    .child(self.compact_button(
                        "Backdrop",
                        "square",
                        self.panels.backdrop,
                        cx,
                        Action::ToggleBackdrop,
                    ))
                    .child(self.compact_button(
                        "Animation",
                        "play",
                        self.panels.animation,
                        cx,
                        Action::ToggleAnimationPanel,
                    ))
                    .child(self.compact_button(
                        "Image tools",
                        "sparkles",
                        self.panels.enhance,
                        cx,
                        Action::ToggleEnhance,
                    ))
                    .child(div().flex_1())
                    .child(self.compact_button(
                        "Copy · ⌘C",
                        if self.feedback.copy == Some(CopyFeedback::Copied) {
                            "check"
                        } else {
                            "copy"
                        },
                        false,
                        cx,
                        Action::CopyImage,
                    ))
                    .child(
                        div()
                            .id("copy-remote")
                            .debug_selector(|| "copy-remote".into())
                            .flex_shrink_0()
                            .child(self.compact_button(
                                "Copy (remote) · Upload to Glance · ⌘⇧C",
                                if matches!(self.feedback.copy, Some(CopyFeedback::LinkCopied(_))) {
                                    "check"
                                } else {
                                    "cloud-upload"
                                },
                                false,
                                cx,
                                Action::CopyRemote,
                            )),
                    )
                    .child(self.export_menu(cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .text_xs()
                            .text_color(rgb(theme.secondary))
                            .child(
                                div()
                                    .id("image-size")
                                    .child(format!(
                                        "{}×{}",
                                        output_dimensions.0, output_dimensions.1
                                    ))
                                    .tooltip(|_, cx| {
                                        cx.new(|_| HoverLabel("Image size in pixels".into())).into()
                                    }),
                            )
                            .child(div().h(px(20.)).w(px(1.)).bg(rgb(theme.divider)))
                            .child(
                                div()
                                    .id("header-zoom")
                                    .debug_selector(|| "header-zoom".into())
                                    .min_w(px(36.))
                                    .cursor_pointer()
                                    .child(zoom_label)
                                    .tooltip(|_, cx| {
                                        cx.new(|_| HoverLabel("Zoom · ⌘+/⌘− · Click to fit".into()))
                                            .into()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.dispatch_ui(Action::Fit, cx);
                                    })),
                            ),
                    ),
            )
            .child(self.canvas(window, cx))
            .when_some(self.video_export.progress, |el, progress| {
                el.child(self.export_progress(progress, cx))
            })
            .when_some(self.feedback.copy, |el, feedback| {
                el.child(self.copy_confirmation(feedback, cx))
            });
        self.accessibility.root(contents)
    }
}
