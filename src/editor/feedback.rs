use super::Editor;
use super::view::icon;
use gpui::{prelude::*, *};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyFeedback {
    Copying,
    ReadingText,
    TextCopied,
    NoText,
    Uploading,
    Copied,
    LinkCopied(u64),
}
impl CopyFeedback {
    fn complete(self) -> bool {
        matches!(
            self,
            Self::Copied | Self::TextCopied | Self::NoText | Self::LinkCopied(_)
        )
    }
    fn label(self) -> &'static str {
        match self {
            Self::Copying => "Copying…",
            Self::ReadingText => "Reading text…",
            Self::TextCopied => "Text copied!",
            Self::NoText => "No text found",
            Self::Uploading => "Uploading…",
            Self::Copied => "Copied!",
            Self::LinkCopied(_) => "Link copied!",
        }
    }
}
impl Editor {
    pub(super) fn set_copy_feedback(
        &mut self,
        feedback: Option<CopyFeedback>,
        cx: &mut Context<Self>,
    ) {
        // Dropping the previous task cancels its timeout, so it can't dismiss
        // a newer confirmation or an upload still in progress.
        self.feedback.timer = None;
        self.feedback.copy = feedback;
        if let Some(feedback) = feedback.filter(|feedback| feedback.complete()) {
            let seconds = if !matches!(feedback, CopyFeedback::LinkCopied(_)) {
                2
            } else {
                3
            };
            self.feedback.timer = Some(cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(seconds))
                    .await;
                let _ = view.update(cx, |editor, cx| {
                    editor.feedback.copy = None;
                    cx.notify();
                });
            }));
        }
        cx.notify();
    }
    pub(super) fn copy_confirmation(&self, feedback: CopyFeedback) -> impl IntoElement {
        let complete = feedback.complete();
        div()
            .id("copy-feedback")
            .debug_selector(|| "copy-feedback".into())
            .absolute()
            .occlude()
            .top(px(54.))
            .right(px(12.))
            .px_3()
            .py_2()
            .rounded_lg()
            .shadow_md()
            .bg(rgb(0x282b34))
            .text_color(rgb(0xffffff))
            .text_sm()
            .flex()
            .items_center()
            .gap_2()
            .child(icon(
                match feedback {
                    CopyFeedback::Copying => "copy",
                    CopyFeedback::ReadingText | CopyFeedback::NoText => "scan-text",
                    CopyFeedback::Uploading => "cloud-upload",
                    _ => "check",
                },
                if complete { 0x87e3b0 } else { 0xc6c9d3 },
            ))
            .child(div().child(feedback.label()).when_some(
                match feedback {
                    CopyFeedback::LinkCopied(minutes) => {
                        Some(format!("Glance · expires in {minutes} min"))
                    }
                    _ => None,
                },
                |el, detail| el.child(div().text_xs().text_color(rgb(0xc6c9d3)).child(detail)),
            ))
    }
}
