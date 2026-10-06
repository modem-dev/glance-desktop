//! Native text input and inline text painting for the editor.
use super::Editor;
use super::state::Layout;
use gpui::*;
use std::ops::Range;
impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let edit = self.interaction.text_edit.as_ref()?;
        let range = edit.buffer.utf16_to_byte_range(range);
        *actual = Some(edit.buffer.byte_to_utf16_range(range.clone()));
        Some(edit.buffer.text()[range].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let buffer = &self.interaction.text_edit.as_ref()?.buffer;
        Some(UTF16Selection {
            range: buffer.byte_to_utf16_range(buffer.selection()),
            reversed: buffer.selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let buffer = &self.interaction.text_edit.as_ref()?.buffer;
        buffer
            .marked
            .clone()
            .map(|range| buffer.byte_to_utf16_range(range))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.interaction.text_edit {
            edit.buffer.marked = None;
            cx.notify();
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = &mut self.interaction.text_edit {
            edit.buffer.replace(range, text);
            edit.caret_on = true;
            cx.notify();
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit) = &mut self.interaction.text_edit {
            let start = edit.buffer.replace(range, text);
            let end = edit.buffer.cursor();
            if start != end {
                edit.buffer.marked = Some(start..end);
            }
            if let Some(selected) = selected {
                let prefix = edit.buffer.utf16(start);
                let range = edit
                    .buffer
                    .utf16_to_byte_range(prefix + selected.start..prefix + selected.end);
                edit.buffer.select_range(range);
            }
            edit.caret_on = true;
            cx.notify();
        }
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let edit = self.interaction.text_edit.as_ref()?;
        let bounds = edit.bounds?;
        let line = edit.line.as_ref()?;
        let range = edit.buffer.utf16_to_byte_range(range);
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - edit.scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - edit.scroll,
                bounds.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let edit = self.interaction.text_edit.as_ref()?;
        if !edit.bounds?.contains(&p) {
            return None;
        }
        Some(edit.buffer.utf16(edit.index(p)))
    }
}

pub fn paint(
    entity: &Entity<Editor>,
    layout: Layout,
    image_bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(edit) = entity.read(cx).interaction.text_edit.as_ref() else {
        return;
    };
    let font_size = px((edit.mark.width * 7.).max(1.) * layout.scale);
    let text = edit.buffer.text();
    let empty = text.is_empty();
    let content: SharedString = if empty {
        "Type here…".into()
    } else {
        text.to_string().into()
    };
    let color = if empty {
        rgb(0x858995)
    } else {
        rgb((edit.mark.color[0] as u32) << 16
            | (edit.mark.color[1] as u32) << 8
            | edit.mark.color[2] as u32)
    };
    let run = TextRun {
        len: content.len(),
        font: font(crate::platform::annotation_font_family()),
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(content, font_size, &[run], None);
    let p = edit.mark.points[0];
    let origin = point(
        px(layout.x + p.0 * layout.scale),
        px(layout.y + p.1 * layout.scale),
    );
    let width = (line.width + px(16.))
        .max(px(180.))
        .min((image_bounds.right() - origin.x).max(px(16.)));
    let bounds = Bounds::new(origin, size(width, font_size * 1.25));
    let cursor = line.x_for_index(edit.buffer.cursor());
    let scroll = (cursor - width + px(12.)).max(px(0.));
    let paint_origin = origin - point(scroll, px(0.));
    let selection = edit.buffer.selection();
    let caret = edit.caret_on;
    let marked = edit.buffer.marked.clone();
    window.paint_quad(quad(
        bounds.dilate(px(5.)),
        px(3.),
        gpui::transparent_black(),
        px(1.),
        rgb(0xf35d45),
        Default::default(),
    ));
    let focus = entity.read(cx).focus.clone();
    window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
    window.with_content_mask(
        Some(ContentMask {
            bounds: bounds.intersect(&image_bounds),
        }),
        |window| {
            if !selection.is_empty() {
                window.paint_quad(fill(
                    Bounds::from_corners(
                        point(paint_origin.x + line.x_for_index(selection.start), origin.y),
                        point(
                            paint_origin.x + line.x_for_index(selection.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x4c8dff45),
                ));
            }
            let _ = line.paint(paint_origin, font_size, window, cx);
            if let Some(marked) = marked {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            paint_origin.x + line.x_for_index(marked.start),
                            bounds.bottom() - px(2.),
                        ),
                        size(
                            line.x_for_index(marked.end) - line.x_for_index(marked.start),
                            px(1.),
                        ),
                    ),
                    color,
                ));
            }
            if selection.is_empty() && caret {
                window.paint_quad(fill(
                    Bounds::new(
                        point(paint_origin.x + cursor, origin.y + px(2.)),
                        size(px(1.5), font_size * 1.15),
                    ),
                    rgb(0xf35d45),
                ));
            }
        },
    );
    entity.update(cx, |editor, _| {
        if let Some(edit) = &mut editor.interaction.text_edit {
            edit.bounds = Some(bounds);
            edit.line = Some(line);
            edit.scroll = scroll;
        }
    });
}
