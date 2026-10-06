//! Native prompt bar and revision-scoped completion of a private agent draft.
pub(super) mod input;
mod view;
use super::{
    Editor, Message,
    actions::Action,
    jobs::{OperationId, OperationKind},
};
use crate::chatgpt::agent;
use gpui::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct State {
    pub open: bool,
    pub input: Entity<input::PromptInput>,
    pub prompt: String,
    pub answer: String,
    pub status: String,
    pub error: bool,
    pub running: Option<Run>,
    pub steps: usize,
}
pub(super) struct Run {
    pub id: OperationId,
    pub revision: u64,
    pub account: String,
    pub model: String,
    pub cancel: Arc<AtomicBool>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(run) = &self.running {
            run.cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Editor {
    pub(super) fn start_ask(
        &mut self,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        agent::validate_prompt(&prompt)?;
        if self.chatgpt.busy {
            return Err("Wait for the ChatGPT account operation to finish.".into());
        }
        if !self.chatgpt.snapshot.can_infer() {
            return Err("Continue with ChatGPT and choose a model to use Ask Glance.".into());
        }
        if self.interaction.text_edit.is_some() || self.interaction.gesture.is_active() {
            return Err("Finish your annotation text or gesture before using Ask Glance.".into());
        }
        let id = self
            .start_operation(OperationKind::Ask)
            .ok_or("Editor is busy.")?;
        let account = self.chatgpt.snapshot.active_account.clone().unwrap();
        let model = self.chatgpt.snapshot.model.clone().unwrap();
        let revision = self.preview.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        self.chatgpt.inference_cancel = cancel.clone();
        self.ask.running = Some(Run {
            id,
            revision,
            account: account.clone(),
            model: model.clone(),
            cancel: cancel.clone(),
        });
        self.ask.open = true;
        self.ask.prompt = prompt.clone();
        self.ask.input.update(cx, |input, cx| {
            input.set_text(&prompt, cx);
            input.set_disabled(true, cx);
        });
        self.ask.answer.clear();
        self.ask.status = "Looking at your screenshot…".into();
        self.ask.steps = 0;
        self.ask.error = false;
        self.chatgpt.menu = false;
        self.chatgpt.picker = None;
        let runtime = self.chatgpt.runtime.clone();
        let document = self.document.clone();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime
                    .lock()
                    .map_err(|_| "ChatGPT service stopped unexpectedly".to_string())?
                    .ask(
                        document,
                        revision,
                        &prompt,
                        (&account, &model),
                        &cancel,
                        |steps, status| {
                            let _ = sender.send_blocking(Message::AskProgress {
                                id,
                                steps,
                                status: status.to_owned(),
                            });
                        },
                    )
            }))
            .unwrap_or_else(|_| Err("Ask Glance stopped unexpectedly. No changes applied.".into()));
            let _ = sender.send_blocking(Message::AskFinished { id, result });
        });
        cx.notify();
        Ok(())
    }
    pub(super) fn cancel_ask(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.ask.running.take() {
            run.cancel.store(true, Ordering::Relaxed);
            if self
                .operations
                .active
                .as_ref()
                .is_some_and(|op| op.id == run.id)
            {
                self.operations.active = None;
            }
            self.ask.status = "Canceled · No changes applied".into();
            self.ask.error = false;
            self.ask
                .input
                .update(cx, |input, cx| input.set_disabled(false, cx));
            self.feedback.status = self.ask.status.clone();
            cx.notify();
        }
    }
    pub(super) fn finish_ask(
        &mut self,
        id: OperationId,
        result: Result<agent::ResultDocument, String>,
        cx: &mut Context<Self>,
    ) {
        if self.ask.running.as_ref().is_none_or(|run| run.id != id)
            || self.operations.active.as_ref().is_none_or(|op| op.id != id)
        {
            return;
        }
        let run = self.ask.running.take().unwrap();
        self.operations.active = None;
        self.ask
            .input
            .update(cx, |input, cx| input.set_disabled(false, cx));
        let result = if run.cancel.load(Ordering::Relaxed) {
            Err("Canceled · No changes applied".into())
        } else if self.chatgpt.busy
            || self.chatgpt.snapshot.active_account.as_ref() != Some(&run.account)
            || self.chatgpt.snapshot.model.as_ref() != Some(&run.model)
            || !self.chatgpt.snapshot.can_infer()
        {
            Err("ChatGPT account changed. No changes applied.".into())
        } else {
            result
        };
        match result {
            Ok(result) => {
                // The shared dispatcher performs optimistic revision and gesture checks.
                let accepted = if result.changed {
                    self.dispatch(
                        Action::ApplyPreparedDocument {
                            document: Box::new(result.document),
                            revision: run.revision,
                            replace: true,
                        },
                        cx,
                    )
                    .map(|_| ())
                } else if run.revision != self.preview.revision
                    || self.interaction.gesture.is_active()
                    || self.interaction.text_edit.is_some()
                {
                    Err("Editor changed during Ask Glance. Ask again.".into())
                } else {
                    Ok(())
                };
                match accepted {
                    Ok(()) => {
                        self.ask.steps = result.steps;
                        self.ask.answer = result.answer;
                        self.ask.status.clear();
                        self.ask.error = false;
                    }
                    Err(error) => {
                        self.ask.status = error;
                        self.ask.error = true;
                    }
                }
            }
            Err(error) => {
                self.ask.status = error;
                self.ask.error = true;
            }
        }
        self.feedback.status = self.ask.status.clone();
        cx.notify();
        if !self.chatgpt.busy {
            let _ = self.chatgpt_job(super::chatgpt::Job::Load, cx);
        }
    }
}
