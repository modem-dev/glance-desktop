use super::*;
use crate::editor::view::{HoverLabel, icon};
use gpui::prelude::*;
impl Editor {
    pub(in crate::editor) fn ask_button(
        &self,
        label: &str,
        primary: bool,
        enabled: bool,
        action: Action,
        cx: &Context<Self>,
    ) -> AnyElement {
        let button = div()
            .id(SharedString::from(format!("ask-{label}")))
            .debug_selector({
                let label = label.to_owned();
                move || format!("ask-{label}")
            })
            .h(px(36.))
            .px_3()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(if primary && enabled {
                0x263044
            } else {
                0xbcc5d3
            }))
            .bg(rgb(if !enabled {
                0xf3f5f8
            } else if primary {
                0x263044
            } else {
                0xeef1f6
            }))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(if !enabled {
                0x89909c
            } else if primary {
                0xffffff
            } else {
                0x293142
            }))
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(move |s| s.bg(rgb(if primary { 0x39465e } else { 0xe0e6ef })))
            })
            .when(label == "Ask Glance", |el| {
                el.child(icon("sparkles", if primary { 0xffffff } else { 0x293142 }))
            })
            .child(if label == "Ask Glance" {
                "AI".to_owned()
            } else {
                label.to_owned()
            })
            .tooltip({
                let label = label.to_owned();
                move |_, cx| {
                    cx.new(|_| {
                        HoverLabel(if label == "Ask Glance" {
                            "Ask Glance · ⌘K / Ctrl+K".into()
                        } else {
                            label.clone().into()
                        })
                    })
                    .into()
                }
            })
            .on_click(cx.listener({
                let action = action.clone();
                move |this, _, window, cx| {
                    if enabled {
                        let toggle = matches!(action, Action::ToggleAskGlance);
                        this.dispatch_ui(action.clone(), cx);
                        if toggle {
                            if this.ask.open {
                                this.ask.input.read(cx).focus(window);
                            } else {
                                this.focus.focus(window);
                            }
                        }
                    }
                }
            }));
        self.accessible_button(label, enabled, action, button)
    }
    pub(in crate::editor) fn ask_bar(&self, cx: &Context<Self>) -> AnyElement {
        let running = self.ask.running.is_some();
        let connected = self.chatgpt.snapshot.can_infer();
        let input = self.ask.input.clone();
        let sender = self.sender.clone();
        let scope = self.tool_scope();
        let input = self.accessibility.element(
            input,
            crate::accessibility::Node::text(
                "Ask Glance prompt",
                self.ask.prompt.clone(),
                std::rc::Rc::new(move |request| {
                    if let crate::accessibility::Request::SetText(prompt) = request {
                        let _ = sender.try_send(Message::Accessibility(
                            scope,
                            Action::SetAskGlancePrompt { prompt },
                        ));
                    }
                }),
            ),
        );
        let model = self
            .chatgpt
            .snapshot
            .models
            .iter()
            .find(|m| Some(&m.slug) == self.chatgpt.snapshot.model.as_ref())
            .map(|m| m.display_name.as_str())
            .unwrap_or("Choose model");
        let model_button = self.account_dropdown(
            super::super::chatgpt::Picker::Model,
            model.to_owned(),
            !running && !self.chatgpt.busy,
            true,
            cx,
        );
        let close = self.accessible_button(
            "Close Ask Glance",
            true,
            Action::ToggleAskGlance,
            div()
                .id("ask-close")
                .debug_selector(|| "ask-close".into())
                .size(px(36.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0xe0e6ef)))
                .child(icon("x", 0x293142))
                .tooltip(|_, cx| cx.new(|_| HoverLabel("Close".into())).into())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.dispatch_ui(Action::ToggleAskGlance, cx);
                    this.focus.focus(window);
                })),
        );
        div()
            .id("ask-bar")
            .debug_selector(|| "ask-bar".into())
            .flex_shrink_0()
            .w_full()
            .px_4()
            .py_2()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(0xf5f7fa))
            .border_t_1()
            .border_color(rgb(0xbcc5d3))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w(px(0.)).child(input))
                    .when(connected, |el| el.child(model_button))
                    .child(if running {
                        self.ask_button("Stop", false, true, Action::CancelAskGlance, cx)
                    } else if connected {
                        self.ask_button(
                            "Run",
                            true,
                            !self.ask.prompt.trim().is_empty()
                                && !self.is_busy()
                                && !self.chatgpt.busy,
                            Action::AskGlance {
                                prompt: self.ask.prompt.clone(),
                            },
                            cx,
                        )
                    } else {
                        self.ask_button(
                            "Sign in",
                            true,
                            !self.chatgpt.busy,
                            Action::ToggleChatgptAccountMenu,
                            cx,
                        )
                    })
                    .child(close),
            )
            .when(!self.ask.status.is_empty(), |el| {
                el.child(
                    div()
                        .id("ask-status")
                        .debug_selector(|| "ask-status".into())
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(if self.ask.error { 0xb63025 } else { 0x536078 }))
                        .when_some(self.ask.running.as_ref(), |el, run| {
                            el.child(
                                div()
                                    .id("ask-spinner")
                                    .debug_selector(|| "ask-spinner".into())
                                    .size(px(14.))
                                    .child(
                                        svg()
                                            .path("icons/rotate-cw.svg")
                                            .size(px(14.))
                                            .text_color(rgb(0x536078))
                                            .with_animation(
                                                "ask-spin",
                                                Animation::new(std::time::Duration::from_secs(1))
                                                    .repeat(),
                                                |svg, delta| {
                                                    svg.with_transformation(Transformation::rotate(
                                                        percentage(delta),
                                                    ))
                                                },
                                            ),
                                    ),
                            )
                            .child(self.ask.status.clone())
                            .child(format!("{}s", run.started.elapsed().as_secs()))
                        })
                        .when(!running, |el| el.child(self.ask.status.clone())),
                )
            })
            .when(!self.ask.answer.is_empty(), |el| {
                el.child(
                    div()
                        .id("ask-answer")
                        .debug_selector(|| "ask-answer".into())
                        .max_h(px(100.))
                        .overflow_y_scroll()
                        .text_sm()
                        .child(self.ask.answer.clone()),
                )
            })
            .into_any_element()
    }
}
