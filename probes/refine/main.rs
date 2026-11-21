//! Runs the refinement corpus through `attn model call` with the request Lens
//! sends (`refiner::request`) and judges each answer: every phrase kept as many
//! times as it was said, every filler dropped. Synthetic text only.
//!
//! The screen-words cases send what Lens would for their screen: the entries
//! `screen_words::matching` keeps and the pairs `screen_words::sound_pairs` offers.
//! Each is judged as the prototype on s-vs128z judged it: cleaned (a capital first,
//! a stop last), kept (every `must`) and replaced (any `must_not`), and the probe
//! ends with the prototype's table, by kind.
//!
//!     cargo run --example refine-probe -- --rounds 3 --out refine.jsonl [--case ID]
//!
//! It finds attn as Lens does (`ATTN_BIN` overrides) and calls the attn instance
//! the environment selects (`ATTN_INSTANCE`). `LENS_PROBE_TOKEN` is a paired
//! sender's token; without it the probe pairs as "Attn Lens refine probe", waits
//! for Allow in attn and prints the token to reuse. Each record goes to the JSONL;
//! the exit is 1 when a refinement lost or changed what was said.
mod corpus;

use attn_lens::lens::{attn, refiner, screen_words};
use std::{collections::BTreeMap, io::Write, time::Instant};

/// The synthetic speaking rate: the 143 s plan dictation has 516 words.
const WORDS_PER_SECOND: f64 = 516.0 / 143.144;

fn main() {
    let mut rounds = 1;
    let (mut only, mut out) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--rounds", Some(n)) => rounds = n.parse().expect("--rounds takes a number"),
            ("--case", Some(id)) => only = Some(id),
            ("--out", Some(path)) => out = Some(path),
            _ => usage(),
        }
    }
    let Some(out) = out else { usage() };
    let bin = attn::locate().unwrap_or_else(|failure| fail(failure));
    let token = std::env::var("LENS_PROBE_TOKEN").unwrap_or_else(|_| pair(&bin));
    let mut file = std::fs::File::create(&out).expect("create --out");
    let mut failed = 0;
    let mut table: BTreeMap<corpus::Kind, [usize; 4]> = BTreeMap::new();
    for round in 1..=rounds {
        for case in corpus::cases() {
            if only.as_ref().is_some_and(|id| id != case.id) {
                continue;
            }
            let record = refine(&bin, &token, round, &case);
            writeln!(file, "{record}").expect("write --out");
            let failures = record["failures"].as_array().map_or(0, Vec::len);
            let verdict = if failures == 0 {
                "PASS"
            } else if record["known_loss"] == true {
                "KNOWN"
            } else {
                failed += 1;
                "FAIL"
            };
            println!(
                "{verdict} {:<14} r{round} {:<28} {:>7} ms  words {:.2}  {}",
                record["model"].as_str().unwrap_or("-"),
                case.id,
                record["wall_ms"],
                record["word_ratio"].as_f64().unwrap_or(0.0),
                record["failures"]
                    .as_array()
                    .map(|f| f
                        .iter()
                        .filter_map(|f| f.as_str())
                        .collect::<Vec<_>>()
                        .join("; "))
                    .unwrap_or_default(),
            );
        }
        for case in corpus::screen_cases() {
            if only.as_ref().is_some_and(|id| id != case.id) {
                continue;
            }
            let record = refine_with_screen(&bin, &token, round, case);
            writeln!(file, "{record}").expect("write --out");
            let row = table.entry(case.kind).or_default();
            let (cleaned, kept) = (record["cleaned"] == true, record["kept"] == true);
            let replaced = record["replaced"].as_array().is_some_and(|r| !r.is_empty());
            for (cell, on) in row.iter_mut().zip([true, cleaned, kept, replaced]) {
                *cell += on as usize;
            }
            println!(
                "{:<5} {:<14} r{round} {:<24} {:>7} ms  {}{}{} | {}",
                case.kind.name(),
                record["model"].as_str().unwrap_or("-"),
                case.id,
                record["wall_ms"],
                if cleaned { "clean " } else { "RAW " },
                if kept { "kept " } else { "LOST " },
                record["replaced"],
                record["refined"]
                    .as_str()
                    .or(record["error"].as_str())
                    .unwrap_or(""),
            );
        }
    }
    if !table.is_empty() {
        println!("kind     n  cleaned  kept  replaced");
        for (kind, [n, cleaned, kept, replaced]) in &table {
            println!(
                "{:<5} {n:>4} {cleaned:>8} {kept:>5} {replaced:>9}",
                kind.name()
            );
        }
    }
    if failed > 0 {
        eprintln!("refine-probe: {failed} refinements lost or changed what was said; see {out}");
        std::process::exit(1);
    }
}

fn refine(
    bin: &std::path::Path,
    token: &str,
    round: u32,
    case: &corpus::Case,
) -> serde_json::Value {
    let words = case.spoken.split_whitespace().count();
    let recording = std::time::Duration::from_secs_f64(words as f64 / WORDS_PER_SECOND);
    let input = refiner::input(&case.spoken, &[], &[]);
    let start = Instant::now();
    let result = attn::ModelCall::start(bin, token, &refiner::request(&input, recording))
        .and_then(|call| call.wait());
    let wall_ms = start.elapsed().as_millis() as u64;
    let mut record = serde_json::json!({
        "case": case.id,
        "round": round,
        "recording_ms": recording.as_millis() as u64,
        "wall_ms": wall_ms,
        "spoken": case.spoken,
    });
    match result {
        Ok(answer) => {
            let ratio = answer.text.split_whitespace().count() as f64 / words as f64;
            let failures = judge(case, &answer.text, ratio);
            record["harness"] = answer.harness.into();
            record["model"] = answer.model.into();
            record["model_ms"] = answer.model_ms.into();
            record["refined"] = answer.text.into();
            record["word_ratio"] = ratio.into();
            record["known_loss"] = (!failures.is_empty()
                && case
                    .known_loss_by
                    .contains(&record["harness"].as_str().unwrap_or("")))
            .into();
            record["failures"] = failures.into();
        }
        Err(attn::Failure::NotPaired(error) | attn::Failure::Error(error)) => {
            record["error"] = error.clone().into();
            record["failures"] = vec![format!("no refinement: {error}")].into();
        }
    }
    record
}

fn refine_with_screen(
    bin: &std::path::Path,
    token: &str,
    round: u32,
    case: &corpus::ScreenCase,
) -> serde_json::Value {
    let words = screen_words::matching(&case.entries(), case.spoken);
    let pairs = screen_words::sound_pairs(corpus::SCREEN_TEXT, case.spoken);
    let input = refiner::input(case.spoken, &words, &pairs);
    let count = case.spoken.split_whitespace().count();
    let recording = std::time::Duration::from_secs_f64(count as f64 / WORDS_PER_SECOND);
    let start = Instant::now();
    let result = attn::ModelCall::start(bin, token, &refiner::request(&input, recording))
        .and_then(|call| call.wait());
    let mut record = serde_json::json!({
        "case": case.id,
        "kind": case.kind.name(),
        "round": round,
        "wall_ms": start.elapsed().as_millis() as u64,
        "spoken": case.spoken,
        "input": serde_json::from_str::<serde_json::Value>(&input).unwrap(),
    });
    match result {
        Ok(answer) => {
            let text = answer.text.trim();
            let cleaned = text.chars().next().is_some_and(char::is_uppercase)
                && text.ends_with(['.', '?', '!']);
            let kept = case.must.iter().all(|must| {
                if must.chars().all(|c| !c.is_uppercase()) {
                    text.to_lowercase().contains(must)
                } else {
                    text.contains(must)
                }
            });
            let replaced: Vec<&str> = case
                .must_not
                .iter()
                .copied()
                .filter(|gone| text.contains(gone))
                .collect();
            record["cleaned"] = cleaned.into();
            record["kept"] = kept.into();
            record["replaced"] = replaced.into();
            record["harness"] = answer.harness.into();
            record["model"] = answer.model.into();
            record["model_ms"] = answer.model_ms.into();
            record["refined"] = answer.text.into();
        }
        Err(attn::Failure::NotPaired(error) | attn::Failure::Error(error)) => {
            record["error"] = error.into();
            record["cleaned"] = false.into();
            record["kept"] = false.into();
            record["replaced"] = serde_json::json!([]);
        }
    }
    record
}

/// What the refinement lost or kept that it should not have.
fn judge(case: &corpus::Case, refined: &str, ratio: f64) -> Vec<String> {
    let text = normalize(refined);
    let mut failures = Vec::new();
    for phrase in &case.keep {
        let found: usize = phrase
            .any
            .iter()
            .map(|alternative| occurrences(&text, &normalize(alternative)))
            .sum();
        if found != phrase.times {
            failures.push(format!(
                "{:?} appears {found} times, want {}",
                phrase.any[0], phrase.times
            ));
        }
    }
    for gone in case.drop {
        if text.contains(&normalize(gone)) {
            failures.push(format!("{gone:?} should have been removed"));
        }
    }
    if case.min_ratio > 0.0 && ratio < case.min_ratio {
        failures.push(format!(
            "kept {ratio:.2} of the words, want at least {:.2}",
            case.min_ratio
        ));
    }
    failures
}

/// Counts `padded` in `text`; neighbours share the space between them.
fn occurrences(text: &str, padded: &str) -> usize {
    let mut count = 0;
    let mut from = 0;
    while let Some(at) = text[from..].find(padded) {
        count += 1;
        from += at + padded.len() - 1;
    }
    count
}

/// Lowercase words with the punctuation a refinement adds taken out, padded with
/// a space on each side: the Go probe's `normalize`, kept so results compare.
fn normalize(text: &str) -> String {
    let text = text
        .to_lowercase()
        .replace(['’', '‘'], "'")
        .replace(['—', '–'], " ");
    let kept: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || "'./_+:-".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = kept
        .split_whitespace()
        .map(|word| word.trim_end_matches(['.', ':', '\'', '-']))
        .filter(|word| !word.is_empty())
        .collect();
    format!(" {} ", words.join(" "))
}

fn pair(bin: &std::path::Path) -> String {
    let token = attn::pair_as(bin, "Attn Lens refine probe", |request| {
        eprintln!("refine-probe: allow pairing in attn, code {}", request.code)
    })
    .unwrap_or_else(|failure| fail(failure));
    eprintln!("refine-probe: paired; rerun with LENS_PROBE_TOKEN={token}");
    token
}

fn fail(failure: attn::Failure) -> ! {
    let (attn::Failure::NotPaired(error) | attn::Failure::Error(error)) = failure;
    eprintln!("refine-probe: {error}");
    std::process::exit(1)
}

fn usage() -> ! {
    eprintln!("usage: refine-probe --out FILE [--rounds N] [--case ID]");
    std::process::exit(2)
}
