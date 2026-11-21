//! Screen words: the identifiers and unusual words on the front window, offered to
//! dictation refinement as a spelling reference. Three pure steps, ported from the
//! prototype on s-vs128z and specified in the plan's Design (s-0ww1ak):
//!
//! - [`pick`] keeps the tokens of a screen read worth spelling (Design › Filter).
//! - [`matching`] keeps only the picked entries the raw transcript could mean
//!   (Design › Match), so nothing the user did not say leaves the Mac (D5).
//! - [`sound_pairs`] pairs transcript words with screen words that sound like them
//!   (Design › Sound pairs, D14).
use std::collections::{BTreeSet, HashMap, HashSet};

/// The longest token `pick` keeps, in characters: longer ones are wrapped paths
/// and OCR runs, which nobody dictates.
const SPEAKABLE: usize = 48;
/// A `/`-separated segment this long with a digit is a key, UUID or hash (D9).
const SECRET_SEGMENT: usize = 20;
/// Edge punctuation that never belongs to an identifier.
const EDGE: &[char] = &[
    '"', '\'', '`', '“', '”', '‘', '’', '(', ')', '[', ']', '{', '}', '<', '>', ',', ';', ':', '!',
    '?', '…', '·', '•', '|', '*',
];

/// Distinct tokens of `text` worth spelling, in reading order. `known` answers
/// whether a lowercase word is ordinary English.
pub fn pick(text: &str, known: impl Fn(&str) -> bool) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut kept = vec![];
    for raw in text.split_whitespace() {
        let mut token = raw.trim_matches(EDGE).trim_end_matches('.');
        for possessive in ["'s", "’s"] {
            token = token.strip_suffix(possessive).unwrap_or(token);
        }
        if token.is_empty() || token.contains(['\'', '’']) || seen.contains(token) {
            continue;
        }
        if worth_spelling(token, &known) {
            seen.insert(token);
            kept.push(token.to_string());
        }
    }
    kept
}

fn worth_spelling(token: &str, known: &impl Fn(&str) -> bool) -> bool {
    let length = token.chars().count();
    let secret = |segment: &str| {
        segment.chars().count() >= SECRET_SEGMENT && segment.chars().any(|c| c.is_numeric())
    };
    if token.contains("://") || token.to_lowercase().starts_with("www.") || length > SPEAKABLE {
        return false;
    }
    if token.split('/').any(secret) {
        return false;
    }
    if flag(token)
        || path(token)
        || camel(token)
        || snake(token)
        || dotted(token)
        || (length >= 3 && mixed(token))
    {
        return true;
    }
    let parts: Vec<&str> = token.split('-').filter(|p| !p.is_empty()).collect();
    if length < 3 || parts.is_empty() || !parts.iter().all(|p| p.chars().all(char::is_alphabetic)) {
        return false;
    }
    let knows = |part: &str| {
        let part = part.to_lowercase();
        known(&part) || part.strip_suffix('s').is_some_and(known)
    };
    !parts.into_iter().all(knows)
}

/// `--token-stdin`, `-v1`.
fn flag(token: &str) -> bool {
    let rest = token.strip_prefix("--").or_else(|| token.strip_prefix('-'));
    rest.is_some_and(|rest| {
        let mut chars = rest.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && rest.len() >= 2
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// Some characters on either side of a "/" that belong in a path: `src/lens`, `~/projects`.
fn path(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    chars.windows(3).any(|w| {
        w[1] == '/'
            && (w[0].is_ascii_alphanumeric() || "_.~-".contains(w[0]))
            && (w[2].is_ascii_alphanumeric() || "_.-".contains(w[2]))
    })
}

/// `scopeTest`, `ScopeTest`, `HTTPServer`.
fn camel(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    chars
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
        || chars.windows(3).any(|w| {
            w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase() && w[2].is_ascii_lowercase()
        })
}

/// `snake_case`.
fn snake(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    chars
        .windows(3)
        .any(|w| w[1] == '_' && w[0].is_ascii_alphanumeric() && w[2].is_ascii_alphanumeric())
}

/// `dictation.rs`, `foo.bar`: a name, a dot, and a name.
fn dotted(token: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let starts_name = |s: &str| s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    let Some((head, tail)) = token.split_once('.') else {
        return false;
    };
    starts_name(head)
        && head.chars().all(|c| word(c) || c == '-')
        && starts_name(tail)
        && tail.chars().all(|c| word(c) || c == '.')
        && !tail.ends_with('.')
}

/// Letters with digits: `t-42`, `c7f1`, `gpt-6`.
fn mixed(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_alphabetic()) && token.chars().any(|c| c.is_numeric())
}

/// Spoken punctuation, which the transcriber writes as words.
const SPOKEN: &[&str] = &[
    "slash",
    "dot",
    "dash",
    "hyphen",
    "underscore",
    "colon",
    "tilde",
    "period",
    "backslash",
    "hash",
];
/// Parts over this many letters may match by similarity; shorter ones exactly,
/// because a ratio on them let `tne` match "then".
const SHORT_PART: usize = 4;
/// Keeps `Johannes` for "yohannes" (0.875) and drops `parse_header` for "parses
/// the header" (0.80): the prototype's corpus.
const PART_RATIO: f64 = 0.82;
/// The longest abbreviation of one spoken word: `src` for "source".
const ABBREVIATION: usize = 3;
/// The most spoken words one part of an entry can take.
const GROUP: usize = 3;

/// The entries of `entries` that some run of the transcript's words could mean, in
/// their order and without repeats.
pub fn matching(entries: &[String], transcript: &str) -> Vec<String> {
    let lower = transcript.to_lowercase();
    let spoken: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|w| !w.is_empty() && !SPOKEN.contains(w))
        .collect();
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter(|entry| seen.insert(entry.as_str()) && means(&parts(entry), &spoken))
        .cloned()
        .collect()
}

/// An entry's parts, lowercase: runs of letters and digits, split again at camelCase
/// and PascalCase humps and digit runs. `src/lens/dictation.rs` gives src, lens,
/// dictation, rs; `HTTPServer` gives http, server.
fn parts(entry: &str) -> Vec<String> {
    let mut out = vec![];
    for chunk in entry
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|c| !c.is_empty())
    {
        let chars: Vec<char> = chunk.chars().collect();
        let run = |from: usize, test: fn(&char) -> bool| {
            from + chars[from..].iter().take_while(|c| test(c)).count()
        };
        let mut at = 0;
        while at < chars.len() {
            let end = if chars[at].is_ascii_digit() {
                run(at, char::is_ascii_digit)
            } else if chars[at].is_ascii_lowercase() {
                run(at, char::is_ascii_lowercase)
            } else {
                let upper = run(at, char::is_ascii_uppercase);
                let lower_follows = chars.get(upper).is_some_and(char::is_ascii_lowercase);
                match (lower_follows, upper - at) {
                    // `HTTPServer`: the last capital starts the next hump.
                    (true, n) if n >= 2 => upper - 1,
                    (true, _) => run(upper, char::is_ascii_lowercase),
                    (false, _) => upper,
                }
            };
            out.push(chars[at..end].iter().collect::<String>().to_lowercase());
            at = end;
        }
    }
    out
}

fn means(parts: &[String], spoken: &[&str]) -> bool {
    if parts.is_empty() || parts.iter().map(String::len).sum::<usize>() < 3 {
        return false;
    }
    (0..spoken.len()).any(|start| fits(parts, spoken, start))
}

/// Whether `parts` can be read from `spoken[start..]`, one group of words per part.
fn fits(parts: &[String], spoken: &[&str], start: usize) -> bool {
    let Some((part, rest)) = parts.split_first() else {
        return true;
    };
    (1..=GROUP)
        .take_while(|n| start + n <= spoken.len())
        .any(|n| like(part, &spoken[start..start + n]) && fits(rest, spoken, start + n))
}

fn like(part: &str, group: &[&str]) -> bool {
    let joined = group.concat();
    part == joined
        || (group.len() == 1 && abbreviates(part, &joined))
        || (part.len() > SHORT_PART && ratio(part, &joined) >= PART_RATIO)
}

/// `part` is a short abbreviation of `word`: same first letter, its letters in order.
fn abbreviates(part: &str, word: &str) -> bool {
    if part.len() > ABBREVIATION || part.len() >= word.len() || part[..1] != word[..1] {
        return false;
    }
    let mut letters = word.chars();
    part.chars().all(|c| letters.any(|w| w == c))
}

/// Python's `difflib.SequenceMatcher(None, a, b).ratio()`, which the thresholds
/// were measured with: twice the matched characters over both lengths. Matches are
/// found as difflib does, longest block first, then recursively on either side.
fn ratio(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut matched = 0;
    let mut ranges = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = ranges.pop() {
        let (i, j, size) = longest_match(&a, &b, alo, ahi, blo, bhi);
        if size > 0 {
            matched += size;
            ranges.push((alo, i, blo, j));
            ranges.push((i + size, ahi, j + size, bhi));
        }
    }
    let total = a.len() + b.len();
    if total == 0 {
        1.
    } else {
        2. * matched as f64 / total as f64
    }
}

/// difflib's `find_longest_match` with no junk: the longest common block, the
/// earliest in `a` on a tie, then the earliest in `b`.
fn longest_match(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut best_i, mut best_j, mut best) = (alo, blo, 0);
    let mut lengths: HashMap<usize, usize> = HashMap::new();
    for (i, &letter) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut next = HashMap::new();
        for j in (blo..bhi).filter(|&j| b[j] == letter) {
            let k = j.checked_sub(1).and_then(|p| lengths.get(&p)).unwrap_or(&0) + 1;
            next.insert(j, k);
            if k > best {
                (best_i, best_j, best) = (i + 1 - k, j + 1 - k, k);
            }
        }
        lengths = next;
    }
    (best_i, best_j, best)
}

/// A transcript word and a screen word that sounds like it (D14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub heard: String,
    pub screen: String,
}

/// Words this short gave accidental pairs (run and rune, kid and kit, F13).
const SOUND_MIN: usize = 4;
/// A screen word must appear this often: once cut accidental pairs from 4 to 1 (F13).
const SOUND_REPEATS: usize = 2;
/// Transcript words never offered: they carry the sentence, not its subject.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "you", "that", "this", "with", "was", "are", "but", "not", "have", "has",
    "had", "can", "will", "would", "could", "should", "what", "when", "where", "which", "who",
    "why", "how", "then", "than", "them", "they", "their", "there", "here", "from", "into", "onto",
    "about", "just", "also", "only", "very", "some", "any", "all", "each", "more", "most", "such",
    "our", "your", "its",
];

/// Pairs each transcript word with the screen words that sound like it, in
/// transcript order. Screen words are taken from `screen_text`, the read's OCR text.
pub fn sound_pairs(screen_text: &str, transcript: &str) -> Vec<Pair> {
    let heard: Vec<String> = letter_runs(transcript).collect();
    let heard_set: HashSet<&str> = heard.iter().map(String::as_str).collect();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for word in letter_runs(screen_text) {
        *counts.entry(word).or_default() += 1;
    }
    let mut by_key: HashMap<String, BTreeSet<&str>> = HashMap::new();
    for (word, &count) in &counts {
        if word.len() >= SOUND_MIN && count >= SOUND_REPEATS && !heard_set.contains(word.as_str()) {
            by_key.entry(sound_key(word)).or_default().insert(word);
        }
    }
    let mut seen = HashSet::new();
    let mut pairs = vec![];
    for word in &heard {
        if word.len() < SOUND_MIN || STOPWORDS.contains(&word.as_str()) || !seen.insert(word) {
            continue;
        }
        for screen in by_key.get(&sound_key(word)).into_iter().flatten() {
            if screen != word {
                pairs.push(Pair {
                    heard: word.clone(),
                    screen: screen.to_string(),
                });
            }
        }
    }
    pairs
}

/// Runs of ASCII letters, lowercase.
fn letter_runs(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
}

/// A small Metaphone-like code: a consonant skeleton, so scale and skill share SKL.
fn sound_key(word: &str) -> String {
    let mut w = word.to_ascii_lowercase();
    for (from, to) in [
        ("sch", "sk"),
        ("ph", "f"),
        ("ck", "k"),
        ("tch", "x"),
        ("sh", "x"),
        ("th", "0"),
        ("wr", "r"),
        ("kn", "n"),
        ("gh", ""),
        ("dg", "j"),
        ("ch", "x"),
    ] {
        w = w.replace(from, to);
    }
    let soft = |next: Option<&char>| next.is_some_and(|c| "eiy".contains(*c));
    let chars: Vec<char> = w.chars().collect();
    let w: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match c {
            'c' if soft(chars.get(i + 1)) => 's',
            'c' | 'q' => 'k',
            other => other,
        })
        .collect();
    let chars: Vec<char> = w.chars().collect();
    let w: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match c {
            'g' if soft(chars.get(i + 1)) => 'j',
            'z' => 's',
            'd' => 't',
            other => other,
        })
        .collect();
    let mut letters = w.chars();
    let head = match letters.next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') | None => 'A',
        Some(c) => c,
    };
    let mut key = String::from(head);
    for c in letters.filter(|c| !"aeiouhwy".contains(*c)) {
        if !key.ends_with(c) {
            key.push(c);
        }
    }
    key.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (heard, screen) pairs.
    type Pairs = &'static [(&'static str, &'static str)];

    /// The ordinary English the tables use; `/usr/share/dict/words` in Lens.
    const ORDINARY: &[&str] = &[
        "the", "and", "pass", "now", "edit", "open", "shell", "run", "test", "plan", "change",
        "window", "read", "word", "sigma", "state", "seed", "plot", "garden", "end", "rock",
        "roll", "keep", "this", "see",
    ];

    fn known(word: &str) -> bool {
        ORDINARY.contains(&word)
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn pick_keeps_each_rule_of_the_filter() {
        let long_path = format!("src/{}.rs", "a".repeat(41)); // 48 characters
        let longer_path = format!("src/{}.rs", "a".repeat(42)); // 49
        #[rustfmt::skip]
        let rows: &[(&str, &str, &[&str])] = &[
            ("flags", "pass --token-stdin and -v2 now", &["--token-stdin", "-v2"]),
            ("paths", "edit src/lens/dictation.rs and ~/projects", &["src/lens/dictation.rs", "~/projects"]),
            ("camelCase and PascalCase", "idleCompaction OpenShell HTTPServer", &["idleCompaction", "OpenShell", "HTTPServer"]),
            ("snake_case", "recording_duration_ms _private", &["recording_duration_ms"]),
            ("dotted names", "dictation.rs CLAUDE.md the end.", &["dictation.rs", "CLAUDE.md"]),
            ("letters with digits", "t-42 c7f1 gpt-6 42 a1", &["t-42", "c7f1", "gpt-6"]),
            ("unknown words", "the attn stdin garden zq", &["attn", "stdin"]),
            ("a word with an unknown part", "sigma-env open-shell", &["sigma-env"]),
            ("plural stems", "seeds plots windows attns", &["attns"]),
            ("possessives", "Johannes's attn’s", &["Johannes", "attn"]),
            ("contractions", "don't won’t rock'n'roll", &[]),
            ("URLs", "https://example.com/a1 www.example.com/x WWW.example.com", &[]),
            ("edge punctuation", "(src/lens), `idleCompaction` “stdin”! attn:", &["src/lens", "idleCompaction", "stdin", "attn"]),
            ("repeats", "attn attn (attn) attn.", &["attn"]),
            ("reading order", "zeta-env alpha_beta attn", &["zeta-env", "alpha_beta", "attn"]),
            ("ordinary words", "open the plan and run this test", &[]),
        ];
        for (rule, text, expected) in rows {
            assert_eq!(pick(text, known), strings(expected), "{rule}");
        }
        assert_eq!(
            pick(&long_path, known),
            [long_path.as_str()],
            "48 characters"
        );
        assert!(pick(&longer_path, known).is_empty(), "over 48 characters");
    }

    #[test]
    fn pick_drops_secret_shaped_tokens() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, bool)] = &[
            ("an access key", "AKIAIOSFODNN7EXAMPLE", false),
            ("a hash", "a1b2c3d4e5f6a7b8c9d0e1", false),
            ("a UUID", "123e4567-e89b-12d3-a456-426614174000", false),
            ("a /-separated key", "AKIAEXAMPLE/abc1def2ghi3jkl4mno5pqr6/x", false),
            ("19 characters with a digit", "abcdefghijklmnopqr1", true),
            ("a long segment without a digit", "src/lens/screen_words_matching_table.rs", true),
        ];
        for (shape, token, kept) in rows {
            assert_eq!(!pick(token, known).is_empty(), *kept, "{shape}");
        }
    }

    #[test]
    fn picked_words_hold_no_whitespace() {
        let screen = include_str!("../../tests/fixtures/screen-words/attn-screen.txt");
        let words = pick(screen, known);
        assert!(!words.is_empty());
        assert!(words.iter().all(|w| !w.contains(char::is_whitespace)));
    }

    #[test]
    fn parts_split_at_separators_humps_and_digits() {
        #[rustfmt::skip]
        let rows: &[(&str, &[&str])] = &[
            ("src/lens/dictation.rs", &["src", "lens", "dictation", "rs"]),
            ("idleCompaction", &["idle", "compaction"]),
            ("OpenShell", &["open", "shell"]),
            ("HTTPServer", &["http", "server"]),
            ("getURL", &["get", "url"]),
            ("ABCdef", &["ab", "cdef"]),
            ("v2Beta", &["v", "2", "beta"]),
            ("t-42", &["t", "42"]),
            ("sigma-env", &["sigma", "env"]),
        ];
        for (entry, expected) in rows {
            assert_eq!(parts(entry), strings(expected), "{entry}");
        }
    }

    #[test]
    fn matching_keeps_each_rule_of_the_match() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, &str, bool)] = &[
            ("parts in order", "idleCompaction", "what does idle compaction do", true),
            ("parts out of order", "idleCompaction", "compaction when idle", false),
            ("spoken punctuation", "src/lens/dictation.rs", "source slash lens slash dictation dot rs", true),
            ("a part from joined words", "Forgejo", "the forge jo migration", true),
            ("three words for a part", "abcd", "ab c d", true),
            ("four words for a part", "abcd", "a b c d", false),
            ("nothing left over", "ScopeTestEnvironment", "the test environment is broken", false),
            ("an abbreviation", "cfg", "the config", true),
            ("an abbreviation's letters in order", "srv", "the source", false),
            ("abbreviations of 3 letters at most", "conf", "the configuration", false),
            ("a long part close enough", "Johannes", "ask yohannes", true),
            ("a long part not close enough", "parse_header", "parses the header", false),
            ("short parts exactly", "wnen", "when", false),
            ("3 letters exactly", "tne", "then", false),
            ("digits", "t-42", "close t 42", true),
            ("fewer than 3 characters", "ab", "ab", false),
            ("fewer than 3 characters with a digit", "a1", "a 1", false),
            ("transcript casing and punctuation", "OpenShell", "Open, Shell!", true),
        ];
        for (rule, entry, transcript, kept) in rows {
            let entries = strings(&[entry]);
            assert_eq!(!matching(&entries, transcript).is_empty(), *kept, "{rule}");
        }
        let entries = strings(&["stdin", "attn", "stdin", "OpenShell"]);
        assert_eq!(
            matching(&entries, "open shell then attn reads stdin"),
            strings(&["stdin", "attn", "OpenShell"]),
            "repeats dropped, reading order kept"
        );
    }

    #[test]
    fn ratio_matches_difflib() {
        // Values from Python's difflib.SequenceMatcher(None, a, b).ratio().
        for (a, b, expected) in [
            ("johannes", "yohannes", 0.875),
            ("header", "theheader", 0.8),
            ("abc", "abc", 1.),
            ("abc", "xyz", 0.),
        ] {
            assert!((ratio(a, b) - expected).abs() < 1e-9, "{a} {b}");
        }
    }

    /// The prototype's corpus: (transcript, the case's screen entries, what matching
    /// kept with the whole screen). Spelling cases keep their identifier; near-miss
    /// cases drop theirs, except `DeployWorker` for "deploy worked" and
    /// `SessionStore` for "session store", where either spelling reads right.
    #[rustfmt::skip]
    const CORPUS: &[(&str, &[&str], &[&str])] = &[
        ("um so i think we should change the plan for the family feature and uh ship it next week", &[], &["Foature", "changed"]),
        ("can you check the logs and tell me why the session keeps dropping", &["Logs.tsx", "session_id", "SessionStore"], &[]),
        ("send it to ana no wait to bruno", &["AnaClient"], &[]),
        ("i'm not sure maybe we should probably wait until monday", &[], &[]),
        ("first we build it then we test it and then we ship it", &["build.rs", "test_it"], &["test_it"]),
        ("thanks that looks good to me please merge it", &["merge_base"], &[]),
        ("the plugin runtime is slow when the inbox is full", &[], &["inbox", "plugin", "runtime"]),
        ("uh open the settings and turn off the option for notifications", &["notifications_enabled", "SettingsWindow"], &["settinas"]),
        ("okay the cloud deploy failed again i guess we need to look at the attention queue", &["Claude", "attn", "deploy-prod"], &[]),
        ("write a function that parses the header and returns an error", &["parse_header", "ParserError"], &[]),
        ("the test environment is broken so use staging", &["ScopeTestEnvironment", "StagingEnv"], &[]),
        ("um fix the bug in the parser and then run the tests", &["ParserError", "parse_header", "test_parser_roundtrip", "cargo-nextest"], &[]),
        ("update the config and then run the tests again", &["ConfigLoader", "config.toml", "tests/editor.rs", "run_tests"], &[]),
        ("i'm not sure the deploy worked maybe check it", &["deploy-prod", "DeployWorker", "deployed_at"], &["DeployWorker"]),
        ("the session store loses data when the app restarts", &["SessionStore", "session_store.rs"], &["SessionStore"]),
        ("can you rename scope test environment so it reads better", &["ScopeTestEnvironment", "StagingEnv"], &["ScopeTestEnvironment"]),
        ("the bug is in source slash lens slash dictation dot rs i think", &["src/lens/dictation.rs", "src/lens/voice.rs"], &["src/lens/dictation.rs"]),
        ("pass dash dash token stdin before the transcript", &["--token-stdin", "--duration-ms"], &["--token-stdin", "stdin"]),
        ("ask yohannes whether the forge jo migration is done", &["Johannes", "Forgejo", "Ingrid"], &["Johannes", "Forgejo"]),
        ("set the sigma env variable on the new machine", &["sigma-env", "SIGMA_HOME"], &["sigma-env"]),
        ("the field is called recording duration ms", &["recording_duration_ms", "raw_text"], &["recording_duration_ms"]),
        ("what does idle compaction do", &["idleCompaction", "compactAfterIdle"], &["idleCompaction"]),
        ("rename scope test environment before friday", &["ScopeTestEnvironment"], &["ScopeTestEnvironment"]),
        ("open open shell and check the workspaces", &["OpenShell"], &["OpenShell", "workspaces"]),
    ];

    #[test]
    fn matching_keeps_what_the_prototype_kept() {
        // The prototype's screen beyond each case's own entries (`corpus.py` on
        // s-vs128z): OCR misreads, ordinary words and identifiers, then `big.json`.
        let rest = include_str!("../../tests/fixtures/screen-words/noise.txt")
            .lines()
            .chain(include_str!("../../tests/fixtures/screen-words/entries.txt").lines());
        for (transcript, own, kept) in CORPUS {
            let screen: Vec<String> = own
                .iter()
                .copied()
                .chain(rest.clone())
                .map(String::from)
                .collect();
            assert_eq!(matching(&screen, transcript), strings(kept), "{transcript}");
        }
    }

    #[test]
    fn sound_key_applies_each_rewrite() {
        #[rustfmt::skip]
        let rows = [
            ("scale", "SKL"), ("skill", "SKL"), ("school", "SKL"), ("cash", "KX"), ("cache", "KX"),
            ("seat", "ST"), ("seed", "ST"), ("ruin", "RN"), ("rune", "RN"), ("phone", "FN"),
            ("knight", "NT"), ("write", "RT"), ("judge", "J"), ("dodge", "TJ"), ("city", "ST"),
            ("science", "SNS"), ("quiz", "KS"), ("match", "MX"), ("chip", "XP"), ("thin", "0N"),
            ("apple", "APL"), ("white", "WT"), ("gnaw", "GN"),
        ];
        for (word, key) in rows {
            assert_eq!(sound_key(word), key, "{word}");
        }
    }

    fn pairs(screen: &str, transcript: &str) -> Vec<(String, String)> {
        sound_pairs(screen, transcript)
            .into_iter()
            .map(|p| (p.heard, p.screen))
            .collect()
    }

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(h, s)| (h.to_string(), s.to_string()))
            .collect()
    }

    #[test]
    fn sound_pairs_follow_each_rule() {
        #[rustfmt::skip]
        let rows: &[(&str, &str, &str, Pairs)] = &[
            ("a pair", "skill skill", "load the scale", &[("scale", "skill")]),
            ("3-letter words", "rune rune kit kit", "run it, my kid", &[]),
            ("once on screen", "skill", "load the scale", &[]),
            ("already in the transcript", "skill skill", "scale the skill", &[]),
            ("stopwords", "white white", "what now", &[]),
            ("not a stopword", "white white", "wait now", &[("wait", "white")]),
            ("screen casing", "Skill SKILL", "the Scale", &[("scale", "skill")]),
            ("transcript order", "skill skill rune rune", "ruin the scale", &[("ruin", "rune"), ("scale", "skill")]),
            ("a heard word once", "skill skill", "scale and scale", &[("scale", "skill")]),
            ("screen words sorted", "seed seed seat set city city", "the seat", &[("seat", "city"), ("seat", "seed")]),
        ];
        for (rule, screen, transcript, expected) in rows {
            assert_eq!(pairs(screen, transcript), owned(expected), "{rule}");
        }
    }

    #[test]
    fn sound_pairs_offer_what_the_prototype_offered() {
        let screen = include_str!("../../tests/fixtures/screen-words/attn-screen.txt");
        // `mishear.py` on s-vs128z: what each transcript offers with this screen.
        #[rustfmt::skip]
        let rows: &[(&str, Pairs)] = &[
            ("load the attn workflow scale before you start planning", &[("scale", "skill")]),
            ("which scale should the builder read first", &[("scale", "skill")]),
            ("the delegation scale says to ask before dispatching", &[("scale", "skill")]),
            ("the session cash keeps stale entries after a restart", &[("cash", "cache")]),
            ("split the pain and focus the terminal", &[("pain", "pane")]),
            ("plant a seat under the plot and tend it", &[("seat", "seed")]),
            ("run the tests in the ruin fork again", &[("ruin", "rune")]),
            ("scale the image down to half before you send it", &[("scale", "skill")]),
            ("the font scale is too small on the retina display", &[("scale", "skill")]),
            ("we need to scale the workers to three replicas", &[("scale", "skill")]),
            ("i paid in cash for the parking", &[("cash", "cache")]),
            ("this bug is a real pain to reproduce", &[("pain", "pane")]),
            ("save me a seat at the meeting", &[("seat", "seed")]),
            ("my kid starts school next week", &[("school", "skill")]),
            ("invalidate the cash after each deploy", &[("cash", "cache")]),
            ("close the left pain and keep the terminal", &[("pain", "pane")]),
            ("harvest the seat when the checks pass", &[("seat", "seed")]),
            ("the scale file has the rules for reviews", &[("scale", "skill")]),
            ("bump the ruin pin to the latest main", &[("ruin", "rune")]),
            ("my back is in pain again today", &[("pain", "pane")]),
            ("do we have the cash flow numbers for october", &[("cash", "cache")]),
            ("fasten your seat belt before we leave", &[("seat", "seed")]),
            ("the old castle is a ruin now", &[("ruin", "rune")]),
            ("let's scale back the scope a bit", &[("scale", "skill")]),
            ("pick the kids up after school", &[("school", "skill")]),
            ("write the report and send it to me", &[("write", "read")]),
        ];
        for (transcript, expected) in rows {
            assert_eq!(pairs(screen, transcript), owned(expected), "{transcript}");
        }
        // The earlier corpus, where nothing was misheard: one accidental pair in 24.
        let accidental: Vec<_> = CORPUS
            .iter()
            .flat_map(|(transcript, ..)| pairs(screen, transcript))
            .collect();
        assert_eq!(accidental, owned(&[("write", "read")]));
    }
}
