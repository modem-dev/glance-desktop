//! One pointer gesture at a time on the canvas: drawing, moving, selecting, panning.
use super::{Editor, TextDraft, actions::Action};
use crate::{
    arrow,
    document::{
        Mark, Tool,
        actions::{DocumentAction, MAX_DOCUMENT_POINTS, MAX_MARK_POINTS},
    },
    navigation,
};
use rune::{Drag, Phase, TextEdit, Tx};

pub(crate) struct AnnotationDrag {
    pub index: usize,
    origin: (f32, f32),
    original: Mark,
    pub moved: Mark,
}

impl AnnotationDrag {
    fn new(mark: &Mark, index: usize, origin: (f32, f32)) -> Self {
        Self {
            index,
            origin,
            original: mark.clone(),
            moved: mark.clone(),
        }
    }

    fn update(&mut self, point: (f32, f32), shift: bool, handle: Option<usize>) {
        self.moved = self.original.clone();
        let delta = navigation::translation(self.origin, point, shift && handle.is_none());
        arrow::drag(&mut self.moved, handle, delta, shift);
    }
}

#[derive(Default)]
pub(crate) enum Gesture {
    #[default]
    Idle,
    Drawing(Mark),
    /// A selected mark moving, or one of its handles when `handle` is set.
    Moving {
        drag: AnnotationDrag,
        handle: Option<usize>,
    },
    MovingSelection(Vec<AnnotationDrag>),
    Selecting {
        origin: (f32, f32),
        current: (f32, f32),
        additive: bool,
        previous: Vec<usize>,
    },
    Panning {
        last: (f32, f32),
    },
    /// The press only zoomed, committed a label or found nothing to do.
    Ignored,
}

impl Gesture {
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn first_drag_index(&self) -> Option<usize> {
        match self {
            Self::MovingSelection(drags) => drags.iter().map(|d| d.index).min(),
            Self::Moving { drag, .. } => Some(drag.index),
            _ => None,
        }
    }

    pub fn moved_mark(&self, index: usize) -> Option<&Mark> {
        match self {
            Self::MovingSelection(drags) => {
                drags.iter().find(|d| d.index == index).map(|d| &d.moved)
            }
            Self::Moving { drag, .. } if drag.index == index => Some(&drag.moved),
            _ => None,
        }
    }

    pub fn draft(&self) -> Option<&Mark> {
        match self {
            Self::Drawing(mark) => Some(mark),
            _ => None,
        }
    }
}

fn selection_rectangle(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0 - b.0).abs(),
        (a.1 - b.1).abs(),
    )
}

impl Editor {
    /// Canvas points to image pixels. Without `clamp`, a point outside the image is None.
    fn coordinate(&self, x: f32, y: f32, clamp: bool) -> Option<(f32, f32)> {
        let layout = self.layout();
        if layout.scale <= 0. {
            return None;
        }
        let px = (x - layout.x) / layout.scale;
        let py = (y - layout.y) / layout.scale;
        if !clamp && (px < 0. || py < 0. || px >= layout.width || py >= layout.height) {
            return None;
        }
        Some((
            px.clamp(0., layout.width - 1.),
            py.clamp(0., layout.height - 1.),
        ))
    }

    pub(crate) fn on_canvas_drag(&mut self, drag: &mut Drag, tx: &mut Tx) {
        let shift = tx.mods().shift;
        // A press on the canvas leaves the Animation panel for editing; that press
        // draws nothing.
        if matches!(drag.phase, Phase::Down) && self.panel == super::Panel::Animation {
            self.dispatch_ui(super::actions::Action::ClosePanel, tx);
            drag.data = 1.;
            return;
        }
        if drag.data == 1. {
            return;
        }
        match drag.phase {
            Phase::Down => self.begin(drag.x, drag.y, shift, tx),
            Phase::Move => self.motion(drag.x, drag.y, shift),
            Phase::Up if drag.cancelled() => self.cancel_gesture(),
            Phase::Up => self.finish(drag.x, drag.y, shift, tx),
        }
    }

    /// A middle press pans, whatever the tool.
    pub(crate) fn on_canvas_pan_drag(&mut self, drag: &mut Drag) {
        match drag.phase {
            Phase::Down => {
                self.cancel_gesture();
                self.gesture = Gesture::Panning {
                    last: (drag.x, drag.y),
                };
            }
            Phase::Move => self.pan_to(drag.x, drag.y),
            _ => self.end_pan(),
        }
    }

    fn pan_to(&mut self, x: f32, y: f32) {
        if let Gesture::Panning { last } = &mut self.gesture {
            self.pan.0 += x - last.0;
            self.pan.1 += y - last.1;
            *last = (x, y);
        }
    }

    pub(crate) fn end_pan(&mut self) {
        if matches!(self.gesture, Gesture::Panning { .. }) {
            self.gesture = Gesture::Idle;
        }
    }

    pub(super) fn begin(&mut self, x: f32, y: f32, shift: bool, tx: &mut Tx) {
        if self.is_busy() || self.doc.is_none() {
            self.gesture = Gesture::Ignored;
            return;
        }
        if self.space_down {
            self.gesture = Gesture::Panning { last: (x, y) };
            return;
        }
        if self.zoom_down {
            self.dispatch_ui(
                Action::ZoomAt {
                    factor: if shift { 0.5 } else { 2. },
                    anchor: (x, y),
                },
                tx,
            );
            self.gesture = Gesture::Ignored;
            return;
        }
        self.gesture = Gesture::Ignored;
        self.commit_text(tx);
        self.commit_number(tx);
        tx.blur();
        let Some(mut p) = self.coordinate(x, y, false) else {
            return;
        };
        let layout = self.layout();
        if self.tool == Tool::Crop {
            p = navigation::endpoint(Tool::Crop, p, p, false, layout);
        }
        let hit_tolerance = 5. / layout.scale;
        let selected = self.selected_indices();
        if !shift || self.tool != Tool::Select {
            if let [index] = selected[..] {
                let mark = &self.doc().marks[index];
                let handle = arrow::handle_at(mark, p, 7. / layout.scale);
                if handle.is_some() || mark.hit(p, hit_tolerance) {
                    self.start_annotation_drag(index, p, handle);
                    return;
                }
            } else if let Some(index) = self.doc().pick(p, hit_tolerance)
                && selected.contains(&index)
            {
                self.start_annotation_drag(index, p, None);
                return;
            }
        }
        if self.tool == Tool::Select {
            if let Some(index) = self.doc().pick(p, hit_tolerance) {
                let mut indices = if shift { selected } else { Vec::new() };
                if shift && indices.contains(&index) {
                    indices.retain(|i| *i != index);
                } else {
                    indices.push(index);
                }
                self.set_selection(indices);
                if !shift {
                    self.start_annotation_drag(index, p, None);
                }
            } else {
                self.gesture = Gesture::Selecting {
                    origin: p,
                    current: p,
                    additive: shift,
                    previous: selected,
                };
                if !shift {
                    self.set_selection(vec![]);
                }
            }
            return;
        }
        self.set_selection(vec![]);
        let settings = self.defaults[self.tool.index()];
        let mut mark = Mark {
            style: settings.style,
            tool: self.tool,
            curve: None,
            points: vec![p],
            color: settings.color,
            width: settings.width,
            text: if self.tool == Tool::Magnifier {
                settings.magnification.to_string()
            } else {
                String::new()
            },
            drawn: None,
        };
        match self.tool {
            Tool::Counter => {
                mark.text = self.counter_number().to_string();
                self.next_counter = self.next_counter.map(|n| n.saturating_add(1).min(999));
                self.edit(DocumentAction::AddAnnotation { mark }, tx);
            }
            Tool::Text => {
                self.text_edit = TextEdit::new("");
                self.text_edit.focused = true;
                self.text = Some(TextDraft { mark });
                self.status = "Type your label • Enter to finish • Escape to cancel".into();
            }
            _ => {
                if self.tool == Tool::Magnifier {
                    self.flatten_for_lens();
                }
                self.stroke_room = self.room_for_stroke();
                self.gesture = Gesture::Drawing(mark);
                self.refresh_lens();
            }
        }
    }

    pub(super) fn motion(&mut self, x: f32, y: f32, shift: bool) {
        if matches!(self.gesture, Gesture::Panning { .. }) {
            self.pan_to(x, y);
            return;
        }
        let Some(p) = self.coordinate(x, y, true) else {
            return;
        };
        if let Gesture::Selecting {
            origin,
            additive,
            previous,
            ..
        } = &self.gesture
        {
            let (origin, additive, previous) = (*origin, *additive, previous.clone());
            self.set_selection(if additive { previous } else { vec![] });
            self.select_region(selection_rectangle(origin, p), additive);
        }
        let layout = self.layout();
        let crop_ratio = self.crop_ratio;
        let mut stroke_full = false;
        let room = self.stroke_room;
        match &mut self.gesture {
            Gesture::Moving { drag, handle } => drag.update(p, shift, *handle),
            Gesture::MovingSelection(drags) => {
                for drag in drags {
                    drag.update(p, shift, None);
                }
            }
            Gesture::Selecting { current, .. } => *current = p,
            Gesture::Drawing(mark) if mark.tool == Tool::Pen => {
                if let Some(last) = mark.points.last()
                    && (p.0 - last.0).hypot(p.1 - last.1) * layout.scale < 0.35
                {
                    return;
                }
                stroke_full = mark.points.len() >= room;
                if !stroke_full {
                    mark.points.push(p);
                }
            }
            Gesture::Drawing(mark) => {
                mark.points.truncate(1);
                mark.points
                    .push(constrained(mark, p, shift, layout, crop_ratio));
            }
            _ => return,
        }
        if stroke_full {
            self.status = format!(
                "This stroke reached its limit of {room} points ({MAX_MARK_POINTS} a stroke, \
                 {MAX_DOCUMENT_POINTS} an image). Release to keep it."
            );
        }
        self.refresh_lens();
    }

    pub(super) fn finish(&mut self, x: f32, y: f32, shift: bool, tx: &mut Tx) {
        let gesture = std::mem::take(&mut self.gesture);
        let (mut drag, handle) = match gesture {
            Gesture::Moving { drag, handle } => (drag, handle),
            Gesture::MovingSelection(mut drags) => {
                if let Some(p) = self.coordinate(x, y, true) {
                    for drag in &mut drags {
                        drag.update(p, shift, None);
                    }
                }
                let moved = drags.iter().any(|d| d.moved != d.original);
                let updates = drags.into_iter().map(|d| (d.index, d.moved)).collect();
                if moved {
                    self.edit(
                        DocumentAction::EditAnnotations {
                            updates,
                            additions: vec![],
                            deletions: vec![],
                            remember: true,
                        },
                        tx,
                    );
                } else {
                    self.changed();
                }
                return;
            }
            Gesture::Selecting {
                origin,
                current,
                additive,
                previous,
            } => {
                let end = self.coordinate(x, y, true).unwrap_or(current);
                // A click on empty canvas clears the selection without selecting nearby bounds.
                let rectangle = selection_rectangle(origin, end);
                self.set_selection(if additive { previous } else { vec![] });
                let scale = self.layout().scale;
                if rectangle.2 * scale >= 3. || rectangle.3 * scale >= 3. {
                    self.select_region(rectangle, additive);
                }
                return;
            }
            Gesture::Drawing(mark) => {
                self.finish_drawing(mark, x, y, shift, tx);
                return;
            }
            Gesture::Idle | Gesture::Ignored | Gesture::Panning { .. } => return,
        };
        if let Some(p) = self.coordinate(x, y, true) {
            drag.update(p, shift, handle);
        }
        if drag.moved.points != drag.original.points || drag.moved.curve != drag.original.curve {
            self.edit(
                DocumentAction::UpdateAnnotation {
                    index: drag.index,
                    mark: drag.moved,
                },
                tx,
            );
        } else {
            // Puts the mark back in the raster after a selection click that did not move it.
            self.changed();
        }
    }

    fn finish_drawing(&mut self, mut mark: Mark, x: f32, y: f32, shift: bool, tx: &mut Tx) {
        if let Some(p) = self.coordinate(x, y, true) {
            let end = constrained(&mark, p, shift, self.layout(), self.crop_ratio);
            if mark.points.len() < self.stroke_room {
                mark.points.push(end);
            }
        }
        let first = mark.points[0];
        let last = *mark.points.last().unwrap();
        if mark.tool == Tool::Spotlight
            && ((first.0 - last.0).abs() < 2. || (first.1 - last.1).abs() < 2.)
        {
            return;
        }
        if mark.tool != Tool::Pen {
            mark.points = vec![first, last];
        }
        if mark.tool == Tool::Crop {
            let rectangle = selection_rectangle(first, last);
            if rectangle.2 >= 2. && rectangle.3 >= 2. {
                self.edit(DocumentAction::Crop { rectangle }, tx);
            }
            return;
        }
        self.edit(DocumentAction::AddAnnotation { mark }, tx);
    }

    /// How many points a new stroke may have.
    fn room_for_stroke(&self) -> usize {
        let used: usize = self.doc().marks.iter().map(|m| m.points.len()).sum();
        MAX_MARK_POINTS.min(MAX_DOCUMENT_POINTS.saturating_sub(used))
    }

    fn start_annotation_drag(&mut self, index: usize, origin: (f32, f32), handle: Option<usize>) {
        let selected = self.selected_indices();
        self.gesture = if handle.is_none() && selected.len() > 1 {
            let marks = &self.doc().marks;
            Gesture::MovingSelection(
                selected
                    .into_iter()
                    .map(|i| AnnotationDrag::new(&marks[i], i, origin))
                    .collect(),
            )
        } else {
            let drag = AnnotationDrag::new(&self.doc().marks[index], index, origin);
            Gesture::Moving { drag, handle }
        };
        if self.doc().marks[index].tool == Tool::Magnifier {
            self.flatten_for_lens();
        }
        // The raster stops below the moving marks, so they can be drawn live.
        self.changed();
        self.refresh_lens();
    }

    pub(crate) fn cancel_gesture(&mut self) {
        let gesture = std::mem::take(&mut self.gesture);
        self.reset_lens();
        if let Gesture::Selecting { previous, .. } = &gesture {
            self.set_selection(previous.clone());
        }
        if gesture.first_drag_index().is_some() {
            self.changed();
        }
    }

    pub(crate) fn selected_indices(&self) -> Vec<usize> {
        let count = self.doc.as_ref().map_or(0, |doc| doc.marks.len());
        let mut indices: Vec<_> = self
            .selected
            .iter()
            .copied()
            .filter(|i| *i < count)
            .collect();
        indices.sort_unstable();
        indices.dedup();
        indices
    }

    pub(crate) fn set_selection(&mut self, mut indices: Vec<usize>) {
        indices.sort_unstable();
        indices.dedup();
        self.selected = indices;
        if self.picker.open {
            let [r, g, b, _] = self.tool_settings().color;
            self.picker.sync([r, g, b]);
        }
    }

    /// Selects marks whose bounds intersect the rectangle, in image pixels.
    pub(crate) fn select_region(&mut self, rectangle: (f32, f32, f32, f32), additive: bool) {
        let mut indices = if additive {
            self.selected_indices()
        } else {
            vec![]
        };
        indices.extend(self.doc().marks.iter().enumerate().filter_map(|(i, mark)| {
            let (left, top, right, bottom) = mark.bounds();
            (left <= rectangle.0 + rectangle.2
                && right >= rectangle.0
                && top <= rectangle.1 + rectangle.3
                && bottom >= rectangle.1)
                .then_some(i)
        }));
        self.set_selection(indices);
    }
}

fn constrained(
    mark: &Mark,
    p: (f32, f32),
    shift: bool,
    layout: navigation::Layout,
    crop_ratio: Option<f32>,
) -> (f32, f32) {
    navigation::crop_endpoint(
        crop_ratio,
        navigation::endpoint(mark.tool, mark.points[0], p, shift, layout),
        mark.tool,
        mark.points[0],
        shift,
        layout,
    )
}
