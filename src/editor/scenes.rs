//! The editor's scenes, for `--window editor --scene NAME`: one per screen and hard
//! state. `scripts/snap.sh` renders them all. Each scene opens the window as a Dock
//! click does, pastes an image as ⌘V does, and draws with the canvas gesture's own
//! press, move and release; options go through the `Action` each control dispatches.
//! A state that needs a job held in flight, such as an export running, stays a mark
//! in the `headless/editor` scripts.
use super::{Editor, actions::Action};
use crate::app::Scenes;
use crate::{
    document::Tool,
    style::{Dash, End, Fill},
};
use image::{Rgba, RgbaImage};
use rune::{TextEdit, Tx};

pub(crate) const SCENES: Scenes<Editor> = &[
    ("empty", |e| e.open_window()),
    ("open", |e| open(e)),
    ("select-none", select_none),
    ("select-both", select_both),
    ("box", box_filled),
    ("pen", pen),
    ("arrow", arrow),
    ("arrow-tool", arrow_tool),
    ("text", text),
    ("highlight", highlight),
    ("pixelate", pixelate),
    ("crop", crop),
    ("crop-ratios", crop_ratios),
    ("step", step),
    ("spotlight", spotlight),
    ("magnifier", magnifier),
    ("backdrop", backdrop),
    ("backdrop-gradient", backdrop_gradient),
    ("backdrop-color", backdrop_color),
    ("animation", animation),
    ("image-tools", image_tools),
    ("color-picker", color_picker),
    ("copied", copied),
    ("wide-image", wide_image),
];

/// The image the editor's scripts open, `headless/editor/fixture.png`, 320 by 200.
const FIXTURE: &[u8] = include_bytes!("../../headless/editor/fixture.png");

const MINT: [u8; 3] = [38, 182, 144];
const AMBER: [u8; 3] = [255, 184, 46];
const BLUE: [u8; 3] = [76, 141, 255];
const WHITE: [u8; 3] = [255, 255, 255];
const INK: [u8; 3] = [32, 34, 42];

fn act(e: &mut Editor, action: Action) {
    e.dispatch_ui(action, &mut Tx::default());
    settle(e);
}

/// Takes in what the port's jobs posted, as the window's next frame would. The fake
/// port runs jobs as they are asked for, so they have all finished.
fn settle(e: &mut Editor) {
    e.poll(0., &mut Tx::default());
}

/// Opens the window and pastes `image`.
fn paste(e: &mut Editor, image: RgbaImage) {
    e.open_window();
    e.port
        .copy_image(image)
        .expect("the clipboard takes an image");
    act(e, Action::PasteImage);
}

fn open(e: &mut Editor) {
    let image = image::load_from_memory(FIXTURE).expect("the fixture decodes");
    paste(e, image.to_rgba8());
}

/// A press at `from` and a release at `to`, in image pixels, through the canvas
/// gesture. It draws at actual size, where a canvas point is a pixel, then fits.
fn drag(e: &mut Editor, from: (f32, f32), to: (f32, f32)) {
    act(e, Action::ActualSize);
    let layout = e.layout();
    let point = |(x, y): (f32, f32)| (layout.x + x * layout.scale, layout.y + y * layout.scale);
    let tx = &mut Tx::default();
    let (start, end) = (point(from), point(to));
    e.begin(start.0, start.1, false, tx);
    for step in 1..=8 {
        let f = step as f32 / 8.;
        let x = start.0 + (end.0 - start.0) * f;
        // A pen stroke bows, so it reads as drawn by hand.
        let y = start.1 + (end.1 - start.1) * f - (f * std::f32::consts::PI).sin() * 12.;
        e.motion(x, y, false);
    }
    e.finish(end.0, end.1, false, tx);
    settle(e);
    act(e, Action::Fit);
}

fn tap(e: &mut Editor, at: (f32, f32)) {
    drag(e, at, at);
}

fn number(e: &mut Editor, label: &str, value: f32) {
    let action = e.number_action(label, value);
    act(e, action);
}

fn style(e: &mut Editor, change: impl FnOnce(&mut crate::style::Style)) {
    let mut style = e.tool_settings().style;
    change(&mut style);
    act(e, Action::SetAppearance(style));
}

/// Two boxes, the select tool, nothing selected.
fn select_none(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Rectangle));
    drag(e, (30., 30.), (80., 60.));
    drag(e, (140., 90.), (190., 130.));
    act(e, Action::SelectTool(Tool::Select));
}

fn select_both(e: &mut Editor) {
    select_none(e);
    drag(e, (10., 10.), (300., 185.));
}

/// A square, dotted, rounded box, filled.
fn box_filled(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Rectangle));
    number(e, "Corner radius", 12.);
    style(e, |s| s.dash = Dash::Dotted);
    number(e, "Thickness", 4.);
    drag(e, (20., 20.), (80., 80.));
    style(e, |s| s.fill = Fill::Filled);
}

fn pen(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Pen));
    act(e, Action::SetRgb(MINT));
    number(e, "Thickness", 12.);
    number(e, "Opacity", 90.);
    style(e, |s| s.cleanup = crate::style::Cleanup::Smooth);
    drag(e, (20., 20.), (140., 100.));
}

/// A dashed line with an arrow at its start and a dot at its end.
fn arrow(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Arrow));
    style(e, |s| {
        s.dash = Dash::Dashed;
        s.start = End::Arrow;
        s.end = End::Dot;
    });
    drag(e, (20., 20.), (220., 20.));
}

/// The arrow tool picked from the Tools menu, before anything is drawn.
fn arrow_tool(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Arrow));
}

fn text(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Text));
    act(e, Action::SetRgb(BLUE));
    number(e, "Font size", 70.);
    tap(e, (20., 20.));
    e.text_edit = TextEdit::new("label");
    e.commit_text(&mut Tx::default());
    settle(e);
}

fn highlight(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Highlight));
    act(e, Action::SetRgb(AMBER));
    number(e, "Intensity", 30.);
    drag(e, (20., 20.), (120., 80.));
}

fn pixelate(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Pixelate));
    number(e, "Block size", 32.);
    drag(e, (20., 20.), (140., 100.));
}

/// Cropped to a square.
fn crop(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Crop));
    act(e, Action::SetCropRatio(Some(1.)));
    drag(e, (20., 20.), (120., 60.));
}

/// Cropped to a square, with the aspect ratio's list open, as its trigger opens it.
fn crop_ratios(e: &mut Editor) {
    crop(e);
    e.toggle_choices("aspect");
}

/// Two numbered badges, starting at 7.
fn step(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Counter));
    act(e, Action::SetRgb(WHITE));
    number(e, "Next number", 7.);
    number(e, "Badge diameter", 72.);
    tap(e, (40., 40.));
    tap(e, (100., 40.));
}

fn spotlight(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Spotlight));
    act(e, Action::SetRgb(INK));
    number(e, "Dim surroundings", 50.);
    drag(e, (100., 60.), (180., 120.));
}

fn magnifier(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Magnifier));
    act(e, Action::SetMagnifierZoom(3.));
    number(e, "Lens diameter", 96.);
    drag(e, (60., 60.), (210., 110.));
}

fn backdrop(e: &mut Editor) {
    open(e);
    act(e, Action::ToggleBackdrop);
}

fn backdrop_gradient(e: &mut Editor) {
    backdrop(e);
    act(e, Action::SetBackdropFill { gradient: true });
}

/// The colour picker open on the second colour of the Ocean gradient.
fn backdrop_color(e: &mut Editor) {
    backdrop_gradient(e);
    act(e, Action::SetBackdropPreset(1));
    act(e, Action::EditBackdropColor(1));
}

fn animation(e: &mut Editor) {
    open(e);
    act(e, Action::ToggleAnimationPanel);
}

fn image_tools(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Rectangle));
    drag(e, (40., 40.), (90., 70.));
    act(e, Action::ToggleImageTools);
}

fn color_picker(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Pen));
    act(e, Action::ToggleColorPicker);
}

/// The toolbar's confirmation after ⌘C.
fn copied(e: &mut Editor) {
    open(e);
    act(e, Action::SelectTool(Tool::Rectangle));
    style(e, |s| s.fill = Fill::Filled);
    drag(e, (20., 20.), (60., 60.));
    act(e, Action::CopyImage);
}

/// An image wider than the view holds, shown downscaled, with a box on it.
fn wide_image(e: &mut Editor) {
    paste(
        e,
        RgbaImage::from_pixel(8400, 66, Rgba([90, 100, 120, 255])),
    );
    act(e, Action::SelectTool(Tool::Rectangle));
    style(e, |s| s.fill = Fill::Filled);
    drag(e, (20., 10.), (60., 50.));
}
