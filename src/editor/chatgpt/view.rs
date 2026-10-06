use super::{Editor, OcrEngine, Picker};
use crate::editor::{
    actions::Action,
    view::{HoverLabel, icon},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

impl Editor {
    fn account_button(
        &self,
        label: &str,
        selected: bool,
        primary: bool,
        enabled: bool,
        action: Action,
        cx: &Context<Self>,
    ) -> AnyElement {
        let dark = selected || primary;
        let foreground = if !enabled {
            0x89909c
        } else if dark {
            0xffffff
        } else {
            0x293142
        };
        let button = div()
            .id(SharedString::from(format!("chatgpt-{label}")))
            .debug_selector({
                let label = label.to_owned();
                move || format!("chatgpt-{label}")
            })
            .w_full()
            .h(px(34.))
            .px_3()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(if dark && enabled { 0x263044 } else { 0xbcc5d3 }))
            .bg(rgb(if !enabled {
                0xf3f5f8
            } else if dark {
                0x263044
            } else {
                0xeef1f6
            }))
            .text_color(rgb(foreground))
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(move |s| s.bg(rgb(if dark { 0x39465e } else { 0xe0e6ef })))
            })
            .child(label.to_owned())
            .on_click(cx.listener({
                let action = action.clone();
                move |this, _, _, cx| {
                    if enabled {
                        this.dispatch_ui(action.clone(), cx);
                    }
                }
            }));
        self.accessible_button(label, enabled, action, button)
    }
    fn account_icon(
        &self,
        label: &'static str,
        name: &'static str,
        enabled: bool,
        action: Action,
        cx: &Context<Self>,
    ) -> AnyElement {
        let button = div()
            .id(label)
            .debug_selector(move || label.to_owned())
            .size(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(rgb(0xbcc5d3))
            .bg(rgb(0xeef1f6))
            .child(icon(name, if enabled { 0x293142 } else { 0x89909c }))
            .when(enabled, |el| {
                el.cursor_pointer().hover(|s| s.bg(rgb(0xe0e6ef)))
            })
            .tooltip(move |_, cx| cx.new(|_| HoverLabel(label.into())).into())
            .on_click(cx.listener({
                let action = action.clone();
                move |this, _, _, cx| {
                    if enabled {
                        this.dispatch_ui(action.clone(), cx);
                    }
                }
            }));
        self.accessible_button(label, enabled, action, button)
    }
    pub(in crate::editor) fn chatgpt_picker_options(
        &self,
        picker: Picker,
    ) -> Vec<(String, Action)> {
        match picker {
            Picker::Account => self
                .chatgpt
                .snapshot
                .accounts
                .iter()
                .map(|a| {
                    (
                        format!(
                            "{}{}",
                            a.label,
                            if a.signed_in { "" } else { " · Signed out" }
                        ),
                        Action::SelectChatgptAccount {
                            account_id: a.id.clone(),
                        },
                    )
                })
                .chain(std::iter::once((
                    "Continue with ChatGPT".into(),
                    Action::ChatgptSignIn { account_id: None },
                )))
                .collect(),
            Picker::Model => self
                .chatgpt
                .snapshot
                .models
                .iter()
                .map(|m| {
                    (
                        m.display_name.clone(),
                        Action::SetChatgptModel {
                            model: m.slug.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
    fn account_dropdown(
        &self,
        picker: Picker,
        label: String,
        enabled: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = match picker {
            Picker::Account => "chatgpt-account",
            Picker::Model => "chatgpt-model",
        };
        let bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
        let painted = bounds.clone();
        let options = self.chatgpt_picker_options(picker);
        let action = Action::ToggleChatgptPicker { picker };
        let accessible_label = format!(
            "{}: {label}",
            if picker == Picker::Account {
                "Account"
            } else {
                "Model"
            }
        );
        let trigger = self.accessible_button(
            accessible_label.clone(),
            enabled,
            action.clone(),
            div()
                .id(id)
                .debug_selector(move || id.to_owned())
                .w_full()
                .h(px(34.))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xbcc5d3))
                .bg(rgb(0xeef1f6))
                .when(enabled, |el| {
                    el.cursor_pointer().hover(|s| s.bg(rgb(0xe0e6ef)))
                })
                .text_color(rgb(if enabled { 0x293142 } else { 0x89909c }))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .child(icon("chevron-down", 0x515d70))
                .tooltip(move |_, cx| {
                    cx.new(|_| HoverLabel(accessible_label.clone().into()))
                        .into()
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled {
                        this.dispatch_ui(action.clone(), cx);
                    }
                })),
        );
        div()
            .relative()
            .flex_1()
            .min_w_0()
            .child(trigger)
            .child(
                canvas(move |rect, _, _| painted.set(rect), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .when(self.chatgpt.picker == Some(picker), |el| {
                el.child(
                    deferred(
                        anchored()
                            .position_mode(AnchoredPositionMode::Local)
                            .position(point(px(0.), px(38.)))
                            .snap_to_window()
                            .child(
                                div()
                                    .id("chatgpt-picker")
                                    .debug_selector(|| "chatgpt-picker".into())
                                    .w(px(292.))
                                    .max_h(px(220.))
                                    .overflow_y_scroll()
                                    .occlude()
                                    .p_1()
                                    .flex()
                                    .flex_col()
                                    .rounded_lg()
                                    .shadow_lg()
                                    .border_1()
                                    .border_color(rgb(0xbcc5d3))
                                    .bg(rgb(0xffffff))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_mouse_down_out(cx.listener(
                                        move |this, e: &MouseDownEvent, _, cx| {
                                            if this.chatgpt.picker == Some(picker)
                                                && !bounds.get().contains(&e.position)
                                            {
                                                this.dispatch_ui(
                                                    Action::ToggleChatgptPicker { picker },
                                                    cx,
                                                );
                                            }
                                        },
                                    ))
                                    .children(options.into_iter().enumerate().map(
                                        |(index, (label, action))| {
                                            let selected = match &action {
                                                Action::SelectChatgptAccount { account_id } => {
                                                    Some(account_id)
                                                        == self
                                                            .chatgpt
                                                            .snapshot
                                                            .active_account
                                                            .as_ref()
                                                }
                                                Action::SetChatgptModel { model } => {
                                                    Some(model)
                                                        == self.chatgpt.snapshot.model.as_ref()
                                                }
                                                _ => false,
                                            };
                                            self.accessible_button(
                                                label.clone(),
                                                enabled,
                                                action.clone(),
                                                div()
                                                    .id(("chatgpt-option", index))
                                                    .debug_selector(move || {
                                                        format!("chatgpt-option-{index}")
                                                    })
                                                    .h(px(32.))
                                                    .px_2()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .rounded_md()
                                                    .text_color(rgb(0x293142))
                                                    .bg(rgb(
                                                        if self.chatgpt.picker_index == index {
                                                            0xe0e6ef
                                                        } else {
                                                            0xffffff
                                                        },
                                                    ))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(rgb(0xe0e6ef)))
                                                    .child(div().w(px(18.)).when(selected, |el| {
                                                        el.child(icon("check", 0x293142))
                                                    }))
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .flex_1()
                                                            .truncate()
                                                            .child(label),
                                                    )
                                                    .on_mouse_move(cx.listener(
                                                        move |this, _, _, cx| {
                                                            if this.chatgpt.picker_index != index {
                                                                this.chatgpt.picker_index = index;
                                                                cx.notify();
                                                            }
                                                        },
                                                    ))
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            if enabled {
                                                                this.dispatch_ui(
                                                                    action.clone(),
                                                                    cx,
                                                                );
                                                            }
                                                        },
                                                    )),
                                            )
                                        },
                                    )),
                            ),
                    )
                    .with_priority(3),
                )
            })
            .into_any_element()
    }
    pub(in crate::editor) fn chatgpt_menu(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let snapshot = &self.chatgpt.snapshot;
        let account = snapshot
            .accounts
            .iter()
            .find(|a| Some(&a.id) == snapshot.active_account.as_ref());
        let enabled = !self.chatgpt.busy && !self.is_busy();
        let cloud = snapshot.ocr_engine == OcrEngine::Chatgpt;
        let show_model = cloud || self.ask.open;
        let mut panel = div()
            .id("chatgpt-menu")
            .debug_selector(|| "chatgpt-menu".into())
            .absolute()
            .top(px(54.))
            .right(px(12.))
            .w(px(320.))
            .max_h((window.viewport_size().height - px(70.)).max(px(150.)))
            .overflow_y_scroll()
            .occlude()
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .shadow_lg()
            .border_1()
            .border_color(rgb(0xd3d9e3))
            .bg(rgb(0xfcfcfd))
            .text_color(rgb(0x293142))
            .text_sm()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, e: &MouseDownEvent, _, cx| {
                if this.chatgpt.picker.is_none()
                    && !this.chatgpt.trigger_bounds.get().contains(&e.position)
                {
                    this.dispatch_ui(Action::ToggleChatgptAccountMenu, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("ChatGPT"))
                    .child(self.account_icon(
                        "Close ChatGPT account menu",
                        "x",
                        true,
                        Action::ToggleChatgptAccountMenu,
                        cx,
                    )),
            );
        if let Some(account) = account {
            // The full registration label stays in the tooltip and account list.
            let label = if snapshot.accounts.len() == 1 {
                account
                    .label
                    .split(" · ")
                    .next()
                    .unwrap_or(&account.label)
                    .to_owned()
            } else {
                account.label.clone()
            };
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.account_dropdown(Picker::Account, label, enabled, cx))
                    .child(self.account_icon(
                        "Reconnect account",
                        "rotate-cw",
                        enabled,
                        Action::ChatgptSignIn {
                            account_id: Some(account.id.clone()),
                        },
                        cx,
                    ))
                    .child(self.account_icon(
                        "Sign out",
                        "log-out",
                        !self.chatgpt.busy,
                        Action::SignOutChatgpt,
                        cx,
                    )),
            );
            if !account.signed_in && !self.chatgpt.signing_in {
                panel = panel.child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b4350))
                        .child("Signed out · Reconnect to use your plan"),
                );
            }
        } else {
            panel = panel.child(self.account_button(
                "Continue with ChatGPT",
                false,
                true,
                enabled,
                Action::ChatgptSignIn { account_id: None },
                cx,
            ));
        }
        if self.chatgpt.signing_in {
            panel = panel
                .child(div().text_xs().child("Finish sign-in in your browser…"))
                .child(self.account_button(
                    "Cancel sign-in",
                    false,
                    false,
                    true,
                    Action::CancelChatgptSignIn,
                    cx,
                ));
        } else if self.chatgpt.busy {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x515d70))
                    .child("Updating account…"),
            );
        } else if self.chatgpt.error {
            panel = panel.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xffefeb))
                    .text_xs()
                    .text_color(rgb(0x9a3024))
                    .child(self.chatgpt.status.clone()),
            );
        }
        panel = panel
            .child(div().h(px(1.)).bg(rgb(0xd3d9e3)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::MEDIUM).child("OCR source")),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(self.account_button(
                        "On device",
                        !cloud,
                        false,
                        enabled,
                        Action::SetOcrEngine {
                            engine: OcrEngine::Local,
                        },
                        cx,
                    )))
                    .child(div().flex_1().child(self.account_button(
                        "ChatGPT",
                        cloud,
                        false,
                        enabled && snapshot.can_infer(),
                        Action::SetOcrEngine {
                            engine: OcrEngine::Chatgpt,
                        },
                        cx,
                    ))),
            );
        if show_model && snapshot.can_infer() {
            let model = snapshot
                .models
                .iter()
                .find(|m| Some(&m.slug) == snapshot.model.as_ref())
                .map(|m| m.display_name.clone())
                .unwrap_or_else(|| "Choose model".into());
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_xs().text_color(rgb(0x515d70)).child("Model"))
                    .child(self.account_dropdown(Picker::Model, model, enabled, cx)),
            );
        } else if account.is_some_and(|a| a.signed_in && !a.plan_enabled) {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(0x515d70))
                    .child("Reconnect to enable ChatGPT plan access."),
            );
        }
        if account.is_some() {
            panel = panel.child(div().flex().items_center().gap_3().justify_end().child(
                div().w(px(128.)).child(self.account_button(
                    "Manage usage",
                    false,
                    false,
                    true,
                    Action::ManageChatgptUsage,
                    cx,
                )),
            ));
        }
        panel
    }
    pub(in crate::editor) fn chatgpt_welcome(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("chatgpt-welcome")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000066))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(390.))
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(0xfcfcfd))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("ChatGPT connected"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().flex_1().child(self.account_button(
                                "Manage usage",
                                false,
                                false,
                                true,
                                Action::ManageChatgptUsage,
                                cx,
                            )))
                            .child(div().flex_1().child(self.account_button(
                                "Got it",
                                false,
                                true,
                                true,
                                Action::DismissChatgptWelcome,
                                cx,
                            ))),
                    ),
            )
    }
}
