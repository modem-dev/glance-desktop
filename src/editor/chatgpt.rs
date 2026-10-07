//! Account jobs run independently of document rendering and never expose tokens.
use super::{Editor, Message, actions::Action};
use crate::chatgpt::{OcrEngine, Service, Snapshot};
use gpui::*;
use std::{cell::Cell, rc::Rc};
mod view;
const REVOCATION_WARNING: &str = "Signed out locally; remote revocation was not confirmed. Disconnect Glance in ChatGPT settings.";

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Picker {
    Account,
    Model,
}
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
    pub trigger_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub picker: Option<Picker>,
    pub picker_index: usize,
    pub error: bool,
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
    pub error: bool,
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
            trigger_bounds: Rc::new(Cell::new(Bounds::default())),
            picker: None,
            picker_index: 0,
            error: false,
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
                    Job::SignIn(id) => service
                        .sign_in(id.as_deref(), &cancel, |url| {
                            let _ = sender
                                .send_blocking(Message::ChatgptAuthorization { generation, url });
                        })
                        .map(|_| "ChatGPT account connected".into()),
                    Job::SignOut => service.sign_out().map(|confirmed| {
                        if confirmed {
                            "Signed out of ChatGPT".into()
                        } else {
                            REVOCATION_WARNING.into()
                        }
                    }),
                    Job::Account(id) => service
                        .select_account(&id)
                        .map(|_| "ChatGPT account selected".into()),
                    Job::Model(model) => service
                        .set_model(&model)
                        .map(|_| "ChatGPT model selected".into()),
                    Job::Engine(engine) => service.set_engine(engine).map(|_| {
                        match engine {
                            OcrEngine::Local => "OCR uses this device",
                            OcrEngine::Chatgpt => "OCR uses ChatGPT",
                        }
                        .into()
                    }),
                    Job::Welcome => service.dismiss_welcome().map(|_| String::new()),
                };
                let error = result
                    .as_ref()
                    .is_err_and(|e| e != "ChatGPT sign-in canceled")
                    || result
                        .as_ref()
                        .is_ok_and(|status| status == REVOCATION_WARNING);
                Ok::<_, String>(Outcome {
                    snapshot: service.snapshot().ok(),
                    status: result.unwrap_or_else(|e| e),
                    error,
                })
            }));
            let outcome = match outcome {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => Outcome {
                    snapshot: None,
                    status: error,
                    error: true,
                },
                Err(_) => Outcome {
                    snapshot: None,
                    status: "ChatGPT account operation stopped unexpectedly".into(),
                    error: true,
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
        self.chatgpt.picker = None;
        self.chatgpt.error = false;
        self.chatgpt.status = if matches!(job, Job::SignIn(_)) {
            "Finish signing in in your browser…".into()
        } else {
            "Updating ChatGPT account…".into()
        };
        self.chatgpt.spawn(job, self.sender.clone());
        cx.notify();
        Ok(())
    }
    pub(super) fn toggle_chatgpt_picker(&mut self, picker: Picker, cx: &mut Context<Self>) {
        if self.chatgpt.picker == Some(picker) {
            self.chatgpt.picker = None;
        } else {
            if picker == Picker::Account || !self.ask.open {
                self.chatgpt.menu = true;
            }
            self.chatgpt.picker = Some(picker);
            self.chatgpt.picker_index = match picker {
                Picker::Account => self
                    .chatgpt
                    .snapshot
                    .accounts
                    .iter()
                    .position(|a| Some(&a.id) == self.chatgpt.snapshot.active_account.as_ref()),
                Picker::Model => self
                    .chatgpt
                    .snapshot
                    .models
                    .iter()
                    .position(|m| Some(&m.slug) == self.chatgpt.snapshot.model.as_ref()),
            }
            .unwrap_or(0);
        }
        cx.notify();
    }
    pub(super) fn chatgpt_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if !self.chatgpt.menu && self.chatgpt.picker.is_none() {
            return false;
        }
        if let Some(picker) = self.chatgpt.picker {
            let options = self.chatgpt_picker_options(picker);
            match key {
                "escape" => self.dispatch_ui(Action::ToggleChatgptPicker { picker }, cx),
                "up" | "down" if !options.is_empty() => {
                    let count = options.len();
                    self.chatgpt.picker_index = (self.chatgpt.picker_index
                        + if key == "up" { count - 1 } else { 1 })
                        % count;
                    cx.notify();
                }
                "enter" => {
                    if let Some((_, action)) = options.get(self.chatgpt.picker_index) {
                        self.dispatch_ui(action.clone(), cx);
                    }
                }
                _ => return false,
            }
            return true;
        }
        if key == "escape" {
            self.dispatch_ui(Action::ToggleChatgptAccountMenu, cx);
            return true;
        }
        false
    }
}
