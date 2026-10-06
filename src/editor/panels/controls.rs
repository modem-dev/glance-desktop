//! Shared inspector geometry and visual choices, independent of tool semantics.
use super::super::{Editor, actions::Action, view::HoverLabel};
use crate::style::{Dash, End, Fill};
use gpui::{prelude::*, *};

pub(super) fn panel(id: &'static str, cx: &Context<Editor>) -> Stateful<Div> {
    let theme = crate::theme::Theme::get(cx);
    div()
        .id(id)
        .debug_selector(move || id.into())
        .w(px(260.))
        .h_full()
        .flex_shrink_0()
        .p(px(14.))
        .flex()
        .flex_col()
        .gap_3()
        .overflow_y_scroll()
        .bg(rgb(theme.chrome))
        .border_l_1()
        .border_color(rgb(theme.divider))
        .cursor(CursorStyle::Arrow)
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.dispatch_ui(Action::CommitText, cx);
                cx.stop_propagation();
            }),
        )
}
pub(super) fn field(label: &str, content: impl IntoElement, cx: &App) -> AnyElement {
    let theme = crate::theme::Theme::get(cx);
    let id = format!("field-{}", label.to_lowercase().replace(' ', "-"));
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .flex()
        .flex_col()
        .gap_1()
        .min_w(px(0.))
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme.muted))
                .child(label.to_string()),
        )
        .child(content)
        .into_any_element()
}
pub(super) fn pair(a: impl IntoElement, b: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .gap_3()
        .child(div().flex_1().min_w(px(0.)).child(a))
        .child(div().flex_1().min_w(px(0.)).child(b))
        .into_any_element()
}
#[derive(Clone, Copy)]
pub(super) enum Sample {
    Stroke(Dash),
    Fill(Fill),
    End(End, bool),
    Plus,
    Straight,
}
pub(super) fn sample(sample: Sample, color: u32) -> impl IntoElement {
    canvas(
        |bounds, _, _| bounds,
        move |b, _, window, _| {
            let ink = rgb(color);
            let line = |window: &mut Window, x: f32, width: f32| {
                window.paint_quad(quad(
                    Bounds::new(
                        point(b.left() + px(x), b.center().y - px(1.)),
                        size(px(width), px(2.)),
                    ),
                    px(1.),
                    ink,
                    px(0.),
                    ink,
                    Default::default(),
                ));
            };
            match sample {
                Sample::Stroke(dash) => match dash {
                    Dash::Solid => line(window, 0., 24.),
                    Dash::Dashed => {
                        for x in [0., 9., 18.] {
                            line(window, x, 6.);
                        }
                    }
                    Dash::Dotted => {
                        for x in [0., 5., 10., 15., 20.] {
                            line(window, x, 2.);
                        }
                    }
                },
                Sample::Fill(fill) => {
                    window.paint_quad(quad(
                        Bounds::new(
                            point(b.center().x - px(7.), b.center().y - px(7.)),
                            size(px(14.), px(14.)),
                        ),
                        px(2.),
                        if fill == Fill::Filled {
                            ink.into()
                        } else {
                            transparent_black()
                        },
                        px(2.),
                        ink,
                        Default::default(),
                    ));
                }
                Sample::End(end, start) => {
                    line(window, 0., 24.);
                    let x = if start {
                        b.left() + px(1.)
                    } else {
                        b.right() - px(1.)
                    };
                    if end == End::Dot {
                        window.paint_quad(quad(
                            Bounds::new(
                                point(x - px(3.), b.center().y - px(3.)),
                                size(px(6.), px(6.)),
                            ),
                            px(3.),
                            ink,
                            px(0.),
                            ink,
                            Default::default(),
                        ));
                    } else if end == End::Arrow {
                        let dx = if start { px(5.) } else { px(-5.) };
                        let mut path = PathBuilder::stroke(px(2.));
                        path.move_to(point(x + dx, b.center().y - px(5.)));
                        path.line_to(point(x, b.center().y));
                        path.line_to(point(x + dx, b.center().y + px(5.)));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, ink);
                        }
                    }
                }
                Sample::Plus => {
                    line(window, 6., 12.);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.center().x - px(1.), b.center().y - px(6.)),
                            size(px(2.), px(12.)),
                        ),
                        ink,
                    ));
                }
                Sample::Straight => {
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(point(b.left() + px(4.), b.bottom() - px(3.)));
                    path.line_to(point(b.right() - px(4.), b.top() + px(3.)));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, ink);
                    }
                }
            }
        },
    )
    .w(px(24.))
    .h(px(16.))
}

impl Editor {
    pub(in crate::editor) fn choice(
        &self,
        id: String,
        label: String,
        content: AnyElement,
        state: (bool, bool),
        action: Action,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = crate::theme::Theme::get(cx);
        let (active, enabled) = state;
        let debug_id = id.clone();
        let element = div()
            .id(SharedString::from(id))
            .debug_selector(move || debug_id.clone())
            .flex_1()
            .min_w(px(0.))
            .h(px(30.))
            .px_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .text_xs()
            .bg(rgb(if active {
                theme.accent_background
            } else {
                theme.input
            }))
            .text_color(rgb(if !enabled {
                theme.disabled
            } else if active {
                theme.accent
            } else {
                theme.secondary
            }))
            .tooltip({
                let label = label.clone();
                move |_, cx| cx.new(|_| HoverLabel(label.clone().into())).into()
            })
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(rgb(theme.hover)))
                    .on_click(cx.listener({
                        let action = action.clone();
                        move |this, _, _, cx| this.dispatch_ui(action.clone(), cx)
                    }))
            })
            .child(content)
            .into_any_element();
        self.accessible_button(label, enabled, action, element)
    }
}
