//! App chrome only. Document, annotation, backdrop and export colors stay independent.
use gpui::{App, Global, Window, WindowAppearance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Theme {
    pub(crate) chrome: u32,
    pub(crate) surface: u32,
    pub(crate) workspace: u32,
    pub(crate) input: u32,
    pub(crate) hover: u32,
    pub(crate) pressed: u32,
    pub(crate) border: u32,
    pub(crate) divider: u32,
    pub(crate) text: u32,
    pub(crate) secondary: u32,
    pub(crate) muted: u32,
    pub(crate) disabled: u32,
    pub(crate) accent: u32,
    pub(crate) accent_fill: u32,
    pub(crate) accent_background: u32,
    pub(crate) positive: u32,
    pub(crate) positive_fill: u32,
    pub(crate) positive_background: u32,
    pub(crate) error: u32,
    pub(crate) track: u32,
    pub(crate) slider_fill: u32,
    pub(crate) knob: u32,
    pub(crate) knob_border: u32,
    pub(crate) on_fill: u32,
    pub(crate) tooltip: u32,
    pub(crate) tooltip_text: u32,
    pub(crate) tooltip_detail: u32,
    pub(crate) preparing: u32,
}
impl Global for Theme {}

impl Theme {
    pub(crate) fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self {
                chrome: 0xfcfcfd,
                surface: 0xffffff,
                workspace: 0xeff0f4,
                input: 0xf0f1f5,
                hover: 0xf0f1f5,
                pressed: 0xe5e7ed,
                border: 0xdfe1e7,
                divider: 0xe5e5ec,
                text: 0x272831,
                secondary: 0x555966,
                muted: 0x646976,
                disabled: 0xa6a8b2,
                accent: 0xb63d2a,
                accent_fill: 0xf35d45,
                accent_background: 0xffe9e4,
                positive: 0x127969,
                positive_fill: 0x27856f,
                positive_background: 0xe5f4f0,
                error: 0xd33d3d,
                track: 0xe7e8ee,
                slider_fill: 0x32b49b,
                knob: 0xffffff,
                knob_border: 0xd3d6df,
                on_fill: 0xffffff,
                tooltip: 0x282b34,
                tooltip_text: 0xffffff,
                tooltip_detail: 0xc6c9d3,
                preparing: 0xe7e9ef,
            },
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                chrome: 0x202229,
                surface: 0x292c35,
                workspace: 0x17191f,
                input: 0x343844,
                hover: 0x3c414f,
                pressed: 0x474d5d,
                border: 0x505665,
                divider: 0x373c48,
                text: 0xf0f1f5,
                secondary: 0xc5c9d4,
                muted: 0xb2b8c6,
                disabled: 0x80889a,
                accent: 0xff9b89,
                accent_fill: 0xf35d45,
                accent_background: 0x50342f,
                positive: 0x75d9bc,
                positive_fill: 0x27856f,
                positive_background: 0x25483e,
                error: 0xff9999,
                track: 0x454b59,
                slider_fill: 0x32b49b,
                knob: 0xf0f1f5,
                knob_border: 0x747d90,
                on_fill: 0xffffff,
                tooltip: 0x343844,
                tooltip_text: 0xf0f1f5,
                tooltip_detail: 0xc6c9d3,
                preparing: 0x202229,
            },
        }
    }

    pub(crate) fn get(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or_else(|| Self::for_appearance(cx.window_appearance()))
    }

    pub(crate) fn install(window: &Window, cx: &mut App) {
        Self::apply(window.appearance(), cx);
        window
            .observe_window_appearance(|window, cx| {
                Self::apply(window.appearance(), cx);
            })
            .detach();
    }

    pub(crate) fn apply(appearance: WindowAppearance, cx: &mut App) {
        cx.set_global(Self::for_appearance(appearance));
        // Refresh child entities too, including open pickers and numeric inputs.
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: u32) -> f64 {
        [16, 8, 0]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(shift, weight)| {
                let channel = ((color >> shift) & 255) as f64 / 255.;
                weight
                    * if channel <= 0.04045 {
                        channel / 12.92
                    } else {
                        ((channel + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }

    #[test]
    fn system_palettes_keep_labels_and_selected_controls_readable() {
        for appearance in [WindowAppearance::Light, WindowAppearance::Dark] {
            let theme = Theme::for_appearance(appearance);
            for (foreground, background) in [
                (theme.text, theme.chrome),
                (theme.text, theme.surface),
                (theme.text, theme.input),
                (theme.muted, theme.chrome),
                (theme.secondary, theme.hover),
                (theme.accent, theme.accent_background),
                (theme.positive, theme.positive_background),
                (theme.error, theme.surface),
                (theme.tooltip_text, theme.tooltip),
                (theme.tooltip_detail, theme.tooltip),
            ] {
                let a = luminance(foreground);
                let b = luminance(background);
                let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "{appearance:?}: #{foreground:06x} on #{background:06x}: {contrast:.2}"
                );
            }
        }
        assert_eq!(
            Theme::for_appearance(WindowAppearance::Light),
            Theme::for_appearance(WindowAppearance::VibrantLight)
        );
        assert_eq!(
            Theme::for_appearance(WindowAppearance::Dark),
            Theme::for_appearance(WindowAppearance::VibrantDark)
        );
    }
}
