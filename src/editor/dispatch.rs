//! The single entry point for application actions. Triggers only construct data.
use super::{
    Editor,
    actions::{Action, ActionReceipt, AnimationFormat, Panel},
};
use crate::{
    animation::Motion,
    backdrop::{Control, PRESETS},
    document::{Tool, actions::DocumentAction},
};
use gpui::{Bounds, ClipboardItem, Context, point, px, size};

impl Editor {
    /// UI adapters surface rejected actions in the status line.
    pub(super) fn dispatch_ui(&mut self, action: Action, cx: &mut Context<Self>) {
        if let Err(error) = self.dispatch(action, cx) {
            self.feedback.status = error;
            cx.notify();
        }
    }
    pub(crate) fn dispatch(
        &mut self,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Result<ActionReceipt, String> {
        if matches!(action, Action::Edit { .. })
            && (self.interaction.text_edit.is_some() || self.interaction.gesture.is_active())
        {
            return Err(
                "Finish the current text edit or gesture before editing the document.".into(),
            );
        }
        if let Action::ApplyPreparedDocument { revision, .. } = &action
            && (*revision != self.preview.revision
                || self.interaction.gesture.is_active()
                || self.interaction.text_edit.is_some())
        {
            return Err("Editor changed during operation. Read state and retry.".into());
        }
        let editing_text = self.interaction.text_edit.is_some();
        let contextual_text = editing_text
            && matches!(
                action,
                Action::Copy
                    | Action::Cut
                    | Action::Paste
                    | Action::Undo
                    | Action::Redo
                    | Action::Delete
                    | Action::SelectAll
            );
        if self.is_busy()
            && !contextual_text
            && !matches!(
                action,
                Action::Show
                    | Action::CancelExport
                    | Action::RevealExport
                    | Action::Help
                    | Action::Quit
                    | Action::ClosePanel { .. }
                    | Action::Cancel
                    | Action::ToggleChatgptAccountMenu
                    | Action::SignOutChatgpt
                    | Action::CancelChatgptSignIn
                    | Action::ManageChatgptUsage
            )
        {
            return Err("Editor is busy. Wait for the current operation to finish.".into());
        }
        // Resolve revision-scoped selection IDs before committing text or canceling gestures.
        let selection_indices = if let Action::SelectAnnotations { ids } = &action {
            if self.interaction.text_edit.is_some() {
                return Err("Finish the current text edit before selecting annotation IDs".into());
            }
            if ids.len() > 500 {
                return Err("Maximum 500 selected annotations".into());
            }
            Some(
                ids.iter()
                    .map(|id| {
                        let (revision, index) = id.split_once(':').ok_or("Invalid object id")?;
                        let revision: u64 = revision.parse().map_err(|_| "Invalid revision")?;
                        let index: usize = index.parse().map_err(|_| "Invalid index")?;
                        if revision != self.preview.revision || index >= self.document.marks.len() {
                            return Err("Stale object id; call get_document again".to_string());
                        }
                        Ok(index)
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            )
        } else {
            None
        };
        // Validate before committing text or canceling a gesture.
        match &action {
            Action::CopyOcr { rectangle, engine } => {
                crate::ocr::validate_rectangle(self.document.base.dimensions(), *rectangle)?;
                if engine.unwrap_or(self.chatgpt.snapshot.ocr_engine)
                    == crate::chatgpt::OcrEngine::Chatgpt
                {
                    if self.chatgpt.busy {
                        return Err("Wait for the current ChatGPT account operation".into());
                    }
                    if !self.chatgpt.snapshot.can_infer() {
                        return Err(
                            "Continue with ChatGPT and choose an available model first".into()
                        );
                    }
                }
                if self.interaction.text_edit.is_some() || self.interaction.gesture.is_active() {
                    return Err(
                        "Finish the current annotation text or gesture before extracting text"
                            .into(),
                    );
                }
            }
            Action::ChatgptSignIn { .. }
            | Action::SelectChatgptAccount { .. }
            | Action::SetChatgptModel { .. }
            | Action::SetOcrEngine { .. }
            | Action::SignOutChatgpt
            | Action::DismissChatgptWelcome
                if self.chatgpt.busy =>
            {
                return Err(
                    "Wait for the current ChatGPT account operation or cancel sign-in".into(),
                );
            }
            Action::ChatgptSignIn {
                account_id: Some(id),
            }
            | Action::SelectChatgptAccount { account_id: id }
                if !self.chatgpt.snapshot.accounts.iter().any(|a| &a.id == id) =>
            {
                return Err("Unknown ChatGPT account".into());
            }
            Action::SetChatgptModel { model }
                if !self
                    .chatgpt
                    .snapshot
                    .models
                    .iter()
                    .any(|m| &m.slug == model) =>
            {
                return Err("Choose a model available to this ChatGPT account".into());
            }
            Action::SetOcrEngine {
                engine: crate::chatgpt::OcrEngine::Chatgpt,
            } if !self.chatgpt.snapshot.can_infer() => {
                return Err("Continue with ChatGPT and choose an available model first".into());
            }
            Action::SignOutChatgpt | Action::DismissChatgptWelcome
                if self.chatgpt.snapshot.active_account.is_none() =>
            {
                return Err("No ChatGPT account selected".into());
            }
            Action::RandomizeMotion { .. }
                if self
                    .document
                    .backdrop
                    .or(self.panels.backdrop_disabled)
                    .is_none_or(|b| b.motion == Motion::Still) =>
            {
                return Err("Choose a backdrop motion before randomizing it.".into());
            }
            Action::SetStrokeWidth { width }
                if !width.is_finite() || !(0.5..=64.).contains(width) =>
            {
                return Err("Stroke width must be 0.5..64".into());
            }
            Action::SelectRegion { rectangle, .. }
                if ![rectangle.0, rectangle.1, rectangle.2, rectangle.3]
                    .iter()
                    .all(|n| n.is_finite())
                    || rectangle.2 < 0.
                    || rectangle.3 < 0. =>
            {
                return Err(
                    "Selection rectangle must have finite coordinates and nonnegative dimensions"
                        .into(),
                );
            }
            Action::SetAppearance { style } => style.validate()?,
            Action::SampleToolColor { .. }
            | Action::PickToolScreenColor
            | Action::BeginToolColorSampling
                if matches!(
                    self.options_tool(),
                    Tool::Select | Tool::Crop | Tool::Pixelate
                ) =>
            {
                return Err("Select an annotation or a tool with a color option".into());
            }
            Action::SetImageAnimation { animation } => animation.validate()?,
            Action::SelectEntrance { effect } => {
                let mut animation = self.document.image_animation;
                if !animation.enabled() {
                    animation.seconds = self.document.animation_seconds();
                }
                animation.effect = *effect;
                animation.validate()?;
            }
            Action::SetAnimationControl { control, value } => {
                let mut animation = self.document.image_animation;
                let (min, max, _) = control.range(animation);
                if !(min..=max).contains(value) {
                    return Err("Animation control out of range".into());
                }
                control.set(&mut animation, *value);
                animation.validate()?;
            }
            Action::SeekAnimation { seconds }
                if !seconds.is_finite()
                    || !(0. ..=self.document.animation_seconds() as f32).contains(seconds) =>
            {
                return Err("Preview time must lie within the clip".into());
            }
            Action::BeginAnimationAdjustment {
                track, position, ..
            } if ![track.0, track.1, track.2, track.3, position.0, position.1]
                .iter()
                .all(|n| n.is_finite())
                || track.2 <= 0.
                || track.3 <= 0. =>
            {
                return Err("Invalid animation slider bounds".into());
            }
            Action::SetMagnifierZoom { zoom }
                if !zoom.is_finite() || !(1.5..=4.).contains(zoom) =>
            {
                return Err("Magnification must be 1.5..4".into());
            }
            Action::SetCounterNumber { number } if !(1..=999).contains(number) => {
                return Err("Step number must be 1..999".into());
            }
            Action::SetCropRatio { ratio: Some(ratio) }
                if !ratio.is_finite() || !(0.1..=10.).contains(ratio) =>
            {
                return Err("Crop ratio must be 0.1..10".into());
            }
            Action::Resize { scale, .. } | Action::SetResizeScale { scale }
                if !scale.is_finite() || !(0.1..=4.).contains(scale) =>
            {
                return Err("Resize scale must be 0.1..4".into());
            }
            Action::Zoom { factor } | Action::ZoomAt { factor, .. }
                if !factor.is_finite() || *factor <= 0. =>
            {
                return Err("Zoom factor must be positive and finite".into());
            }
            Action::ZoomAt { anchor, .. }
            | Action::NudgeSelection { delta: anchor, .. }
            | Action::PanBy { delta: anchor }
                if !anchor.0.is_finite() || !anchor.1.is_finite() =>
            {
                return Err("Coordinates must be finite".into());
            }
            Action::SetBackdropPreset { preset } if *preset >= PRESETS.len() => {
                return Err("Invalid backdrop preset".into());
            }
            Action::SetBackdropColor { stop, .. }
            | Action::SampleBackdropColor { stop, .. }
            | Action::PickBackdropScreenColor { stop }
            | Action::BeginBackdropColorSampling { stop }
                if *stop > 1 =>
            {
                return Err("Backdrop color stop must be 0 or 1".into());
            }
            Action::SampleBackdropColor {
                position: (x, y), ..
            }
            | Action::SampleToolColor { position: (x, y) }
                if *x >= self.document.base.width() || *y >= self.document.base.height() =>
            {
                return Err("Sample position must be inside the source image".into());
            }
            Action::SetBackdrop { backdrop: Some(b) } => {
                if b.preset >= PRESETS.len()
                    || b.padding > 512
                    || b.inner_radius > 256
                    || b.inside_padding > 512
                    || b.shadow > 128
                    || !(2..=15).contains(&b.seconds)
                {
                    return Err("Backdrop values out of range".into());
                }
                let mut animation = self.document.image_animation;
                animation.seconds = b.seconds;
                animation.validate()?;
            }
            Action::SetBackdropControl { control, value } => {
                if *value < control.min() || *value > control.max() {
                    return Err("Backdrop control value out of range".into());
                }
                if *control == Control::Duration {
                    let mut animation = self.document.image_animation;
                    animation.seconds = *value;
                    animation.validate()?;
                }
            }
            Action::BeginBackdropAdjustment {
                track, position, ..
            } if ![track.0, track.1, track.2, track.3, position.0, position.1]
                .iter()
                .all(|n| n.is_finite())
                || track.2 <= 0.
                || track.3 <= 0. =>
            {
                return Err("Invalid slider bounds".into());
            }
            _ => {}
        }
        if !matches!(
            action,
            Action::BeginBackdropColorSampling { .. }
                | Action::SampleBackdropColor { .. }
                | Action::BeginToolColorSampling
                | Action::SampleToolColor { .. }
                | Action::Fit
                | Action::ActualSize
                | Action::Zoom { .. }
                | Action::ZoomAt { .. }
                | Action::PanBy { .. }
                | Action::Show
                | Action::CommitText
        ) {
            self.panels.sampling_color = None;
            self.panels.sampling_tool_color = false;
        }
        let previous_operation = self.operations.active.as_ref().map(|op| op.id);
        match action {
            Action::Edit { edit } => self.edit_document(edit, cx)?,
            Action::ApplyPreparedDocument {
                document, replace, ..
            } => {
                self.document = *document;
                self.set_selection(
                    self.document
                        .marks
                        .len()
                        .checked_sub(1)
                        .into_iter()
                        .collect(),
                );
                self.interaction.tool = Tool::Select;
                if replace {
                    self.viewport.zoom = None;
                    self.viewport.pan = (0., 0.);
                }
                self.playback.position = 0.;
                self.playback.epoch = std::time::Instant::now();
                self.preview.mark_count = usize::MAX;
                self.changed();
            }
            Action::Show => crate::platform::show_editor(cx),
            Action::Capture { area } => self.capture(area, cx)?,
            Action::OpenImage => self.open(cx),
            Action::OpenPath { path } => self.open_path(path, cx),
            Action::SaveImage => self.export(true, cx),
            Action::CopyImage => self.export(false, cx),
            Action::CopyRemote => self.copy_remote(cx),
            Action::PasteImage => self.paste_image(cx),
            Action::CopyOcr { rectangle, engine } => {
                self.cancel_gesture();
                let rectangle =
                    crate::ocr::validate_rectangle(self.document.base.dimensions(), rectangle)?;
                let engine = engine.unwrap_or(self.chatgpt.snapshot.ocr_engine);
                let revision = self.preview.revision;
                let image = self.document.base.clone();
                let id = self
                    .start_operation(super::jobs::OperationKind::Ocr)
                    .ok_or("Editor is busy")?;
                self.extraction = None;
                self.feedback.status = if engine == crate::chatgpt::OcrEngine::Chatgpt {
                    "Reading text with ChatGPT · Using ChatGPT plan…"
                } else {
                    "Reading text locally…"
                }
                .into();
                self.set_copy_feedback(Some(super::feedback::CopyFeedback::ReadingText), cx);
                let runtime = self.chatgpt.runtime.clone();
                if engine == crate::chatgpt::OcrEngine::Chatgpt {
                    self.chatgpt.inference_cancel =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                }
                let cancel = self.chatgpt.inference_cancel.clone();
                let account = self
                    .chatgpt
                    .snapshot
                    .active_account
                    .clone()
                    .unwrap_or_default();
                let model = self.chatgpt.snapshot.model.clone().unwrap_or_default();
                #[cfg(not(test))]
                let recognize = crate::ocr::extract;
                #[cfg(test)]
                let recognize = self.recognize_text;
                self.spawn_operation(id, move || {
                    let result = if engine == crate::chatgpt::OcrEngine::Chatgpt {
                        runtime
                            .lock()
                            .map_err(|_| "ChatGPT account service stopped".to_string())
                            .and_then(|mut service| {
                                service.recognize(&image, rectangle, &account, &model, &cancel)
                            })
                    } else {
                        recognize(&image, Some(rectangle))
                    };
                    let result = if engine == crate::chatgpt::OcrEngine::Chatgpt
                        && cancel.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        Err("ChatGPT OCR canceled".into())
                    } else {
                        result
                    };
                    super::jobs::OperationResult::ExtractedText {
                        revision,
                        rectangle,
                        result,
                        engine,
                    }
                });
            }
            Action::ToggleChatgptAccountMenu => self.chatgpt.menu = !self.chatgpt.menu,
            Action::ChatgptSignIn { account_id } => {
                self.chatgpt_job(super::chatgpt::Job::SignIn(account_id), cx)?
            }
            Action::CancelChatgptSignIn => {
                if self.chatgpt.signing_in {
                    self.chatgpt
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    self.chatgpt.status = "Canceling ChatGPT sign-in…".into();
                }
            }
            Action::SelectChatgptAccount { account_id } => {
                self.chatgpt_job(super::chatgpt::Job::Account(account_id), cx)?
            }
            Action::SignOutChatgpt => {
                self.chatgpt
                    .inference_cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                self.chatgpt_job(super::chatgpt::Job::SignOut, cx)?;
            }
            Action::SetOcrEngine { engine } => {
                self.chatgpt_job(super::chatgpt::Job::Engine(engine), cx)?
            }
            Action::SetChatgptModel { model } => {
                self.chatgpt_job(super::chatgpt::Job::Model(model), cx)?
            }
            Action::ManageChatgptUsage => cx.open_url("https://chatgpt.com/settings/usage"),
            Action::DismissChatgptWelcome => self.chatgpt_job(super::chatgpt::Job::Welcome, cx)?,
            Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::Undo
            | Action::Redo
            | Action::Delete
            | Action::SelectAll
                if contextual_text =>
            {
                let edit = self.interaction.text_edit.as_mut().unwrap();
                match action {
                    Action::Copy | Action::Cut => {
                        let range = edit.buffer.selection();
                        if !range.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                edit.buffer.text()[range].into(),
                            ));
                            if matches!(action, Action::Cut) {
                                edit.replace_text("");
                            }
                        }
                    }
                    Action::Paste => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            edit.replace_text(&text);
                        }
                    }
                    Action::Undo | Action::Redo => {
                        edit.buffer.history(matches!(action, Action::Redo))
                    }
                    Action::Delete => edit.buffer.delete(false),
                    Action::SelectAll => edit.buffer.select_all(),
                    _ => unreachable!(),
                }
                edit.caret_on = true;
            }
            Action::Copy => self.export(false, cx),
            Action::Cut => return Err("Cut requires an active text edit".into()),
            Action::Paste => self.paste_image(cx),
            Action::Undo => self.history(false, cx),
            Action::Redo => self.history(true, cx),
            Action::Delete => self.delete_selected(cx),
            Action::DuplicateSelection => self.duplicate_selected(cx),
            Action::SelectAll => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.interaction.tool = Tool::Select;
                self.panels.animation = false;
                self.set_selection((0..self.document.marks.len()).collect());
            }
            Action::SelectAnnotations { .. } => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.interaction.tool = Tool::Select;
                self.panels.animation = false;
                self.set_selection(selection_indices.unwrap());
            }
            Action::SelectRegion {
                rectangle,
                additive,
            } => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.interaction.tool = Tool::Select;
                self.panels.animation = false;
                self.select_region(rectangle, additive);
            }
            Action::SelectTool { tool } => self.set_tool(tool, cx),
            Action::SetColor { color } => {
                self.interaction.color = color;
                self.apply_style(true, cx);
            }
            Action::SampleToolColor { position: (x, y) } => {
                let pixel = self.document.base.get_pixel(x, y).0;
                let mut color = self.tool_settings().color;
                color[..3].copy_from_slice(&pixel[..3]);
                self.interaction.color = color;
                self.apply_style(true, cx);
                self.panels.sampling_tool_color = false;
            }
            Action::BeginToolColorSampling => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.panels.sampling_color = None;
                self.panels.sampling_tool_color = true;
                self.feedback.status =
                    "Click a pixel in the image to pick its color • Escape to cancel".into();
            }
            Action::PickToolScreenColor => self.pick_screen_color(None, cx)?,
            Action::SetStrokeWidth { width } => {
                self.interaction.width = width;
                self.apply_style(false, cx);
            }
            Action::SetAppearance { style } => self.set_appearance(style, cx)?,
            Action::SetMagnifierZoom { zoom } => self.set_magnifier_zoom(zoom, cx)?,
            Action::SetCounterNumber { number } => self.set_counter_number(number, cx)?,
            Action::SetCropRatio { ratio } => self.interaction.crop_ratio = ratio,
            Action::AddLinePoint => self.line_point(false, cx)?,
            Action::StraightenLine => self.line_point(true, cx)?,
            Action::CycleStrokeWidth => {
                self.interaction.width = match self.interaction.width as u32 {
                    3 => 5.,
                    5 => 9.,
                    _ => 3.,
                };
                self.apply_style(false, cx);
            }
            Action::CycleMagnifierZoom => {
                let next = match self.tool_settings().magnification as u32 {
                    2 => 3.,
                    3 => 4.,
                    _ => 2.,
                };
                self.set_magnifier_zoom(next, cx)?;
            }
            Action::NudgeSelection { delta, remember } => {
                self.cancel_gesture();
                self.update_selected(remember, |mark| mark.translate(delta.0, delta.1), cx)?;
            }

            Action::Fit | Action::ActualSize => {
                self.viewport.zoom = matches!(action, Action::ActualSize).then_some(1.);
                self.viewport.pan = (0., 0.);
            }
            Action::Zoom { factor } => self.change_zoom(factor, cx),
            Action::ZoomAt { factor, anchor } => self.zoom_at(factor, anchor, cx),
            Action::PanBy { delta } => {
                self.viewport.pan.0 += delta.0;
                self.viewport.pan.1 += delta.1;
            }
            Action::ToggleBackdrop => self.toggle_backdrop(cx),
            Action::ToggleEnhance => self.toggle_enhance(cx),
            Action::ToggleAnimationPanel => self.toggle_animation_panel(cx),
            Action::ClosePanel { panel } => {
                self.panels.popup = None;
                self.panels.sampling_color = None;
                self.panels.sampling_tool_color = false;
                match panel {
                    Panel::Backdrop => self.panels.backdrop = false,
                    Panel::Enhance => self.panels.enhance = false,
                    Panel::Animation => self.panels.animation = false,
                }
            }
            Action::SetResizeScale { scale } => self.panels.resize_scale = scale,
            Action::ToggleSmartResize => self.panels.resize_smart = !self.panels.resize_smart,
            Action::ApplyResize => self.resize_image(false, cx),
            Action::Resize { scale, smart } => {
                self.panels.resize_scale = scale;
                self.panels.resize_smart = smart;
                self.resize_image(false, cx);
            }
            Action::Rotate => self.resize_image(true, cx),
            Action::SetBackdrop { backdrop } => {
                self.commit_text(cx);
                self.cancel_gesture();
                if let Some(b) = backdrop {
                    self.backdrop_style(|current| *current = b, cx);
                } else if self.document.backdrop.is_some() {
                    self.edit_document(DocumentAction::SetBackdrop { backdrop: None }, cx)?;
                    self.feedback.status = "Backdrop removed • ⌘Z to restore".into();
                    self.panels.backdrop = false;
                }
            }
            Action::SetBackdropFormat { format } => self.backdrop_style(|b| b.format = format, cx),
            Action::ToggleBackdropEnabled => {
                self.commit_text(cx);
                self.cancel_gesture();
                let current = self.document.backdrop;
                let backdrop = if current.is_some() {
                    None
                } else {
                    let mut b = self.panels.backdrop_disabled.unwrap_or_default();
                    if self.document.image_animation.enabled() {
                        b.seconds = self.document.animation_seconds();
                    }
                    Some(b)
                };
                self.edit_document(DocumentAction::SetBackdrop { backdrop }, cx)?;
                self.panels.backdrop_disabled = current;
            }
            Action::SetBackdropFill { gradient } => self.backdrop_style(
                |b| {
                    b.gradient = gradient;
                    b.motion = Motion::Still;
                },
                cx,
            ),
            Action::SelectMotion { motion } => self.backdrop_style(
                |b| {
                    if b.motion != motion
                        && b.colors.is_none()
                        && let Some(preset) = motion.suggested_preset()
                    {
                        b.preset = preset;
                    }
                    b.motion = motion;
                    b.gradient = true;
                },
                cx,
            ),
            Action::RandomizeMotion { seed } => self.backdrop_style(
                |b| {
                    b.seed = seed.unwrap_or_else(|| {
                        loop {
                            let candidate = rand::random::<u32>();
                            if candidate != 0 && candidate != b.seed {
                                break candidate;
                            }
                        }
                    });
                },
                cx,
            ),
            Action::SetBackdropPreset { preset } => self.backdrop_style(
                |b| {
                    b.preset = preset;
                    b.colors = None;
                },
                cx,
            ),
            Action::SetBackdropColor { stop, rgb } => {
                self.backdrop_style(|b| b.set_color(stop, rgb), cx)
            }
            Action::SampleBackdropColor {
                stop,
                position: (x, y),
            } => {
                let pixel = self.document.base.get_pixel(x, y).0;
                self.backdrop_style(|b| b.set_color(stop, [pixel[0], pixel[1], pixel[2]]), cx);
                self.panels.sampling_color = None;
                self.panels.sampling_tool_color = false;
            }
            Action::BeginBackdropColorSampling { stop } => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.panels.sampling_color = Some(stop);
                self.panels.sampling_tool_color = false;
                self.feedback.status =
                    "Click a pixel in the image to pick its color • Escape to cancel".into();
            }
            Action::PickBackdropScreenColor { stop } => {
                self.pick_backdrop_screen_color(stop, cx)?
            }
            Action::SetBackdropControl { control, value } => {
                let phase = self.animation_phase();
                let adjusting = matches!(
                    self.interaction.gesture,
                    super::state::Gesture::AdjustingBackdrop(..)
                );
                if !adjusting {
                    self.commit_text(cx);
                    self.cancel_gesture();
                    self.document.remember();
                }
                let enabling = self.document.backdrop.is_none();
                let previous_padding = self.document.backdrop.map_or(0, |b| b.inside_padding);
                let b = self.document.backdrop.get_or_insert_with(|| {
                    self.panels.backdrop_disabled.take().unwrap_or_default()
                });
                let previous = *b;
                control.set(b, value);
                let changed = enabling || previous != *b;
                let padding_changed = previous_padding != b.inside_padding;
                if control == Control::Duration {
                    self.document.image_animation.seconds = b.seconds;
                    self.playback.position = phase * b.seconds as f32;
                    self.playback.epoch = std::time::Instant::now();
                }
                if changed {
                    self.preview.revision += 1;
                    if padding_changed {
                        self.schedule_preview();
                    }
                }
            }
            Action::BeginBackdropAdjustment {
                control,
                track,
                position,
            } => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.document.remember();
                if self.document.backdrop.is_none() {
                    self.preview.revision += 1;
                }
                let previous_padding = self.document.backdrop.map_or(0, |b| b.inside_padding);
                self.document.backdrop.get_or_insert_with(|| {
                    self.panels.backdrop_disabled.take().unwrap_or_default()
                });
                self.interaction.gesture = super::state::Gesture::AdjustingBackdrop(
                    control,
                    Bounds::new(
                        point(px(track.0), px(track.1)),
                        size(px(track.2), px(track.3)),
                    ),
                );
                self.backdrop_slider_move(point(px(position.0), px(position.1)), cx);
                if previous_padding != self.document.backdrop.map_or(0, |b| b.inside_padding) {
                    self.schedule_preview();
                }
            }
            Action::TogglePlayback => self.toggle_animation(cx),
            Action::ReplayAnimation => self.replay_animation(cx),
            Action::SeekAnimation { seconds } => {
                self.playback.position = seconds;
                self.playback.epoch = std::time::Instant::now();
                self.playback.paused = true;
                self.playback.seek = self.playback.seek.wrapping_add(1);
            }
            Action::SelectEntrance { effect } => {
                self.commit_text(cx);
                self.cancel_gesture();
                let mut animation = self.document.image_animation;
                if !animation.enabled() {
                    animation.seconds = self.document.animation_seconds();
                }
                animation.effect = effect;
                self.edit_document(DocumentAction::SetImageAnimation { animation }, cx)?;
                self.panels.animation = true;
                self.panels.backdrop = false;
                self.panels.enhance = false;
                self.set_selection(Vec::new());
                self.replay_animation(cx);
            }
            Action::SetImageAnimation { animation } => {
                self.commit_text(cx);
                self.cancel_gesture();
                self.edit_document(DocumentAction::SetImageAnimation { animation }, cx)?;
            }
            Action::SetAnimationControl { control, value } => {
                if control == crate::animation::AnimationControl::Time {
                    self.playback.position = value as f32 / 1000.;
                    self.playback.paused = true;
                    self.playback.seek = self.playback.seek.wrapping_add(1);
                } else {
                    let mut animation = self.document.image_animation;
                    control.set(&mut animation, value);
                    if matches!(
                        self.interaction.gesture,
                        super::state::Gesture::AdjustingAnimation(..)
                    ) {
                        if self.document.image_animation != animation {
                            self.document.image_animation = animation;
                            if let Some(b) = &mut self.document.backdrop {
                                b.seconds = animation.seconds;
                            }
                            self.preview.revision += 1;
                            self.replay_animation(cx);
                        }
                    } else {
                        self.commit_text(cx);
                        self.cancel_gesture();
                        self.edit_document(DocumentAction::SetImageAnimation { animation }, cx)?;
                    }
                }
            }
            Action::BeginAnimationAdjustment {
                control,
                track,
                position,
            } => {
                self.commit_text(cx);
                self.cancel_gesture();
                if control != crate::animation::AnimationControl::Time {
                    self.document.remember();
                }
                self.interaction.gesture = super::state::Gesture::AdjustingAnimation(
                    control,
                    Bounds::new(
                        point(px(track.0), px(track.1)),
                        size(px(track.2), px(track.3)),
                    ),
                );
                self.animation_slider_move(point(px(position.0), px(position.1)), cx);
            }
            Action::ExportAnimation { format } => match format {
                AnimationFormat::Mp4 => self.export_video(cx),
                AnimationFormat::Gif => self.export_gif(cx),
            },
            Action::CancelExport => self.cancel_video(cx),
            Action::RevealExport => {
                if let Some(path) = &self.video_export.last_video {
                    cx.reveal_path(path);
                }
            }
            Action::CommitText => self.commit_text(cx),
            Action::Cancel => {
                self.panels.sampling_color = None;
                self.panels.sampling_tool_color = false;
                if self.video_export.cancel.is_some() {
                    self.cancel_video(cx);
                } else if self.interaction.text_edit.take().is_some() {
                    self.feedback.status = "Text canceled".into();
                } else {
                    self.cancel_gesture();
                    self.set_selection(Vec::new());
                }
            }
            Action::Help => {
                cx.open_url("https://github.com/modem-dev/glance-desktop/blob/main/docs/usage.md")
            }
            Action::Quit => cx.quit(),
        }
        cx.notify();
        Ok(ActionReceipt {
            revision: self.preview.revision,
            operation_id: self
                .operations
                .active
                .as_ref()
                .filter(|op| Some(op.id) != previous_operation)
                .map(|op| op.id.value()),
        })
    }
}
