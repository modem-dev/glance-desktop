//! Single-line Unicode prompt input with IME, selection, clipboard and text undo.
use crate::text::Buffer;
use gpui::{prelude::*, *};
use std::ops::Range;
pub(in crate::editor) enum Event {
    Changed(String),
    Submit,
    Close,
}
pub(in crate::editor) struct PromptInput {
    pub(super) buffer: Buffer,
    focus: FocusHandle,
    return_focus: FocusHandle,
    disabled: bool,
    bounds: Bounds<Pixels>,
    line: Option<ShapedLine>,
    scroll: Pixels,
}
impl EventEmitter<Event> for PromptInput {}
impl PromptInput {
    pub fn new(return_focus: FocusHandle, cx: &mut Context<Self>) -> Self {
        Self {
            buffer: Buffer::default(),
            focus: cx.focus_handle(),
            return_focus,
            disabled: false,
            bounds: Bounds::default(),
            line: None,
            scroll: px(0.),
        }
    }
    pub fn has_focus(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
    pub fn focus(&self, window: &mut Window) {
        self.focus.focus(window);
    }
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.buffer.text() != text {
            self.buffer.select_all();
            self.buffer.replace(None, text);
            cx.notify();
        }
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(Event::Changed(self.buffer.text().to_owned()));
        cx.notify();
    }
    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if !self.buffer.selection().is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.text()[self.buffer.selection()].to_owned(),
            ));
            if cut && !self.disabled {
                self.buffer.delete(false);
                self.changed(cx);
            }
        }
    }
    fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.buffer.text().to_owned();
        let key = e.keystroke.key.as_str();
        let m = e.keystroke.modifiers;
        if self.buffer.marked.is_some() && matches!(key, "enter" | "escape") {
            return;
        }
        if key == "escape" {
            cx.emit(Event::Close);
            self.return_focus.focus(window);
        } else if !self.disabled {
            if crate::platform::command_pressed(m) {
                match key {
                    "a" => self.buffer.select_all(),
                    "c" | "x" => self.copy(key == "x", cx),
                    "v" => {
                        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                            self.replace_text_in_range(None, &text, window, cx);
                        }
                    }
                    "z" => self.buffer.history(m.shift),
                    "left" => self.buffer.move_to(0, m.shift),
                    "right" => self.buffer.move_to(self.buffer.text().len(), m.shift),
                    _ => return,
                }
            } else {
                match key {
                    "enter" => {
                        cx.emit(Event::Submit);
                        self.return_focus.focus(window);
                    }
                    "backspace" => self.buffer.delete(false),
                    "delete" => self.buffer.delete(true),
                    "left" | "right" => self.buffer.move_cursor(key == "right", m.shift),
                    "home" => self.buffer.move_to(0, m.shift),
                    "end" => self.buffer.move_to(self.buffer.text().len(), m.shift),
                    _ => return,
                }
            }
            if self.buffer.text() != before {
                self.changed(cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}
impl EntityInputHandler for PromptInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.buffer.utf16_to_byte_range(range);
        *actual = Some(self.buffer.byte_to_utf16_range(r.clone()));
        Some(self.buffer.text()[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.byte_to_utf16_range(self.buffer.selection()),
            reversed: self.buffer.selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked
            .clone()
            .map(|r| self.buffer.byte_to_utf16_range(r))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let replacing = range
            .clone()
            .map(|r| self.buffer.utf16_to_byte_range(r))
            .or(self.buffer.marked.clone())
            .unwrap_or(self.buffer.selection());
        let text = text.replace(['\r', '\n'], " ");
        if self.buffer.text().len() - replacing.len() + text.len()
            > crate::chatgpt::agent::MAX_PROMPT
        {
            return;
        }
        self.buffer.replace(range, &text);
        self.changed(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let before = self.buffer.text().to_owned();
        let replacing = range
            .clone()
            .map(|r| self.buffer.utf16_to_byte_range(r))
            .or(self.buffer.marked.clone())
            .unwrap_or(self.buffer.selection());
        let start = replacing.start;
        self.replace_text_in_range(range, text, window, cx);
        if self.buffer.text() == before {
            return;
        }
        let end = self.buffer.cursor();
        self.buffer.marked = (start != end).then_some(start..end);
        if let Some(selected) = selected {
            let prefix = self.buffer.utf16(start);
            let r = self
                .buffer
                .utf16_to_byte_range(prefix + selected.start..prefix + selected.end);
            self.buffer.select_range(r);
        }
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let l = self.line.as_ref()?;
        let r = self.buffer.utf16_to_byte_range(r);
        Some(Bounds::from_corners(
            point(
                self.bounds.left() + l.x_for_index(r.start) - self.scroll,
                self.bounds.top(),
            ),
            point(
                self.bounds.left() + l.x_for_index(r.end) - self.scroll,
                self.bounds.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.bounds.contains(&p) {
            return None;
        }
        Some(
            self.buffer.utf16(
                self.line
                    .as_ref()?
                    .closest_index_for_x(p.x - self.bounds.left() + self.scroll),
            ),
        )
    }
}
fn paint(entity: &Entity<PromptInput>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let this = entity.read(cx);
    let empty = this.buffer.text().is_empty();
    let text: SharedString = if empty {
        "What would you like to change?".into()
    } else {
        this.buffer.text().to_owned().into()
    };
    let line = window.text_system().shape_line(
        text.clone(),
        px(14.),
        &[TextRun {
            len: text.len(),
            font: font(crate::platform::UI_FONT),
            color: rgb(if empty { 0x697386 } else { 0x263044 }).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let bounds = Bounds::new(
        point(bounds.left() + px(12.), bounds.center().y - px(9.)),
        size((bounds.size.width - px(24.)).max(px(0.)), px(20.)),
    );
    let focused = this.focus.is_focused(window) && !this.disabled;
    let focus = this.focus.clone();
    let selection = this.buffer.selection();
    let cursor = this.buffer.cursor();
    let marked = this.buffer.marked.clone();
    let scroll = if focused {
        (line.x_for_index(cursor) - bounds.size.width + px(8.)).max(px(0.))
    } else {
        px(0.)
    };
    let origin = bounds.origin - point(scroll, px(0.));
    if focused {
        window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        if focused && !selection.is_empty() {
            window.paint_quad(fill(
                Bounds::from_corners(
                    point(origin.x + line.x_for_index(selection.start), origin.y),
                    point(origin.x + line.x_for_index(selection.end), bounds.bottom()),
                ),
                rgba(0x4c8dff45),
            ));
        }
        let _ = line.paint(origin, px(20.), window, cx);
        if focused && selection.is_empty() {
            window.paint_quad(fill(
                Bounds::new(
                    point(origin.x + line.x_for_index(cursor), origin.y),
                    size(px(1.5), px(19.)),
                ),
                rgb(0x263044),
            ));
        }
        if focused && let Some(r) = marked {
            window.paint_quad(fill(
                Bounds::new(
                    point(
                        origin.x + line.x_for_index(r.start),
                        bounds.bottom() - px(1.),
                    ),
                    size(line.x_for_index(r.end) - line.x_for_index(r.start), px(1.)),
                ),
                rgb(0x263044),
            ));
        }
    });
    entity.update(cx, |this, _| {
        this.bounds = bounds;
        this.line = Some(line);
        this.scroll = scroll;
    });
}
impl Render for PromptInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        div()
            .id("ask-prompt-input")
            .debug_selector(|| "ask-prompt-input".into())
            .w_full()
            .h(px(42.))
            .track_focus(&self.focus)
            .key_context("AskPrompt")
            .bg(rgb(if self.disabled { 0xf3f5f8 } else { 0xffffff }))
            .rounded_md()
            .border_1()
            .border_color(rgb(0xbcc5d3))
            .on_key_down(cx.listener(Self::key))
            .on_action(cx.listener(|this, _: &crate::menus::Copy, _, cx| {
                this.copy(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Paste, window, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    this.replace_text_in_range(None, &text, window, cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Undo, _, cx| {
                if !this.disabled {
                    this.buffer.history(false);
                    this.changed(cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Redo, _, cx| {
                if !this.disabled {
                    this.buffer.history(true);
                    this.changed(cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::SelectAll, _, cx| {
                this.buffer.select_all();
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::menus::Delete, _, cx| {
                if !this.disabled {
                    this.buffer.delete(false);
                    this.changed(cx);
                }
                cx.stop_propagation();
            }))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if !this.disabled {
                        this.focus.focus(window);
                        if e.click_count >= 2 {
                            this.buffer.select_all();
                        } else if let Some(line) = &this.line {
                            let i = line.closest_index_for_x(
                                e.position.x - this.bounds.left() + this.scroll,
                            );
                            this.buffer.move_to(i, e.modifiers.shift);
                        }
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .child(canvas(|b, _, _| b, move |b, _, w, cx| paint(&entity, b, w, cx)).size_full())
    }
}
