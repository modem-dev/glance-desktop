//! Application intent, independent of buttons, key events, GPUI, and transports.
use crate::{
    animation::Motion,
    backdrop::{Backdrop, Control, Format},
    document::Tool,
};
use std::path::PathBuf;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Edit {
        edit: crate::document::actions::DocumentAction,
    },
    /// Prepared by a worker; optimistic revision checking prevents lost edits.
    #[serde(skip)]
    ApplyPreparedDocument {
        document: Box<crate::document::Document>,
        revision: u64,
        replace: bool,
    },
    Show,
    Capture {
        area: bool,
    },
    OpenImage,
    OpenPath {
        path: PathBuf,
    },
    SaveImage,
    CopyImage,
    CopyRemote,
    PasteImage,
    /// Offline OCR of the source screenshot; optional [x, y, width, height].
    CopyOcr {
        #[serde(default)]
        rectangle: Option<[u32; 4]>,
    },
    // Contextual editing commands operate on text while an inline edit is open.
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    Delete,
    DuplicateSelection,
    SelectAll,
    SelectAnnotations {
        ids: Vec<String>,
    },
    /// Select intersecting annotation bounds in source image pixels.
    SelectRegion {
        rectangle: (f32, f32, f32, f32),
        #[serde(default)]
        additive: bool,
    },
    SelectTool {
        tool: Tool,
    },
    SetColor {
        color: [u8; 4],
    },
    /// Samples the unannotated source image and preserves annotation opacity.
    SampleToolColor {
        position: (u32, u32),
    },
    PickToolScreenColor,
    /// Canvas gesture; MCP uses SampleToolColor with source pixel coordinates.
    BeginToolColorSampling,
    SetStrokeWidth {
        width: f32,
    },
    SetAppearance {
        style: crate::style::Style,
    },
    SetMagnifierZoom {
        zoom: f32,
    },
    SetCounterNumber {
        number: u32,
    },
    SetCropRatio {
        ratio: Option<f32>,
    },
    AddLinePoint,
    StraightenLine,
    CycleStrokeWidth,
    CycleMagnifierZoom,
    NudgeSelection {
        delta: (f32, f32),
        remember: bool,
    },
    Fit,
    ActualSize,
    Zoom {
        factor: f32,
    },
    ZoomAt {
        factor: f32,
        anchor: (f32, f32),
    },
    PanBy {
        delta: (f32, f32),
    },
    ToggleBackdrop,
    ToggleEnhance,
    ToggleAnimationPanel,
    ClosePanel {
        panel: Panel,
    },
    SetResizeScale {
        scale: f32,
    },
    ToggleSmartResize,
    ApplyResize,
    Resize {
        scale: f32,
        smart: bool,
    },
    Rotate,
    SetBackdrop {
        backdrop: Option<Backdrop>,
    },
    SetBackdropFormat {
        format: Format,
    },
    ToggleBackdropEnabled,
    SetBackdropFill {
        gradient: bool,
    },
    SelectMotion {
        motion: Motion,
    },
    /// None picks a fresh seed; Some reproduces a specific looping variation.
    RandomizeMotion {
        #[serde(default)]
        seed: Option<u32>,
    },
    SetBackdropPreset {
        preset: usize,
    },
    SetBackdropColor {
        stop: usize,
        rgb: [u8; 3],
    },
    SampleBackdropColor {
        stop: usize,
        position: (u32, u32),
    },
    PickBackdropScreenColor {
        stop: usize,
    },
    /// Canvas gesture; MCP uses SampleBackdropColor with source pixel coordinates.
    BeginBackdropColorSampling {
        stop: usize,
    },
    SetBackdropControl {
        control: Control,
        value: u32,
    },
    BeginBackdropAdjustment {
        control: Control,
        track: (f32, f32, f32, f32),
        position: (f32, f32),
    },
    TogglePlayback,
    ReplayAnimation,
    SeekAnimation {
        seconds: f32,
    },
    SelectEntrance {
        effect: crate::animation::Entrance,
    },
    SetImageAnimation {
        animation: crate::animation::ImageAnimation,
    },
    SetAnimationControl {
        control: crate::animation::AnimationControl,
        value: u32,
    },
    BeginAnimationAdjustment {
        control: crate::animation::AnimationControl,
        track: (f32, f32, f32, f32),
        position: (f32, f32),
    },
    ExportAnimation {
        format: AnimationFormat,
    },
    CancelExport,
    RevealExport,
    CommitText,
    Cancel,
    Help,
    Quit,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Panel {
    Backdrop,
    Enhance,
    Animation,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnimationFormat {
    Mp4,
    Gif,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct ActionReceipt {
    pub(crate) revision: u64,
    /// Accepted background work has started; this is not a completion receipt.
    pub(crate) operation_id: Option<u64>,
}

impl Action {
    pub(crate) fn from_json(value: serde_json::Value) -> Result<Self, String> {
        let action: Self = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        // Internally tagged unit variants can ignore extra keys even with
        // deny_unknown_fields. Check the selected variant's fields as well.
        let canonical = serde_json::to_value(&action).map_err(|e| e.to_string())?;
        if let Some(fields) = value.as_object() {
            for key in fields.keys() {
                if canonical.get(key).is_none() {
                    return Err(format!("Unexpected action field: {key}"));
                }
            }
        }
        Ok(action)
    }
}
