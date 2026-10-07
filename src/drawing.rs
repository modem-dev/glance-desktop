//! Live GPU overlays. No screenshot allocation or texture upload in pointer handlers.
use crate::{
    Layout,
    document::{Mark, Tool},
};
use gpui::*;
use lyon::tessellation::{LineCap, LineJoin};

pub fn paths(mark: &Mark, layout: Layout) -> Vec<Path<Pixels>> {
    if mark.points.is_empty() {
        return vec![];
    }
    let map = |(x, y): (f32, f32)| {
        point(
            px(layout.x + x * layout.scale),
            px(layout.y + y * layout.scale),
        )
    };
    let width = mark.width.max(0.5) * layout.scale;
    let make = || {
        let mut path = PathBuilder::stroke(px(width));
        path.style = PathStyle::Stroke(
            StrokeOptions::default()
                .with_line_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
        path
    };
    let mut result = vec![];
    let points = match mark.tool {
        Tool::Pen => crate::style::pen_points(mark),
        Tool::Arrow => crate::style::shaft(mark),
        Tool::Rectangle => crate::style::box_points(mark),
        _ => return result,
    };
    if mark.tool == Tool::Rectangle && mark.style.fill == crate::style::Fill::Filled {
        let mut path = PathBuilder::fill();
        path.move_to(map(points[0]));
        for &p in &points[1..] {
            path.line_to(map(p));
        }
        path.close();
        if let Ok(p) = path.build() {
            result.push(p);
        }
    } else {
        let dash = if mark.tool == Tool::Pen {
            crate::style::Dash::Solid
        } else {
            mark.style.dash
        };
        for points in crate::style::strokes(&points, dash, mark.width) {
            let mut start = 0;
            while start < points.len() {
                let end = (start + 1024).min(points.len());
                let mut path = make();
                path.move_to(map(points[start]));
                if end - start == 1 {
                    path.line_to(map((points[start].0 + 0.01, points[start].1)));
                }
                for &p in &points[start + 1..end] {
                    path.line_to(map(p));
                }
                if let Ok(p) = path.build() {
                    result.push(p);
                }
                if end == points.len() {
                    break;
                }
                start = end - 1;
            }
        }
    }
    if mark.tool == Tool::Arrow {
        for head in crate::style::heads(mark) {
            let mut path = PathBuilder::fill();
            path.move_to(map(head[0]));
            path.line_to(map(head[1]));
            path.line_to(map(head[2]));
            path.close();
            if let Ok(p) = path.build() {
                result.push(p);
            }
        }
        for p in crate::style::dots(mark) {
            let radius = (mark.width * 2.).max(3.);
            let mut path = PathBuilder::fill();
            for i in 0..32 {
                let angle = i as f32 * std::f32::consts::TAU / 32.;
                let q = map((p.0 + radius * angle.cos(), p.1 + radius * angle.sin()));
                if i == 0 {
                    path.move_to(q);
                } else {
                    path.line_to(q);
                }
            }
            path.close();
            if let Ok(p) = path.build() {
                result.push(p);
            }
        }
    }
    result
}
pub fn paint(mark: &Mark, layout: Layout, window: &mut Window, cx: &mut App) {
    if mark.points.is_empty() {
        return;
    }
    let color = rgba(u32::from_be_bytes(mark.color));
    let a = mark.points[0];
    let b = *mark.points.last().unwrap();
    let bounds = Bounds::new(
        point(
            px(layout.x + a.0.min(b.0) * layout.scale),
            px(layout.y + a.1.min(b.1) * layout.scale),
        ),
        size(
            px((a.0 - b.0).abs() * layout.scale),
            px((a.1 - b.1).abs() * layout.scale),
        ),
    );
    match mark.tool {
        Tool::Select | Tool::Magnifier => {}
        Tool::Spotlight => crate::effects::paint_spotlight(mark, layout, window),
        Tool::Pen | Tool::Arrow | Tool::Rectangle => {
            for path in paths(mark, layout) {
                window.paint_path(path, color);
            }
        }
        Tool::Highlight => {
            window.paint_quad(fill(
                bounds,
                Rgba {
                    a: 0.35 * color.a,
                    ..color
                },
            ));
        }
        Tool::Pixelate | Tool::Crop => {
            let tint = if mark.tool == Tool::Crop {
                rgb(0xffffff)
            } else {
                rgb(0x74839a)
            };
            window.paint_quad(quad(
                bounds,
                px(0.),
                Rgba { a: 0.12, ..tint },
                px(1.),
                tint,
                Default::default(),
            ));
            // Selection handles remain a constant screen size at every zoom level.
            for corner in [
                bounds.origin,
                bounds.bottom_right(),
                point(bounds.origin.x, bounds.bottom_right().y),
                point(bounds.bottom_right().x, bounds.origin.y),
            ] {
                window.paint_quad(quad(
                    Bounds::new(corner - point(px(3.), px(3.)), size(px(6.), px(6.))),
                    px(1.),
                    rgb(0xffffff),
                    px(1.),
                    rgb(0x64748b),
                    Default::default(),
                ));
            }
        }
        Tool::Counter => {
            let radius = (mark.width * 3.6).max(1.) * layout.scale;
            let center = point(
                px(layout.x + a.0 * layout.scale),
                px(layout.y + a.1 * layout.scale),
            );
            window.paint_quad(quad(
                Bounds::new(
                    center - point(px(radius), px(radius)),
                    size(px(radius * 2.), px(radius * 2.)),
                ),
                px(radius),
                color,
                px(0.),
                color,
                Default::default(),
            ));
            let font_size = px((mark.width * 4.4).max(1.) * layout.scale);
            let run = TextRun {
                len: mark.text.len(),
                font: font(crate::platform::annotation_font_family()),
                color: rgb(
                    if mark.color[..3].iter().map(|c| *c as u32).sum::<u32>() > 600 {
                        0x20222a
                    } else {
                        0xffffff
                    },
                )
                .into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(mark.text.clone().into(), font_size, &[run], None);
            let _ = line.paint(
                center - point(line.width * 0.5, font_size * 0.5),
                font_size,
                window,
                cx,
            );
        }
        Tool::Text => {
            let font_size = px((mark.width * 7.).max(1.) * layout.scale);
            let run = TextRun {
                len: mark.text.len(),
                font: font(crate::platform::annotation_font_family()),
                color: color.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                mark.text.replace('\n', " ").into(),
                font_size,
                &[run],
                None,
            );
            let _ = line.paint(
                point(
                    px(layout.x + a.0 * layout.scale),
                    px(layout.y + a.1 * layout.scale),
                ),
                font_size,
                window,
                cx,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{Layout, Mark, Tool, paths};
    #[test]
    fn long_strokes_are_chunked_without_vertex_overflow() {
        let mark = Mark {
            style: Default::default(),
            tool: Tool::Pen,
            curve: None,
            points: (0..10000)
                .map(|i| (i as f32 * 0.2, (i as f32 * 0.02).sin() * 20.))
                .collect(),
            color: [255, 0, 0, 255],
            width: 5.,
            text: String::new(),
        };
        let paths = paths(
            &mark,
            Layout {
                scale: 1.,
                ..Default::default()
            },
        );
        assert!(paths.len() > 1);
        assert_eq!(paths.len(), 10);
    }
}
