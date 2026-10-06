use super::actions::Action;
use super::state::Layout;
use super::{Editor, text_input};
use crate::{
    animation, arrow,
    document::{Mark, Tool},
    drawing, effects,
};
use gpui::{prelude::*, *};
impl Editor {
    pub(super) fn canvas(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let self_revision = self.preview.revision;
        let image = self.preview.image.clone();
        let preview_padding = self.preview.inside_padding;
        let text_entity = cx.entity();
        let overlays: Vec<Mark> = self
            .document
            .marks
            .iter()
            .enumerate()
            .skip(self.preview.mark_count)
            .map(|(index, mark)| {
                self.interaction
                    .gesture
                    .moved_mark(index)
                    .unwrap_or(mark)
                    .clone()
            })
            .chain(self.interaction.gesture.draft().cloned())
            .collect();
        self.prepare_lens(&overlays);
        let live_lens = self.preview.lens.clone();
        let selected_indices = self.selected_indices();
        let selected_marks: Vec<Mark> = selected_indices
            .iter()
            .filter_map(|index| {
                self.interaction
                    .gesture
                    .moved_mark(*index)
                    .or_else(|| self.document.marks.get(*index))
                    .cloned()
            })
            .collect();
        let selected_mark = (selected_marks.len() == 1).then(|| selected_marks[0].clone());
        let selection_bounds: Vec<_> = selected_marks
            .iter()
            .filter(|mark| {
                selected_marks.len() > 1
                    || !matches!(mark.tool, Tool::Arrow | Tool::Magnifier | Tool::Spotlight)
            })
            .map(Mark::bounds)
            .collect();
        let marquee = if let super::state::Gesture::Selecting {
            origin, current, ..
        } = self.interaction.gesture
        {
            Some((
                origin.0.min(current.0),
                origin.1.min(current.1),
                origin.0.max(current.0),
                origin.1.max(current.1),
            ))
        } else {
            None
        };
        let canvas_bounds = self.viewport.canvas_bounds.clone();
        let layout = self.viewport.layout.clone();
        let dimensions = self.document.base.dimensions();
        let backdrop = self.document.backdrop;
        let composition_preview = self.playback.composition_preview.clone();
        let composition = self.panels.animation && self.document.image_animation.enabled();
        let composition_source = composition.then(|| {
            composition_preview
                .borrow_mut()
                .source(&self.document, self_revision)
        });
        self.sync_preview_preparation(self.preview_preparing());
        let clip_time = self.clip_time();
        let seek = self.playback.seek;
        if !composition {
            composition_preview.borrow_mut().clear(window);
        }
        let motion_preview = self.playback.motion_preview.clone();
        if composition {
            motion_preview.borrow_mut().suspend();
        } else if !backdrop.is_some_and(|b| b.motion.uses_shader()) {
            motion_preview.borrow_mut().clear(window);
        }
        let animation_phase = self.animation_phase();
        let animation_playing =
            !self.playback.paused && !self.is_busy() && window.is_window_active();
        if animation_playing
            && (if composition {
                clip_time < self.document.animation_seconds() as f32
            } else {
                backdrop.is_some_and(|b| b.motion != animation::Motion::Still)
            })
        {
            window.request_animation_frame();
        }
        let output_dimensions = backdrop.map_or(dimensions, |b| b.dimensions(dimensions));
        let zoom = self.viewport.zoom;
        let pan = self.viewport.pan;
        div()
            .relative()
            .on_scroll_wheel(cx.listener(|this, e, _, cx| this.scroll(e, cx)))
            .on_drop(cx.listener(|this, files: &ExternalPaths, _, cx| {
                if let Some(path) = files.paths().first() {
                    this.dispatch_ui(Action::OpenPath { path: path.clone() }, cx);
                }
            }))
            .flex()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .cursor(
                if self.panels.sampling_color.is_some() || self.panels.sampling_tool_color {
                    CursorStyle::Crosshair
                } else if self.viewport.space_down {
                    CursorStyle::OpenHand
                } else if self.interaction.tool == Tool::Select {
                    CursorStyle::Arrow
                } else {
                    CursorStyle::Crosshair
                },
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::begin))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, e: &MouseDownEvent, _, cx| this.begin_pan(e.position, cx)),
            )
            .child(
                self.accessibility.element(
                    canvas(
                        move |bounds, _, _| bounds,
                        move |bounds, _, window, cx| {
                            canvas_bounds.set(bounds);
                            window.paint_quad(fill(
                                bounds,
                                rgb(crate::theme::Theme::get(cx).workspace),
                            ));
                            let fit = ((f32::from(bounds.size.width) - 80.)
                                / output_dimensions.0 as f32)
                                .min(
                                    (f32::from(bounds.size.height) - 70.)
                                        / output_dimensions.1 as f32,
                                )
                                .clamp(0.01, 1.);
                            let scale = zoom.unwrap_or(fit);
                            let w = dimensions.0 as f32 * scale;
                            let h = dimensions.1 as f32 * scale;

                            let x = f32::from(bounds.origin.x)
                                + (f32::from(bounds.size.width) - w) / 2.
                                + pan.0;
                            let y = f32::from(bounds.origin.y)
                                + (f32::from(bounds.size.height) - h) / 2.
                                + pan.1;
                            layout.set(Layout {
                                x,
                                y,
                                scale,
                                width: dimensions.0 as f32,
                                height: dimensions.1 as f32,
                            });
                            let image_bounds = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
                            let inside = backdrop.map_or(0., |b| b.inside_padding as f32 * scale);
                            let padded_bounds = image_bounds.dilate(px(inside));
                            let preview_bounds =
                                image_bounds.dilate(px(preview_padding as f32 * scale));
                            let framing = backdrop.map(|b| b.layout(dimensions));
                            let frame_bounds = framing.map_or(image_bounds, |frame| {
                                Bounds::new(
                                    point(
                                        px(x - frame.origin.0 as f32 * scale),
                                        px(y - frame.origin.1 as f32 * scale),
                                    ),
                                    size(
                                        px(frame.dimensions.0 as f32 * scale),
                                        px(frame.dimensions.1 as f32 * scale),
                                    ),
                                )
                            });
                            if let Some(document) = &composition_source {
                                window.paint_quad(fill(
                                    frame_bounds,
                                    document.animation_backdrop().background(),
                                ));
                                motion_preview.borrow().paint_cached(
                                    document.animation_backdrop(),
                                    frame_bounds,
                                    window,
                                );
                                composition_preview.borrow_mut().paint(
                                    clip_time,
                                    seek,
                                    frame_bounds,
                                    animation_playing
                                        && clip_time < document.animation_seconds() as f32,
                                    window,
                                );
                                let preparing =
                                    !composition_preview.borrow().ready_for(self_revision, seek);
                                if preparing {
                                    paint_preparing(frame_bounds.intersect(&bounds), window, cx);
                                }
                                text_entity.update(cx, |editor, cx| {
                                    if editor.playback.preparing != preparing {
                                        editor.sync_preview_preparation(preparing);
                                        cx.notify();
                                    }
                                });
                                return;
                            }
                            if let Some(b) = backdrop {
                                window.paint_quad(quad(
                                    frame_bounds,
                                    px(0.),
                                    b.background(),
                                    px(0.),
                                    rgb(0xffffff),
                                    Default::default(),
                                ));
                                if b.motion != animation::Motion::Still {
                                    window.with_content_mask(
                                        Some(ContentMask {
                                            bounds: frame_bounds.intersect(&bounds),
                                        }),
                                        |window| {
                                            animation::paint(
                                                b,
                                                animation_phase,
                                                frame_bounds,
                                                px(0.),
                                                &mut motion_preview.borrow_mut(),
                                                animation_playing,
                                                window,
                                            )
                                        },
                                    );
                                }
                                if b.shadow > 0 {
                                    window.with_content_mask(
                                        Some(ContentMask {
                                            bounds: frame_bounds.intersect(&bounds),
                                        }),
                                        |window| {
                                            window.paint_shadows(
                                                padded_bounds,
                                                px(b.inner_radius as f32 * scale).into(),
                                                &[BoxShadow {
                                                    color: rgba(0x00000038).into(),
                                                    offset: point(
                                                        px(0.),
                                                        px(b.shadow as f32 * scale * 0.25),
                                                    ),
                                                    blur_radius: px(b.shadow as f32 * scale),
                                                    spread_radius: px(0.),
                                                }],
                                            );
                                        },
                                    );
                                }
                            } else {
                                window.paint_shadows(
                                    image_bounds,
                                    Default::default(),
                                    &[
                                        BoxShadow {
                                            color: rgba(0x17203320).into(),
                                            offset: point(px(0.), px(12.)),
                                            blur_radius: px(32.),
                                            spread_radius: px(0.),
                                        },
                                        BoxShadow {
                                            color: rgba(0x17203310).into(),
                                            offset: point(px(0.), px(2.)),
                                            blur_radius: px(6.),
                                            spread_radius: px(0.),
                                        },
                                    ],
                                );
                            }
                            if backdrop.is_none() {
                                window.paint_quad(quad(
                                    image_bounds,
                                    px(backdrop.map_or(0., |b| b.inner_radius as f32 * scale)),
                                    rgb(0xffffff),
                                    px(1.),
                                    rgb(0xd8d8e1),
                                    Default::default(),
                                ));
                            }
                            let _ = window.paint_image(
                                preview_bounds,
                                px(backdrop.map_or(0., |b| b.inner_radius as f32 * scale)).into(),
                                image,
                                0,
                                false,
                            );
                            window.with_content_mask(
                                Some(ContentMask {
                                    bounds: image_bounds.intersect(&bounds),
                                }),
                                |window| {
                                    for mark in &overlays {
                                        drawing::paint(mark, layout.get(), window, cx);
                                        if mark.tool == Tool::Magnifier {
                                            effects::paint_lens(
                                                mark,
                                                layout.get(),
                                                live_lens
                                                    .as_ref()
                                                    .filter(|(key, _)| {
                                                        *key == effects::LensKey::new(
                                                            self_revision,
                                                            mark,
                                                        )
                                                    })
                                                    .map(|(_, image)| image.clone()),
                                                window,
                                            );
                                        }
                                    }
                                    if let Some(mark) = &selected_mark
                                        && matches!(
                                            mark.tool,
                                            Tool::Arrow | Tool::Magnifier | Tool::Spotlight
                                        )
                                    {
                                        arrow::paint_handles(mark, layout.get(), window);
                                    }
                                    for &(left, top, right, bottom) in &selection_bounds {
                                        let l = layout.get();
                                        let b = Bounds::new(
                                            point(
                                                px(l.x + left * l.scale),
                                                px(l.y + top * l.scale),
                                            ),
                                            size(
                                                px((right - left) * l.scale),
                                                px((bottom - top) * l.scale),
                                            ),
                                        )
                                        .dilate(px(4.));
                                        window.paint_quad(quad(
                                            b,
                                            px(3.),
                                            gpui::transparent_black(),
                                            px(1.),
                                            rgb(0x4c8dff),
                                            Default::default(),
                                        ));
                                    }
                                    if let Some((left, top, right, bottom)) = marquee {
                                        let l = layout.get();
                                        let b = Bounds::new(
                                            point(
                                                px(l.x + left * l.scale),
                                                px(l.y + top * l.scale),
                                            ),
                                            size(
                                                px((right - left) * l.scale),
                                                px((bottom - top) * l.scale),
                                            ),
                                        );
                                        window.paint_quad(quad(
                                            b,
                                            px(0.),
                                            rgba(0x4c8dff22),
                                            px(1.),
                                            rgb(0x4c8dff),
                                            Default::default(),
                                        ));
                                    }
                                    text_input::paint(
                                        &text_entity,
                                        layout.get(),
                                        image_bounds,
                                        window,
                                        cx,
                                    );
                                },
                            );
                            let preparing = backdrop.is_some_and(|b| {
                                b.motion.uses_shader() && !motion_preview.borrow().ready_for(b)
                            });
                            if preparing {
                                paint_preparing(frame_bounds.intersect(&bounds), window, cx);
                            }
                            text_entity.update(cx, |editor, cx| {
                                if editor.playback.preparing != preparing {
                                    editor.sync_preview_preparation(preparing);
                                    cx.notify();
                                }
                            });
                        },
                    )
                    .h_full()
                    .flex_1()
                    .min_w_0(),
                    crate::accessibility::Node::group(format!(
                        "Screenshot canvas, {} by {} pixels, {} annotations{}",
                        dimensions.0,
                        dimensions.1,
                        self.document.marks.len(),
                        if self.playback.preparing {
                            ", preparing preview"
                        } else {
                            ""
                        }
                    )),
                ),
            )
            .when(self.panels.backdrop, |el| {
                el.child(self.backdrop_controls(cx))
            })
            .when(
                !self.panels.backdrop && !self.panels.enhance && !self.panels.animation,
                |el| el.child(self.tool_controls(cx)),
            )
            .when(self.panels.enhance, |el| {
                el.child(self.enhance_controls(cx))
            })
            .when(self.panels.animation, |el| {
                el.child(self.animation_controls(cx))
            })
    }
}

// An opaque cover hides partial backdrops/cards until a matching worker frame exists.
// Preparation has no measurable total, so use a status rather than a fake percentage.
fn paint_preparing(bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let theme = crate::theme::Theme::get(cx);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_quad(fill(bounds, rgb(theme.preparing)));
        let text: SharedString = "Preparing preview…".into();
        let font_size = px(14.);
        let line = window.text_system().shape_line(
            text.clone(),
            font_size,
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
        let badge = Bounds::new(
            bounds.center() - point((line.width + px(32.)) / 2., px(20.)),
            size(line.width + px(32.), px(40.)),
        );
        window.paint_quad(quad(
            badge,
            px(10.),
            rgb(theme.surface),
            px(1.),
            rgb(theme.border),
            Default::default(),
        ));
        let _ = line.paint(
            badge.origin + point(px(16.), px(10.)),
            font_size,
            window,
            cx,
        );
    });
}
