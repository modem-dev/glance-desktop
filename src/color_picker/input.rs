use super::*;
impl EntityInputHandler for ColorPicker {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.utf16_to_byte_range(range);
        *actual = Some(self.buffer.byte_to_utf16_range(range.clone()));
        Some(self.buffer.text()[range].into())
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
            .map(|range| self.buffer.byte_to_utf16_range(range))
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
        if !self.open || !self.editing {
            return;
        }
        if self.buffer.text().len().saturating_add(text.len()) > 64 {
            self.error = true;
            cx.notify();
            return;
        }
        self.buffer.replace(range, text);
        self.error = false;
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open || !self.editing || self.buffer.text().len().saturating_add(text.len()) > 64 {
            return;
        }
        let start = self.buffer.replace(range, text);
        let end = self.buffer.cursor();
        self.buffer.marked = (start != end).then_some(start..end);
        self.error = false;
        cx.notify();
        if let Some(selected) = selected {
            let prefix = self.buffer.utf16(start);
            let range = self
                .buffer
                .utf16_to_byte_range(prefix + selected.start..prefix + selected.end);
            self.buffer.select_range(range);
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.input_line.as_ref()?;
        let range = self.buffer.utf16_to_byte_range(range);
        Some(Bounds::from_corners(
            point(
                self.input_bounds.left() + line.x_for_index(range.start),
                self.input_bounds.top(),
            ),
            point(
                self.input_bounds.left() + line.x_for_index(range.end),
                self.input_bounds.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.input_bounds.contains(&point).then(|| {
            self.buffer
                .utf16(self.input_line.as_ref().map_or(0, |line| {
                    line.closest_index_for_x(point.x - self.input_bounds.left())
                }))
        })
    }
}
pub(super) fn paint(
    entity: &Entity<ColorPicker>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let theme = crate::theme::Theme::get(cx);
    let this = entity.read(cx);
    let text: SharedString = this.buffer.text().to_string().into();
    let line = window.text_system().shape_line(
        text.clone(),
        px(12.),
        &[TextRun {
            len: text.len(),
            font: font(crate::platform::UI_FONT),
            color: rgb(theme.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let origin = point(bounds.left() + px(6.), bounds.center().y - px(7.));
    let input_bounds = Bounds::new(
        origin,
        size((bounds.size.width - px(12.)).max(px(0.)), px(16.)),
    );
    let selection = this.buffer.selection();
    let cursor = this.buffer.cursor();
    let focused = this.editing && this.focus.is_focused(window);
    let focus = this.focus.clone();
    if focused {
        window.handle_input(
            &focus,
            ElementInputHandler::new(input_bounds, entity.clone()),
            cx,
        );
    }
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        if focused && !selection.is_empty() {
            window.paint_quad(fill(
                Bounds::from_corners(
                    point(origin.x + line.x_for_index(selection.start), origin.y),
                    point(
                        origin.x + line.x_for_index(selection.end),
                        input_bounds.bottom(),
                    ),
                ),
                rgba(0x4c8dff45),
            ));
        }
        let _ = line.paint(origin, px(16.), window, cx);
        if focused && selection.is_empty() {
            window.paint_quad(fill(
                Bounds::new(
                    point(origin.x + line.x_for_index(cursor), origin.y),
                    size(px(1.), px(16.)),
                ),
                rgb(theme.positive),
            ));
        }
    });
    entity.update(cx, |this, _| {
        this.input_bounds = input_bounds;
        this.input_line = Some(line);
    });
}
