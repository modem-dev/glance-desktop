//! Account jobs run independently of document rendering and never expose tokens.
use super::{Editor, Message, actions::Action};
use crate::chatgpt::{OcrEngine, Service, Snapshot};
use gpui::{prelude::*, *};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
pub(super) struct State {
    pub runtime: Arc<Mutex<Service>>,
    pub snapshot: Snapshot,
    pub busy: bool,
    pub signing_in: bool,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub inference_cancel: Arc<AtomicBool>,
    pub menu: bool,
    pub status: String,
}
pub(super) enum Job {
    Load,
    SignIn(Option<String>),
    SignOut,
    Account(String),
    Model(String),
    Engine(OcrEngine),
    Welcome,
}
pub(crate) struct Outcome {
    pub snapshot: Option<Snapshot>,
    pub status: String,
}
impl State {
    pub fn new(native: bool, sender: async_channel::Sender<Message>) -> Self {
        let mut state = Self {
            runtime: Arc::new(Mutex::new(Service::new(if native {
                Service::default_directory()
            } else {
                None
            }))),
            snapshot: Snapshot::default(),
            busy: false,
            signing_in: false,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            inference_cancel: Arc::new(AtomicBool::new(false)),
            menu: false,
            status: String::new(),
        };
        if native {
            state.spawn(Job::Load, sender);
        }
        state
    }
    fn spawn(&mut self, job: Job, sender: async_channel::Sender<Message>) {
        self.generation += 1;
        self.busy = true;
        self.signing_in = matches!(job, Job::SignIn(_));
        self.cancel = Arc::new(AtomicBool::new(false));
        let generation = self.generation;
        let cancel = self.cancel.clone();
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut service = runtime
                    .lock()
                    .map_err(|_| "ChatGPT account service stopped unexpectedly".to_string())?;
                let result: Result<String, String> = match job {
                    Job::Load => service.snapshot().map(|_| String::new()),
                    Job::SignIn(id) => service.sign_in(id.as_deref(), &cancel, |url| {
                        let _ = sender.send_blocking(Message::ChatgptAuthorization { generation, url });
                    }).map(|_| "ChatGPT account connected".into()),
                    Job::SignOut => service.sign_out().map(|confirmed| if confirmed { "Signed out of ChatGPT".into() }
                        else { "Signed out locally; remote revocation was not confirmed. Disconnect Glance in ChatGPT settings.".into() }),
                    Job::Account(id) => service.select_account(&id).map(|_| "ChatGPT account selected".into()),
                    Job::Model(model) => service.set_model(&model).map(|_| "ChatGPT model selected".into()),
                    Job::Engine(engine) => service.set_engine(engine).map(|_| match engine {
                        OcrEngine::Local => "OCR uses this device", OcrEngine::Chatgpt => "OCR uses your ChatGPT plan and sends the image to OpenAI",
                    }.into()),
                    Job::Welcome => service.dismiss_welcome().map(|_| String::new()),
                };
                Ok::<_, String>(Outcome {
                    snapshot: service.snapshot().ok(),
                    status: result.unwrap_or_else(|e| e),
                })
            }));
            let outcome = match outcome {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => Outcome {
                    snapshot: None,
                    status: error,
                },
                Err(_) => Outcome {
                    snapshot: None,
                    status: "ChatGPT account operation stopped unexpectedly".into(),
                },
            };
            let _ = sender.send_blocking(Message::ChatgptFinished {
                generation,
                outcome,
            });
        });
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.inference_cancel.store(true, Ordering::Relaxed);
    }
}
impl Editor {
    pub(super) fn chatgpt_job(&mut self, job: Job, cx: &mut Context<Self>) -> Result<(), String> {
        if self.chatgpt.busy {
            return Err("Wait for the current ChatGPT account operation or cancel sign-in".into());
        }
        self.chatgpt.status = if matches!(job, Job::SignIn(_)) {
            "Finish signing in in your browser…".into()
        } else {
            "Updating ChatGPT account…".into()
        };
        self.chatgpt.spawn(job, self.sender.clone());
        cx.notify();
        Ok(())
    }
    pub(super) fn chatgpt_menu(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        let snapshot = &self.chatgpt.snapshot;
        let mut panel = div()
            .id("chatgpt-menu")
            .debug_selector(|| "chatgpt-menu".into())
            .absolute()
            .top(px(54.))
            .right(px(12.))
            .w(px(330.))
            .max_h((window.viewport_size().height - px(70.)).max(px(150.)))
            .overflow_y_scroll()
            .occlude()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .shadow_lg()
            .bg(rgb(0xfcfcfd))
            .text_color(rgb(0x44454f))
            .text_sm()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("ChatGPT"))
                    .child(self.button("Done", false, cx, Action::ToggleChatgptAccountMenu)),
            )
            .child(div().text_xs().child(
                "Use your ChatGPT plan for OCR. ChatGPT OCR sends the source image to OpenAI.",
            ));
        if !self.chatgpt.status.is_empty() {
            panel = panel.child(div().text_xs().child(self.chatgpt.status.clone()));
        }
        if self.chatgpt.signing_in {
            panel =
                panel.child(self.button("Cancel sign-in", false, cx, Action::CancelChatgptSignIn));
        } else if !self.chatgpt.busy {
            for account in &snapshot.accounts {
                panel = panel.child(self.button(
                    &format!(
                        "{}{}",
                        account.label,
                        if account.signed_in {
                            ""
                        } else {
                            " · Signed out"
                        }
                    ),
                    Some(&account.id) == snapshot.active_account.as_ref(),
                    cx,
                    Action::SelectChatgptAccount {
                        account_id: account.id.clone(),
                    },
                ));
            }
            panel = panel.child(self.button(
                "Continue with ChatGPT",
                false,
                cx,
                Action::ChatgptSignIn { account_id: None },
            ));
            if let Some(id) = &snapshot.active_account {
                panel = panel.child(self.button(
                    "Reconnect selected account",
                    false,
                    cx,
                    Action::ChatgptSignIn {
                        account_id: Some(id.clone()),
                    },
                ));
                panel = panel.child(self.button(
                    "Sign out of selected account",
                    false,
                    cx,
                    Action::SignOutChatgpt,
                ));
            }
            panel = panel
                .child(div().h(px(1.)).bg(rgb(0xe5e5ec)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Copy as OCR uses"),
                )
                .child(self.button(
                    "This device · Offline",
                    snapshot.ocr_engine == OcrEngine::Local,
                    cx,
                    Action::SetOcrEngine {
                        engine: OcrEngine::Local,
                    },
                ));
            if snapshot.can_infer() {
                panel = panel.child(self.button(
                    "ChatGPT · Using ChatGPT plan",
                    snapshot.ocr_engine == OcrEngine::Chatgpt,
                    cx,
                    Action::SetOcrEngine {
                        engine: OcrEngine::Chatgpt,
                    },
                ));
                for model in &snapshot.models {
                    panel = panel.child(self.button(
                        &model.display_name,
                        Some(&model.slug) == snapshot.model.as_ref(),
                        cx,
                        Action::SetChatgptModel {
                            model: model.slug.clone(),
                        },
                    ));
                }
            } else if snapshot
                .accounts
                .iter()
                .any(|a| a.signed_in && Some(&a.id) == snapshot.active_account.as_ref())
            {
                panel = panel.child(div().text_xs().child("ChatGPT plan use is unavailable. Reconnect to grant permission, or refresh the selected account to load models."));
            }
        }
        panel.child(self.button("Manage usage", false, cx, Action::ManageChatgptUsage))
    }
    pub(super) fn chatgpt_welcome(&self, cx: &Context<Self>) -> impl IntoElement {
        div().id("chatgpt-welcome").absolute().inset_0().occlude().bg(rgba(0x00000066))
            .flex().items_center().justify_center()
            .on_mouse_down(MouseButton::Left, |_,_,cx| cx.stop_propagation())
            .child(div().w(px(390.)).p_5().rounded_lg().bg(rgb(0xfcfcfd)).flex().flex_col().gap_3()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("You’re using your ChatGPT plan"))
                .child("When you choose ChatGPT for OCR, Glance sends your screenshot to OpenAI and uses your ChatGPT plan or available credits. Manage usage and access in ChatGPT settings.")
                .child(self.button("Manage usage",false,cx,Action::ManageChatgptUsage))
                .child(self.button("Got it",false,cx,Action::DismissChatgptWelcome)))
    }
}
