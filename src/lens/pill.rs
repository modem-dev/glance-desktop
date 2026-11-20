//! The pill: a dictation's words while you speak, then the raw text while attn
//! refines it, then how it ended. It sits above the Dock at the bottom of the
//! display with the pointer, and never takes focus or clicks: ↩ and Esc reach it
//! from the key engine (F7 on the plan).
use super::{
    Lens, Summary, VoiceState, bar::voice_text, dictation::DictationSummary, session::DisplayFrame,
};
use rune::{Cx, El, col, el, text};
use std::sync::Arc;

pub const SIZE: (f32, f32) = (520., 72.);
/// Points between the pill and the Dock, or the display's bottom.
const MARGIN: f64 = 12.;
/// The pill shows the end of what was said; the whole text pastes.
const SHOWN_CHARACTERS: usize = 120;

/// The pill's window on `display`: bottom center, above the Dock.
pub fn placement(display: DisplayFrame) -> [f32; 4] {
    [
        ((display.frame[2] - f64::from(SIZE.0)) / 2.) as f32,
        (display.frame[3] - display.dock - f64::from(SIZE.1) - MARGIN) as f32,
        SIZE.0,
        SIZE.1,
    ]
}

pub struct Pill {
    pub lens: Arc<Lens>,
}

/// The pill's two lines: the words, and what is happening to them.
fn lines(dictation: &DictationSummary) -> (String, String) {
    match dictation {
        DictationSummary::Recording { status, words } => (
            words.clone(),
            voice_text(status.map_or(VoiceState::Starting, VoiceState::Running)),
        ),
        DictationSummary::Finishing { words } => (words.clone(), "Finishing…".into()),
        DictationSummary::Refining { words } => (
            words.clone(),
            "Refining… · ↩ paste as is · esc cancel".into(),
        ),
        DictationSummary::Settled { text, how, pasting } => (
            text.clone(),
            if *pasting {
                "Pasting…".into()
            } else {
                how.note()
            },
        ),
    }
}

/// The last [`SHOWN_CHARACTERS`] of `words`, from a word's start.
fn tail(words: &str) -> String {
    let count = words.chars().count();
    if count <= SHOWN_CHARACTERS {
        return words.into();
    }
    let rest: String = words.chars().skip(count - SHOWN_CHARACTERS).collect();
    let from_word = rest
        .split_once(' ')
        .map_or(rest.as_str(), |(_, after)| after);
    format!("…{from_word}")
}

pub fn pill_view(s: &Pill, _: &mut Cx<Pill>) -> El {
    let k = crate::kit::hud();
    s.lens.read(|summary: &Summary| {
        let Some(dictation) = &summary.dictation else {
            return el().key("pill-empty");
        };
        let (words, status) = lines(dictation);
        col()
            .key("pill")
            .w_full()
            .h_full()
            .padding(12.)
            .gap(4.)
            .radius(k.radius.xl + 2.)
            .fill(k.c.bg_base)
            .border(1., crate::kit::hud_edge())
            .child(
                text(if words.is_empty() {
                    "…".into()
                } else {
                    tail(&words)
                })
                .key("pill-words")
                .font(k.ty.body)
                .color(k.c.text_base),
            )
            .child(
                text(status)
                    .key("pill-status")
                    .font(k.ty.small_medium)
                    .color(k.c.text_accent),
            )
    })
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn long_words_show_their_end_from_a_word_start() {
        let words = format!("{} the end", "word ".repeat(40));
        let shown = tail(&words);
        assert!(shown.starts_with("…word"), "{shown}");
        assert!(shown.ends_with("the end"));
        assert_eq!(tail("short"), "short");
    }
}
