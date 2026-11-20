//! The options of the selected mark, otherwise of the active tool.
use super::{
    super::{Editor, ToolSettings, actions::Action, icons::CIRCLE_QUESTION_MARK, live::rgba},
    controls::{self, Layer, button, choices, field, number},
};
use crate::{
    document::{Mark, Tool},
    style::{Cleanup, Dash, End, Fill, Style},
};
use rune::{Cursor, Cx, El, col, el, row, text};
use rune_kit::{IconButton, IconButtonVariant, Swatches};

const PRESETS: [([u8; 3], &str); 6] = [
    ([255, 56, 100], "Coral"),
    ([255, 184, 46], "Amber"),
    ([38, 182, 144], "Mint"),
    ([76, 141, 255], "Blue"),
    ([255, 255, 255], "White"),
    ([32, 34, 42], "Ink"),
];

const CROP_RATIOS: [(Option<f32>, &str); 5] = [
    (None, "Free"),
    (Some(1.), "1:1"),
    (Some(4. / 3.), "4:3"),
    (Some(16. / 9.), "16:9"),
    (Some(9. / 16.), "9:16"),
];

const DASHES: [(Dash, &str); 3] = [
    (Dash::Solid, "Solid"),
    (Dash::Dashed, "Dash"),
    (Dash::Dotted, "Dot"),
];

const ENDS: [(End, &str); 3] = [
    (End::None, "None"),
    (End::Arrow, "Arrow"),
    (End::Dot, "Dot"),
];

/// Name, shortcut and what the tool does, for the help line.
fn help(tool: Tool) -> (&'static str, &'static str, &'static str) {
    match tool {
        Tool::Select => (
            "Select",
            "V",
            "Drag empty canvas to select multiple annotations. Shift-click adds or removes; ⌘A selects all. Drag to move; Delete removes the selection.",
        ),
        Tool::Pen => ("Draw", "P", "Drag to draw a freehand stroke."),
        Tool::Arrow => (
            "Arrow",
            "A",
            "Drag to draw a line or arrow. Select it to adjust handles and add points.",
        ),
        Tool::Rectangle => ("Box", "R", "Drag to draw a box. Hold Shift for a square."),
        Tool::Text => (
            "Text",
            "T",
            "Click and type. Enter finishes; Escape cancels.",
        ),
        Tool::Highlight => (
            "Highlight",
            "H",
            "Drag to highlight an area while keeping details visible.",
        ),
        Tool::Pixelate => (
            "Pixelate",
            "B",
            "Drag over an area to pixelate it. Hold Shift for a square.",
        ),
        Tool::Crop => (
            "Crop",
            "X",
            "Drag to crop. Hold Shift for a square; ⌘Z undoes.",
        ),
        Tool::Counter => (
            "Step",
            "N",
            "Click to place numbered steps. Numbers advance automatically.",
        ),
        Tool::Spotlight => (
            "Spotlight",
            "S",
            "Drag a focus area to dim its surroundings. Handles resize it.",
        ),
        Tool::Magnifier => (
            "Magnifier",
            "M",
            "Drag from a detail to its lens position to magnify it.",
        ),
    }
}

impl Editor {
    fn color_section(&self, cx: &Cx<Editor>, tool: Tool, settings: ToolSettings) -> El {
        let k = controls::kit(cx);
        let label = if matches!(tool, Tool::Magnifier | Tool::Spotlight) {
            "Border color"
        } else {
            "Color"
        };
        let presets = PRESETS.map(|([r, g, b], name)| (rgba([r, g, b, 255]), name));
        let current = PRESETS
            .iter()
            .position(|(rgb, _)| settings.color[..3] == *rgb);
        let [r, g, b, _] = settings.color;
        let swatches = row()
            .gap(8.)
            .center_y()
            .child(
                Swatches::new(label, &presets)
                    .selected(current)
                    .on_pick(|s: &mut Editor, i, tx| {
                        s.dispatch_ui(Action::SetRgb(PRESETS[i].0), tx)
                    })
                    .build(k)
                    .key("color-presets"),
            )
            .child(
                el().key("custom-color")
                    .role("button")
                    .label("Custom color")
                    .a11y_value(format!("#{r:02X}{g:02X}{b:02X}"))
                    .size(22., 22.)
                    .radius(k.radius.md)
                    .fill(rgba([r, g, b, 255]))
                    .border(0.5, k.c.border_strong)
                    .cursor(Cursor::Pointer)
                    .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::ToggleColorPicker, tx)),
            );
        let mut section = col().gap(8.).child(field(cx, label, swatches));
        if self.picker.open && self.picker.stop.is_none() {
            section = section.child(self.color_picker_view(cx));
        }
        section.child(button(
            cx,
            "pick-screen",
            "Pick from screen",
            true,
            |s: &mut Editor, tx| s.dispatch_ui(Action::PickScreenColor, tx),
        ))
    }

    pub(super) fn tool_controls(&self, cx: &mut Cx<Editor>, layer: &mut Layer) -> El {
        let k = controls::kit(cx);
        let tool = self.options_tool();
        let settings = self.tool_settings();
        let style = settings.style;
        let selected = !self.selected.is_empty();
        let opacity = (settings.color[3] as f32 / 255. * 100.).round();
        let appearance = |change: Style| Action::SetAppearance(change);
        let (name, shortcut, description) = help(tool);
        let mut panel = col().key("tool-options").gap(14.).child(
            row()
                .justify_between()
                .center_y()
                .child(
                    text(if selected { "Selected" } else { name })
                        .key("tool-name")
                        .font(k.ty.body_medium)
                        .color(k.c.text_base),
                )
                .child(
                    IconButton::new(&CIRCLE_QUESTION_MARK)
                        .variant(IconButtonVariant::GhostMuted)
                        .label(format!("{name} help"))
                        .help(format!("{name} ({shortcut}): {description}"))
                        .build(k)
                        .key("tool-help"),
                ),
        );
        if !matches!(tool, Tool::Select | Tool::Crop | Tool::Pixelate) {
            panel = panel.child(self.color_section(cx, tool, settings));
        }
        match tool {
            Tool::Select => {
                panel = panel.child(
                    text("Select an annotation to edit its properties.")
                        .key("select-hint")
                        .wrap()
                        .font(k.ty.ui)
                        .color(k.c.text_muted),
                );
            }
            Tool::Pen | Tool::Arrow => {
                panel = panel.child(number(self, cx, "Thickness", settings.width, "px"));
            }
            Tool::Rectangle if style.fill == Fill::Outline => {
                panel = panel.child(number(self, cx, "Thickness", settings.width, "px"));
            }
            Tool::Rectangle | Tool::Crop | Tool::Highlight | Tool::Spotlight => {}
            Tool::Text => {
                panel = panel.child(number(self, cx, "Font size", settings.width * 7., "px"));
            }
            Tool::Pixelate => {
                panel = panel.child(number(
                    self,
                    cx,
                    "Block size",
                    (settings.width * 4.).max(12.),
                    "px",
                ));
            }
            Tool::Counter => {
                panel = panel
                    .child(number(
                        self,
                        cx,
                        "Badge diameter",
                        settings.width * 7.2,
                        "px",
                    ))
                    .child(number(
                        self,
                        cx,
                        if selected { "Number" } else { "Next number" },
                        self.counter_number() as f32,
                        "",
                    ));
            }
            Tool::Magnifier => {
                let probe = Mark {
                    tool,
                    style,
                    curve: None,
                    points: vec![],
                    color: settings.color,
                    width: settings.width,
                    text: String::new(),
                    drawn: None,
                };
                panel = panel.child(number(
                    self,
                    cx,
                    "Lens diameter",
                    crate::effects::radius(&probe) * 2.,
                    "px",
                ));
            }
        }
        if matches!(tool, Tool::Pen | Tool::Arrow | Tool::Rectangle | Tool::Text) {
            panel = panel.child(number(self, cx, "Opacity", opacity, "%"));
        }
        if tool == Tool::Highlight {
            panel = panel.child(number(self, cx, "Intensity", opacity, "%"));
        }
        if tool == Tool::Pen {
            panel = panel.child(field(
                cx,
                "Cleanup",
                choices(
                    self,
                    cx,
                    layer,
                    "cleanup",
                    "Cleanup",
                    [
                        (Cleanup::Raw, "Raw"),
                        (Cleanup::Smooth, "Smooth"),
                        (Cleanup::Adaptive, "Adaptive"),
                    ]
                    .map(|(cleanup, label)| {
                        (
                            label,
                            appearance(Style { cleanup, ..style }),
                            style.cleanup == cleanup,
                        )
                    }),
                ),
            ));
        }
        if tool == Tool::Arrow {
            panel = panel
                .child(field(
                    cx,
                    "Stroke",
                    choices(
                        self,
                        cx,
                        layer,
                        "stroke",
                        "Stroke",
                        DASHES.map(|(dash, label)| {
                            (
                                label,
                                appearance(Style { dash, ..style }),
                                style.dash == dash,
                            )
                        }),
                    ),
                ))
                .child(field(
                    cx,
                    "Start",
                    controls::select(
                        self,
                        cx,
                        layer,
                        "start",
                        "Start",
                        ENDS.map(|(start, label)| {
                            (
                                label,
                                appearance(Style { start, ..style }),
                                style.start == start,
                            )
                        }),
                    ),
                ))
                .child(field(
                    cx,
                    "End",
                    controls::select(
                        self,
                        cx,
                        layer,
                        "end",
                        "End",
                        ENDS.map(|(end, label)| {
                            (label, appearance(Style { end, ..style }), style.end == end)
                        }),
                    ),
                ))
                .child(field(
                    cx,
                    "Points",
                    row()
                        .gap(8.)
                        .child(button(
                            cx,
                            "add-point",
                            "Add point",
                            selected,
                            |s: &mut Editor, tx| s.dispatch_ui(Action::AddLinePoint, tx),
                        ))
                        .child(button(
                            cx,
                            "straighten",
                            "Straighten",
                            selected,
                            |s: &mut Editor, tx| s.dispatch_ui(Action::StraightenLine, tx),
                        )),
                ));
        }
        if tool == Tool::Rectangle {
            panel = panel
                .child(field(
                    cx,
                    "Fill",
                    choices(
                        self,
                        cx,
                        layer,
                        "fill",
                        "Fill",
                        [(Fill::Outline, "Outline"), (Fill::Filled, "Filled")].map(
                            |(fill, label)| {
                                (
                                    label,
                                    appearance(Style { fill, ..style }),
                                    style.fill == fill,
                                )
                            },
                        ),
                    ),
                ))
                .child(number(self, cx, "Corner radius", style.radius, "px"));
            if style.fill == Fill::Outline {
                panel = panel.child(field(
                    cx,
                    "Stroke",
                    choices(
                        self,
                        cx,
                        layer,
                        "stroke",
                        "Stroke",
                        DASHES.map(|(dash, label)| {
                            (
                                label,
                                appearance(Style { dash, ..style }),
                                style.dash == dash,
                            )
                        }),
                    ),
                ));
            }
        }
        if tool == Tool::Crop {
            panel = panel.child(field(
                cx,
                "Aspect ratio",
                choices(
                    self,
                    cx,
                    layer,
                    "aspect",
                    "Aspect ratio",
                    CROP_RATIOS.map(|(ratio, label)| {
                        (label, Action::SetCropRatio(ratio), self.crop_ratio == ratio)
                    }),
                ),
            ));
        }
        if tool == Tool::Spotlight {
            panel = panel.child(number(
                self,
                cx,
                "Dim surroundings",
                (style.dim * 100.).round(),
                "%",
            ));
        }
        if tool == Tool::Magnifier {
            panel = panel.child(field(
                cx,
                "Zoom",
                choices(
                    self,
                    cx,
                    layer,
                    "zoom",
                    "Zoom",
                    [2f32, 3., 4.].map(|zoom| {
                        (
                            format!("{zoom:.0}×"),
                            Action::SetMagnifierZoom(zoom),
                            settings.magnification == zoom,
                        )
                    }),
                ),
            ));
        }
        if selected {
            panel = panel.child(
                row()
                    .gap(8.)
                    .child(button(
                        cx,
                        "duplicate",
                        "Duplicate",
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::DuplicateSelection, tx),
                    ))
                    .child(button(
                        cx,
                        "delete",
                        "Delete",
                        true,
                        |s: &mut Editor, tx| s.dispatch_ui(Action::Delete, tx),
                    )),
            );
        }
        panel
    }
}
