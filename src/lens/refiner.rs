//! The dictation refiner: the request Lens sends `attn model call` when voice
//! stops. Lens owns its instructions and its input (D1, D10 of `s-jhntry`); attn
//! runs them unchanged. The instructions came word for word from attn's
//! `internal/prompts/content/dictation-refiner`, its instructions and their
//! `plain` variant: Lens pastes into any app, so it asks for no Markdown.
//! `probes/refine` runs the corpus that judges a change to any of it.
use super::{attn, screen_words::Pair};
use std::time::Duration;

/// The model's system prompt.
pub fn instructions() -> &'static str {
    include_str!("refiner.md").trim_end()
}

/// The user message: JSON whose `transcript` is the raw text, so the model reads
/// the dictation as text to edit, not as instructions. `screen_words` are the
/// screen's words the transcript could mean and `maybe_misheard` the screen words
/// that sound like a transcript word (`s-0ww1ak` D5, D14); each field is there
/// only when it holds something, so a dictation without them reads as before.
pub fn input(transcript: &str, screen_words: &[String], maybe_misheard: &[Pair]) -> String {
    let mut input = serde_json::json!({ "transcript": transcript });
    if !screen_words.is_empty() {
        input["screen_words"] = screen_words.into();
    }
    if !maybe_misheard.is_empty() {
        input["maybe_misheard"] = maybe_misheard
            .iter()
            .map(|p| serde_json::json!({ "heard": p.heard, "screen": p.screen }))
            .collect();
    }
    input.to_string()
}

/// The call for `input`, from a recording `recording` long. The standard tier:
/// the screen-words probes measured that spelling from the screen needs Sonnet
/// or better (`s-0ww1ak` D15). No harness: that is the user's choice in attn.
pub fn request(input: &str, recording: Duration) -> attn::Request<'_> {
    attn::Request {
        purpose: "dictation-refine",
        tier: Some(attn::Tier::Standard),
        system: instructions(),
        input,
        limit: limit(recording),
    }
}

/// 15 s plus a quarter of the recording, the formula attn used for `attn
/// refine` (`s-jhntry` D6). Healthy refinements took 1.8–8.7 s, with a long
/// tail past 13 s (`s-jhntry` F8).
pub fn limit(recording: Duration) -> Duration {
    Duration::from_secs(15) + recording / 4
}
