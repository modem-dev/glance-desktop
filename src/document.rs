use ab_glyph::Font;
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_circle_mut, draw_text_mut};
use std::sync::Arc;
pub(crate) mod actions;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Select,
    Pen,
    Arrow,
    Rectangle,
    Highlight,
    Pixelate,
    Crop,
    Text,
    Counter,
    Spotlight,
    Magnifier,
}
impl Tool {
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Spotlight => "Spotlight",
            Self::Magnifier => "Magnifier",
            Self::Select => "Select",
            Self::Text => "Text",
            Self::Counter => "Step",
            Self::Pen => "Pen",
            Self::Arrow => "Arrow",
            Self::Rectangle => "Box",
            Self::Highlight => "Highlight",
            Self::Pixelate => "Pixelate",
            Self::Crop => "Crop",
        }
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    #[serde(default)]
    pub style: crate::style::Style,
    pub tool: Tool,
    /// Quadratic control point; None is a straight arrow.
    pub curve: Option<(f32, f32)>,
    pub points: Vec<(f32, f32)>,
    pub color: [u8; 4],
    pub width: f32,
    pub text: String,
}
#[derive(Clone)]
struct Snapshot {
    base: Arc<RgbaImage>,
    marks: Vec<Mark>,
    backdrop: Option<crate::backdrop::Backdrop>,
    image_animation: crate::animation::ImageAnimation,
}
#[derive(Clone)]
pub struct Document {
    pub base: Arc<RgbaImage>,
    pub marks: Vec<Mark>,
    pub backdrop: Option<crate::backdrop::Backdrop>,
    pub image_animation: crate::animation::ImageAnimation,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Document {
    pub fn new(base: RgbaImage) -> Self {
        Self {
            base: Arc::new(base),
            marks: vec![],
            backdrop: None,
            image_animation: Default::default(),
            undo: vec![],
            redo: vec![],
        }
    }
    pub fn render_snapshot(&self) -> Self {
        Self {
            base: self.base.clone(),
            marks: self.marks.clone(),
            backdrop: self.backdrop,
            image_animation: self.image_animation,
            undo: vec![],
            redo: vec![],
        }
    }
    /// Commit a completed private agent draft as one undoable change.
    pub(crate) fn apply_agent_draft(&mut self, draft: Self) {
        self.remember();
        self.base = draft.base;
        self.marks = draft.marks;
        self.backdrop = draft.backdrop;
        self.image_animation = draft.image_animation;
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            base: self.base.clone(),
            marks: self.marks.clone(),
            backdrop: self.backdrop,
            image_animation: self.image_animation,
        }
    }
    pub fn remember(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 30 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn commit(&mut self, mark: Mark) {
        if mark.points.is_empty() {
            return;
        }
        if mark.tool == Tool::Crop {
            let (x, y, w, h) = region(&mark, self.base.width(), self.base.height());
            if w < 2 || h < 2 {
                return;
            }
            self.remember();
            self.base = Arc::new(image::imageops::crop_imm(&*self.base, x, y, w, h).to_image());
            for mark in &mut self.marks {
                mark.translate(-(x as f32), -(y as f32));
            }
        } else {
            self.remember();
            self.marks.push(mark);
        }
    }
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.base = previous.base;
            self.marks = previous.marks;
            self.backdrop = previous.backdrop;
            self.image_animation = previous.image_animation;
        }
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.base = next.base;
            self.marks = next.marks;
            self.backdrop = next.backdrop;
            self.image_animation = next.image_animation;
        }
    }
    #[cfg(test)]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn plain(&self, draft: Option<&Mark>) -> RgbaImage {
        let mut out = (*self.base).clone();
        for mark in self
            .marks
            .iter()
            .chain(draft)
            .filter(|m| !matches!(m.tool, Tool::Spotlight | Tool::Magnifier))
        {
            paint(&mut out, mark);
        }
        out
    }
    pub fn render(&self, draft: Option<&Mark>) -> RgbaImage {
        let mut out = self.plain(draft);
        let effects: Vec<&Mark> = self.marks.iter().chain(draft).collect();
        let source = effects
            .iter()
            .any(|m| m.tool == Tool::Magnifier)
            .then(|| out.clone());
        crate::effects::spotlight_raster(&mut out, &effects);
        if let Some(source) = source {
            for mark in effects.iter().filter(|m| m.tool == Tool::Magnifier) {
                crate::effects::magnifier_raster(&mut out, &source, mark);
            }
        }
        out
    }
    pub fn export_at(&self, phase: f32) -> RgbaImage {
        if self.image_animation.enabled() {
            return crate::animation::Renderer::for_document(self, None).frame(phase);
        }
        if let Some(b) = self.backdrop
            && b.motion != crate::animation::Motion::Still
        {
            crate::animation::Renderer::new(&self.render(None), b, None).frame(phase)
        } else {
            self.export()
        }
    }
    pub fn animation_seconds(&self) -> u32 {
        if self.image_animation.enabled() {
            self.image_animation.seconds
        } else {
            self.backdrop.map_or(5, |b| b.seconds)
        }
    }
    pub fn animation_backdrop(&self) -> crate::backdrop::Backdrop {
        let mut b = self.backdrop.unwrap_or(crate::backdrop::Backdrop {
            preset: 7,
            padding: 0,
            shadow: 0,
            inner_radius: 0,
            inside_padding: 0,
            ..Default::default()
        });
        b.seconds = self.animation_seconds();
        b
    }
    pub fn export(&self) -> RgbaImage {
        let image = self.render(None);
        if let Some(b) = self.backdrop {
            b.apply(&image)
        } else {
            image
        }
    }
    pub fn resize(&mut self, scale: f32, smart: bool) -> Result<(), String> {
        let target = crate::enhance::dimensions(self.base.dimensions(), scale)?;
        if target == self.base.dimensions() {
            return Ok(());
        }
        let sx = target.0 as f32 / self.base.width() as f32;
        let sy = target.1 as f32 / self.base.height() as f32;
        let mut marks = self.marks.clone();
        for mark in &mut marks {
            for p in &mut mark.points {
                p.0 *= sx;
                p.1 *= sy;
            }
            if let Some(c) = &mut mark.curve {
                c.0 *= sx;
                c.1 *= sy;
            }
            mark.width *= (sx + sy) * 0.5;
            mark.style.radius *= (sx + sy) * 0.5;
            actions::validate_mark(mark)?;
        }
        let resized = crate::enhance::resize(&self.base, target, smart);
        self.remember();
        self.base = Arc::new(resized);
        self.marks = marks;
        Ok(())
    }
    pub fn rotate(&mut self) {
        // Flatten labels so their visual orientation rotates along with the image.
        let rotated = image::imageops::rotate90(&self.render(None));
        self.remember();
        self.base = Arc::new(rotated);
        self.marks.clear();
    }
}
fn region(mark: &Mark, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let a = mark.points[0];
    let b = *mark.points.last().unwrap();
    let x = a.0.min(b.0).floor().clamp(0., width as f32) as u32;
    let y = a.1.min(b.1).floor().clamp(0., height as f32) as u32;
    let right = a.0.max(b.0).ceil().clamp(0., width as f32) as u32;
    let bottom = a.1.max(b.1).ceil().clamp(0., height as f32) as u32;
    (x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}
fn thick_line(image: &mut RgbaImage, a: (f32, f32), b: (f32, f32), width: f32, color: Rgba<u8>) {
    let radius = (width / 2.).round().max(1.) as i32;
    let steps = ((b.0 - a.0).hypot(b.1 - a.1)).ceil().max(1.) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        draw_filled_circle_mut(
            image,
            (
                (a.0 + (b.0 - a.0) * t).round() as i32,
                (a.1 + (b.1 - a.1) * t).round() as i32,
            ),
            radius,
            color,
        );
    }
}
pub(crate) fn paint(out: &mut RgbaImage, mark: &Mark) {
    if mark.points.is_empty() {
        return;
    }
    let color = Rgba(mark.color);
    let a = mark.points[0];
    let b = *mark.points.last().unwrap();
    match mark.tool {
        Tool::Select | Tool::Spotlight | Tool::Magnifier => {}
        Tool::Counter => {
            let radius = (mark.width * 3.6).max(1.);
            draw_filled_circle_mut(
                out,
                (a.0.round() as i32, a.1.round() as i32),
                radius.round() as i32,
                color,
            );
            let font_size = (mark.width * 4.4).max(1.);
            let mut label = mark.clone();
            label.tool = Tool::Text;
            label.color = if mark.color[..3].iter().map(|c| *c as u32).sum::<u32>() > 600 {
                [32, 34, 42, 255]
            } else {
                [255, 255, 255, 255]
            };
            label.width = font_size / 7.;
            label.points = vec![(
                a.0 - font_size * 0.28 * mark.text.len() as f32,
                a.1 - font_size * 0.5,
            )];
            paint(out, &label);
        }
        Tool::Text => {
            let font = crate::platform::annotation_font();
            if let Some(font) = font {
                // GPUI uses em pixels; ab_glyph scales by ascent + descent.
                let em_size = (mark.width * 7.).max(1.);
                let scale = em_size * font.height_unscaled()
                    / font.units_per_em().unwrap_or(font.height_unscaled());
                use image::{GrayImage, Luma};
                let bounds = mark.bounds();
                // Advance bounds are for selection; glyph ink (such as j's
                // negative bearing or accented capitals) extends beyond them.
                // Reserve an em around the label, clipped to the image.
                let margin = em_size.ceil();
                let x = (bounds.0.floor() - margin).max(0.) as u32;
                let y = (bounds.1.floor() - margin).max(0.) as u32;
                let right = (bounds.2.ceil() + margin).max(0.).min(out.width() as f32) as u32;
                let bottom = (bounds.3.ceil() + margin).max(0.).min(out.height() as f32) as u32;
                if x >= right || y >= bottom {
                    return;
                }
                let mut mask = GrayImage::new(right - x, bottom - y);
                draw_text_mut(
                    &mut mask,
                    Luma([255]),
                    (a.0 - x as f32) as i32,
                    (a.1 + (em_size - scale) / 2. - y as f32).round() as i32,
                    scale,
                    font,
                    &mark.text,
                );
                for (dx, dy, p) in mask.enumerate_pixels() {
                    if p[0] == 0 {
                        continue;
                    }
                    let mut tint = color;
                    tint[3] = ((tint[3] as u16 * p[0] as u16 + 127) / 255) as u8;
                    crate::style::blend(out.get_pixel_mut(x + dx, y + dy), tint);
                }
            }
        }
        Tool::Pen | Tool::Rectangle => paint_shape(out, mark),
        Tool::Arrow => crate::arrow::raster(out, mark),
        Tool::Crop => {
            for (start, end) in [
                (a, (b.0, a.1)),
                ((b.0, a.1), b),
                (b, (a.0, b.1)),
                ((a.0, b.1), a),
            ] {
                thick_line(out, start, end, 2., Rgba([255; 4]));
            }
        }
        Tool::Highlight => {
            let (x, y, w, h) = region(mark, out.width(), out.height());
            for yy in y..y + h {
                for xx in x..x + w {
                    let p = out.get_pixel_mut(xx, yy);
                    let mut tint = color;
                    tint[3] = (mark.color[3] as f32 * 0.35).round() as u8;
                    crate::style::blend(p, tint);
                }
            }
        }
        Tool::Pixelate => {
            let (x, y, w, h) = region(mark, out.width(), out.height());
            let block = (mark.width * 4.).max(12.) as u32;
            for yy in (y..y + h).step_by(block as usize) {
                for xx in (x..x + w).step_by(block as usize) {
                    let bw = block.min(x + w - xx);
                    let bh = block.min(y + h - yy);
                    let mut sum = [0u64; 4];
                    for py in yy..yy + bh {
                        for px in xx..xx + bw {
                            for (i, c) in out.get_pixel(px, py).0.iter().enumerate() {
                                sum[i] += *c as u64;
                            }
                        }
                    }
                    let p = Rgba(sum.map(|s| (s / (bw * bh) as u64) as u8));
                    for py in yy..yy + bh {
                        for px in xx..xx + bw {
                            out.put_pixel(px, py, p);
                        }
                    }
                }
            }
        }
    }
}
/// A single coverage mask prevents translucent stroke joints from darkening.
fn paint_shape(out: &mut RgbaImage, m: &Mark) {
    use image::{GrayImage, Luma, imageops};
    use imageproc::{drawing::draw_polygon_mut, point::Point};
    let b = m.bounds();
    let x = b.0.floor().max(0.) as u32;
    let y = b.1.floor().max(0.) as u32;
    let right = (b.2.ceil() + 2.).max(0.).min(out.width() as f32) as u32;
    let bottom = (b.3.ceil() + 2.).max(0.).min(out.height() as f32) as u32;
    if x >= right || y >= bottom {
        return;
    }
    let mut mask = GrayImage::new((right - x) * 2, (bottom - y) * 2);
    let map = |p: (f32, f32)| ((p.0 - x as f32) * 2., (p.1 - y as f32) * 2.);
    let points = if m.tool == Tool::Pen {
        crate::style::pen_points(m)
    } else {
        crate::style::box_points(m)
    };
    if m.tool == Tool::Rectangle && m.style.fill == crate::style::Fill::Filled {
        let mut polygon: Vec<_> = points
            .iter()
            .take(points.len() - 1)
            .map(|&p| {
                let p = map(p);
                Point::new(p.0.round() as i32, p.1.round() as i32)
            })
            .collect();
        polygon.dedup();
        if polygon.len() >= 3 && polygon.first() != polygon.last() {
            draw_polygon_mut(&mut mask, &polygon, Luma([255]));
        }
    } else {
        let radius = m.width.round().max(1.) as i32;
        let dash = if m.tool == Tool::Pen {
            crate::style::Dash::Solid
        } else {
            m.style.dash
        };
        for path in crate::style::strokes(&points, dash, m.width) {
            let pairs: Vec<_> = if path.len() == 1 {
                vec![(path[0], path[0])]
            } else {
                path.windows(2).map(|p| (p[0], p[1])).collect()
            };
            for (a, b) in pairs {
                let (a, b) = (map(a), map(b));
                let steps = (b.0 - a.0).hypot(b.1 - a.1).ceil().max(1.) as usize;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    draw_filled_circle_mut(
                        &mut mask,
                        (
                            (a.0 + (b.0 - a.0) * t).round() as i32,
                            (a.1 + (b.1 - a.1) * t).round() as i32,
                        ),
                        radius,
                        Luma([255]),
                    );
                }
            }
        }
    }
    let mask = imageops::resize(&mask, right - x, bottom - y, imageops::FilterType::Triangle);
    for (dx, dy, p) in mask.enumerate_pixels() {
        if p[0] > 0 {
            let mut color = m.color;
            color[3] = ((color[3] as u16 * p[0] as u16 + 127) / 255) as u8;
            crate::style::blend(out.get_pixel_mut(x + dx, y + dy), Rgba(color));
        }
    }
}
pub fn demo() -> RgbaImage {
    let mut image = RgbaImage::from_pixel(1200, 760, Rgba([22, 27, 36, 255]));
    let label = |image: &mut RgbaImage, x: f32, y: f32, text: &str, size: f32, color: [u8; 4]| {
        paint(
            image,
            &Mark {
                style: Default::default(),
                tool: Tool::Text,
                curve: None,
                points: vec![(x, y)],
                color,
                width: size / 7.,
                text: text.into(),
            },
        );
    };
    label(
        &mut image,
        72.,
        48.,
        "A little clarity goes a long way.",
        36.,
        [240, 243, 250, 255],
    );
    label(
        &mut image,
        74.,
        99.,
        "YOUR SCREENSHOT IS THE STARTING POINT",
        16.,
        [123, 141, 162, 255],
    );
    let rows = [
        (
            "Capture the context",
            "A whole screen, or just the part that matters.",
        ),
        ("Make your point", "Circle the detail. Draw the connection."),
        (
            "Guide the conversation",
            "Use an arrow to show exactly where to look.",
        ),
        (
            "Keep it focused",
            "Crop out the noise. Highlight the useful bit.",
        ),
        (
            "Add a little explanation",
            "A short text label can say a lot.",
        ),
        (
            "Ready to share",
            "Copy your image, or save a full-resolution PNG.",
        ),
    ];
    for (row, (title, description)) in rows.iter().enumerate() {
        let y = 174 + row as u32 * 80;
        for yy in y..y + 64 {
            for xx in 64..1136 {
                image.put_pixel(xx, yy, Rgba([29, 36, 48, 255]));
            }
        }
        for yy in y + 12..y + 50 {
            for xx in 84..122 {
                image.put_pixel(xx, yy, Rgba([49, 65, 84, 255]));
            }
        }
        label(
            &mut image,
            95.,
            y as f32 + 14.,
            &(row + 1).to_string(),
            24.,
            [154, 181, 213, 255],
        );
        label(
            &mut image,
            146.,
            y as f32 + 9.,
            title,
            24.,
            [219, 229, 243, 255],
        );
        label(
            &mut image,
            147.,
            y as f32 + 38.,
            description,
            17.,
            [134, 152, 176, 255],
        );
    }
    label(
        &mut image,
        74.,
        697.,
        "Try the pen on this canvas.  Capture something real when you are ready.",
        18.,
        [113, 134, 158, 255],
    );
    image
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_exports_preserve_glyph_overhangs() {
        use ab_glyph::Font;
        let font = crate::platform::annotation_font().expect("annotation font is available");
        let em = 35.;
        let scale =
            em * font.height_unscaled() / font.units_per_em().unwrap_or(font.height_unscaled());
        for text in ["j", "Á", "f", "jÁfy"] {
            let mark = Mark {
                style: Default::default(),
                tool: Tool::Text,
                points: vec![(20., 20.)],
                curve: None,
                color: [255; 4],
                width: em / 7.,
                text: text.into(),
            };
            let mut actual = RgbaImage::new(160, 100);
            paint(&mut actual, &mark);
            let mut expected = image::GrayImage::new(160, 100);
            draw_text_mut(
                &mut expected,
                image::Luma([255]),
                20,
                (20. + (em - scale) / 2.).round() as i32,
                scale,
                font,
                text,
            );
            for ((x, y, actual), expected) in actual.enumerate_pixels().zip(expected.pixels()) {
                assert_eq!(actual[3], expected[0], "{text}: clipped glyph at {x},{y}");
            }
        }
    }
    fn mark(tool: Tool, a: (f32, f32), b: (f32, f32)) -> Mark {
        Mark {
            style: Default::default(),
            tool,
            curve: None,
            points: vec![a, b],
            color: [255, 0, 0, 255],
            width: 2.,
            text: String::new(),
        }
    }
    #[test]
    fn annotation_history_shares_pixels() {
        let mut doc = Document::new(RgbaImage::from_pixel(3840, 2160, Rgba([0, 0, 0, 255])));
        let original = doc.base.clone();
        for i in 0..30 {
            doc.commit(mark(Tool::Pen, (i as f32, 5.), (i as f32, 10.)));
        }
        assert!(Arc::ptr_eq(&original, &doc.base));
        assert!(
            doc.undo
                .iter()
                .all(|state| Arc::ptr_eq(&original, &state.base))
        );
        let export = doc.render_snapshot();
        assert!(Arc::ptr_eq(&original, &export.base));
        assert!(export.undo.is_empty());
    }
    #[test]
    fn history_restores_crop_and_annotations() {
        let mut d = Document::new(RgbaImage::from_pixel(20, 20, Rgba([0, 0, 0, 255])));
        d.commit(mark(Tool::Rectangle, (2., 2.), (15., 15.)));
        d.commit(mark(Tool::Crop, (18., 18.), (1., 1.)));
        assert_eq!(d.base.dimensions(), (17, 17));
        assert_eq!(d.marks[0].points[0], (1., 1.));
        d.undo();
        assert_eq!(d.base.dimensions(), (20, 20));
        assert_eq!(d.marks.len(), 1);
        d.redo();
        assert_eq!(d.base.dimensions(), (17, 17));
        d.undo();
        d.commit(mark(Tool::Pen, (5., 5.), (7., 7.)));
        assert!(!d.can_redo());
    }
    #[test]
    fn pixelation_clamps_reverse_drag_and_preserves_outside() {
        let mut base = RgbaImage::new(20, 20);
        for (x, y, p) in base.enumerate_pixels_mut() {
            *p = Rgba([x as u8 * 10, y as u8 * 10, 0, 255]);
        }
        let mut d = Document::new(base.clone());
        d.commit(mark(Tool::Pixelate, (30., 30.), (5., 5.)));
        let out = d.render(None);
        assert_eq!(out.get_pixel(0, 0), base.get_pixel(0, 0));
        assert_eq!(out.get_pixel(5, 5), out.get_pixel(10, 10));
        assert_ne!(out.get_pixel(5, 5), base.get_pixel(5, 5));
    }
    #[test]
    fn export_has_committed_marks_and_preview_is_non_destructive() {
        let mut d = Document::new(RgbaImage::from_pixel(30, 30, Rgba([0, 0, 0, 255])));
        let m = mark(Tool::Pen, (5., 5.), (20., 20.));
        assert_eq!(
            d.render(Some(&m)).get_pixel(10, 10),
            &Rgba([255, 0, 0, 255])
        );
        assert_eq!(d.render(None).get_pixel(10, 10), &Rgba([0, 0, 0, 255]));
        d.commit(m);
        assert_eq!(d.render(None).get_pixel(10, 10), &Rgba([255, 0, 0, 255]));
    }
    #[test]
    fn backdrop_is_undoable_and_crop_keeps_framing_separate() {
        let mut d = Document::new(RgbaImage::from_pixel(30, 30, Rgba([0, 0, 0, 255])));
        let original = d.base.clone();
        d.remember();
        d.backdrop = Some(crate::backdrop::Backdrop {
            padding: 10,
            ..Default::default()
        });
        assert!(Arc::ptr_eq(&d.base, &original));
        assert_eq!(d.export().dimensions(), (50, 50));
        assert_eq!(d.render(None).dimensions(), (30, 30));
        d.undo();
        assert!(d.backdrop.is_none());
        d.redo();
        d.commit(mark(Tool::Crop, (5., 5.), (25., 25.)));
        assert_eq!(d.base.dimensions(), (20, 20));
        assert_eq!(d.export().dimensions(), (40, 40));
        d.undo();
        assert_eq!(d.export().dimensions(), (50, 50));
    }
    #[test]
    fn resize_preserves_vector_marks_and_restores_original_on_undo() {
        let mut d = Document::new(RgbaImage::from_pixel(40, 20, Rgba([70, 90, 110, 255])));
        let original = d.base.clone();
        let mut label = mark(Tool::Text, (10., 10.), (10., 10.));
        label.width = 20. / 7.;
        label.text = "Sharp".into();
        d.commit(label);
        d.resize(2., true).unwrap();
        assert_eq!(d.base.dimensions(), (80, 40));
        assert_eq!(d.marks[0].points[0], (20., 20.));
        assert!((d.marks[0].width * 7. - 40.).abs() < 0.01);
        d.undo();
        assert!(Arc::ptr_eq(&d.base, &original));
        assert_eq!(d.marks[0].points[0], (10., 10.));
        d.redo();
        assert_eq!(d.base.dimensions(), (80, 40));
        let before = d.base.clone();
        assert!(d.resize(1000., true).is_err());
        assert!(Arc::ptr_eq(&d.base, &before));
    }
    #[test]
    fn rotation_restores_annotations_and_pixel_positions() {
        let mut src = RgbaImage::from_pixel(4, 3, Rgba([0, 0, 0, 255]));
        src.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
        let mut d = Document::new(src);
        d.rotate();
        assert_eq!(d.base.dimensions(), (3, 4));
        assert_eq!(d.base.get_pixel(2, 0), &Rgba([255, 255, 255, 255]));
        d.undo();
        assert_eq!(d.base.dimensions(), (4, 3));
    }
}
