//! The one `Kit` every Lens view reads its colours, type, radii and shadows from.
//! It is rune-kit's neutral kit painted with attn's default scheme, so Lens reads as
//! part of attn.
use rune_kit::Kit;
use std::sync::OnceLock;

/// The kit for the light or dark appearance, built once per appearance.
pub(crate) fn kit(dark: bool) -> &'static Kit {
    static DARK: OnceLock<Kit> = OnceLock::new();
    static LIGHT: OnceLock<Kit> = OnceLock::new();
    let (cell, scheme) = if dark {
        (&DARK, &DARK_UI)
    } else {
        (&LIGHT, &LIGHT_UI)
    };
    cell.get_or_init(|| painted(dark, scheme))
}

/// The colours of one appearance of attn's scheme that `painted` maps onto the kit.
struct Scheme {
    bg_app: &'static str,
    bg_panel: &'static str,
    bg_elevated: &'static str,
    bg_selected: &'static str,
    bg_button: &'static str,
    bg_overlay: &'static str,
    border: &'static str,
    border_hover: &'static str,
    border_focus: &'static str,
    border_subtle: &'static str,
    text_primary: &'static str,
    text_secondary: &'static str,
    text_tertiary: &'static str,
    text_muted: &'static str,
    accent: &'static str,
    brand: &'static str,
    on_accent: &'static str,
    /// attn's `brand_ink`: `brand` 12% of the way from black, in encoded sRGB.
    brand_ink: &'static str,
}

// From attn's `internal/colors/schemes/attn.json` (`dark.ui` and `light.ui`) at attn
// commit e1099d7a9. A later change to attn's scheme is a diff against these values.
const DARK_UI: Scheme = Scheme {
    bg_app: "#0e0e11",
    bg_panel: "#131317",
    bg_elevated: "#1b1b20",
    bg_selected: "#30323e",
    bg_button: "#222227",
    bg_overlay: "#0e0e11b3",
    border: "#2a2b2f",
    border_hover: "#3d3e43",
    border_focus: "#54555c",
    border_subtle: "#1e1f23",
    text_primary: "#e7e7ea",
    text_secondary: "#c0c0c5",
    text_tertiary: "#9a9ba0",
    text_muted: "#7a7b81",
    accent: "#5ecad6",
    brand: "#a083ea",
    on_accent: "#0e0e11",
    brand_ink: "#13101c",
};

const LIGHT_UI: Scheme = Scheme {
    bg_app: "#f6f6f9",
    bg_panel: "#fefeff",
    bg_elevated: "#eff0f3",
    bg_selected: "#d6d8e8",
    bg_button: "#e5e6ea",
    bg_overlay: "#1a1a1f4d",
    border: "#d3d4d9",
    border_hover: "#afb0b7",
    border_focus: "#84858f",
    border_subtle: "#e4e4e8",
    text_primary: "#1a1a1f",
    text_secondary: "#46474e",
    text_tertiary: "#64656d",
    text_muted: "#8b8c92",
    accent: "#27869a",
    brand: "#8a66d9",
    on_accent: "#ffffff",
    brand_ink: "#110c1a",
};

/// The neutral kit with the palette fields attn's `painted()` sets
/// (`native/crates/attn-ui/src/lib.rs` at e1099d7a9), and its input ring and flat
/// neutral button, without which a field on the panel's own fill has no edge.
fn painted(dark: bool, s: &Scheme) -> Kit {
    let c = |hex: &str| rune::color::hex(hex).unwrap_or_else(|| panic!("{hex} is not a colour"));
    let mut kit = Kit::neutral(dark);
    let p = &mut kit.c;
    p.page = c(s.bg_app);
    p.bg_base = c(s.bg_panel);
    p.bg_deep = c(s.bg_app);
    p.layer_01 = c(s.bg_elevated);
    p.surface_raised = c(s.bg_elevated);
    p.surface_menu = c(s.bg_panel);
    p.surface_raised_hover = c(s.bg_selected);
    p.bg_button_neutral = c(s.bg_button);
    p.layer_04 = c(s.border_hover);
    p.bg_accent = c(s.accent);
    p.bg_icon_button_contrast = c(s.brand);
    p.submit_fg = c(s.brand_ink);
    p.submit_icon = c(s.brand_ink);
    p.text_base = c(s.text_primary);
    p.text_muted = c(s.text_secondary);
    p.text_faint = c(s.text_tertiary);
    p.text_weak = c(s.text_tertiary);
    p.text_accent = c(s.accent);
    p.text_on_accent = c(s.on_accent);
    p.icon_base = c(s.text_secondary);
    p.icon_muted = c(s.text_tertiary);
    p.icon_faint = c(s.text_muted);
    p.border_muted = c(s.border_subtle);
    p.border_base = c(s.border);
    p.std_border = c(s.border);
    p.border_strong = c(s.border_hover);
    p.border_focus = c(s.border_focus);
    p.hover = c(s.bg_elevated);
    p.pressed = c(s.bg_selected);
    p.scrim = c(s.bg_overlay);
    let ring = rune::Shadow {
        spread: 1.0,
        color: c(s.border),
        inset: true,
        ..rune::Shadow::default()
    };
    kit.elevation.button_neutral = Vec::new();
    kit.elevation = std::mem::take(&mut kit.elevation).with_input(vec![ring]);
    kit
}
