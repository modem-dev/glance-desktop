use super::Editor;
use super::actions::Action;
use crate::automation::{Request, Snapshot, state};
use gpui::Context;
use serde_json::json;
impl Editor {
    pub fn automation(&mut self, request: Request, cx: &mut Context<Self>) {
        match request {
            Request::State(reply) => {
                let operation = self.operations.active.as_ref().map(|op| {
                    json!({
                        "id":op.id.value(), "kind":op.kind, "progress":self.video_export.progress,
                    })
                });
                let _ = reply.send(Ok(json!({
                    "revision":self.preview.revision, "busy":self.is_busy(), "operation":operation,
                    "tool":self.interaction.tool, "selected":self.interaction.selected,
                    "selected_indices":self.selected_indices(),
                    "selected_ids":self.selected_indices().iter().map(|i| format!("{}:{i}", self.preview.revision)).collect::<Vec<_>>(),
                    "text_editing":self.interaction.text_edit.is_some(), "gesture_active":self.interaction.gesture.is_active(),
                    "tool_options": {"tool":self.options_tool(), "color":self.tool_settings().color, "width":self.tool_settings().width, "style":self.tool_settings().style, "magnification":self.tool_settings().magnification, "crop_ratio":self.interaction.crop_ratio, "counter_number":self.counter_number()},
                    "zoom":self.viewport.zoom, "pan":self.viewport.pan,
                    "panels":{"tools":!self.panels.backdrop && !self.panels.enhance && !self.panels.animation,"backdrop":self.panels.backdrop,"enhance":self.panels.enhance,"animation":self.panels.animation},
                    "image_animation":self.document.image_animation,
                    "sampling_backdrop_color":self.panels.sampling_color,
                    "sampling_tool_color":self.panels.sampling_tool_color,
                    "playback":{"preparing":self.preview_preparing(),"paused":self.playback.paused,"time":self.clip_time(),"seconds":self.document.animation_seconds()},
                    "status":self.feedback.status,
                    "extraction": self.extraction.as_ref().filter(|result| result.revision == self.preview.revision).map(|result| json!({
                        "revision":result.revision, "rectangle":result.rectangle,
                        "text":result.text, "engine":"local",
                    })),
                })));
            }
            Request::Dispatch {
                action,
                expected_revision,
                reply,
            } => {
                let result = if expected_revision.is_some_and(|r| r != self.preview.revision) {
                    Err("Stale expected_revision; read state and retry".into())
                } else {
                    self.dispatch(action, cx)
                };
                let _ = reply.send(result);
            }
            Request::Show(reply) => {
                let result = self
                    .dispatch(Action::Show, cx)
                    .map(|_| json!({"native_window":true}));
                let _ = reply.send(result);
            }
            Request::Snapshot(reply) => {
                if self.is_busy()
                    || self.interaction.gesture.is_active()
                    || self.interaction.text_edit.is_some()
                {
                    let _ = reply.send(Err("Editor is busy or has an unfinished gesture/text edit. Finish it and retry.".into()));
                    return;
                }
                let _ = reply.send(Ok(Snapshot {
                    document: self.document.clone(),
                    revision: self.preview.revision,
                    phase: self.export_phase(),
                }));
            }
            Request::Apply {
                document,
                revision,
                replace,
                reply,
            } => {
                let result = self
                    .dispatch(
                        Action::ApplyPreparedDocument {
                            document: Box::new(document),
                            revision,
                            replace,
                        },
                        cx,
                    )
                    .map(|_| {
                        state(&Snapshot {
                            document: self.document.render_snapshot(),
                            revision: self.preview.revision,
                            phase: 0.,
                        })
                    });
                let _ = reply.send(result);
            }
        }
    }
}
