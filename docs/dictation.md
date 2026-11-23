# Dictation

With no capture open, the voice key, **right ⌥**, dictates into the app in front:
a browser field, a terminal, a text editor. Tap it to start and tap again to
stop, or hold it and talk until you let go.

## What you see

A pill sits at the bottom of the display with the pointer, above the Dock. It
never takes the keyboard or a click, so the app in front keeps its cursor.

1. While you talk, the pill shows the words as each phrase is transcribed.
2. When you stop, a model tidies the text (filler words, punctuation, casing)
   through attn. The pill shows the raw text and **Refining… · ↩ paste as is ·
   esc cancel**.
3. The text pastes once at the cursor, and the pill says how it went for 3 s.

While the pill says Refining:

- **↩** pastes the raw text at once. attn's answer, if it comes later, is dropped.
- **Esc** pastes nothing.

Lens takes ↩ and Esc from the app in front only then; at any other time they
reach the app as usual. While the area picker shows, ↩ and Esc are the picker's
(the last area, and cancel); a cancelled pick gives them back to the pill.

## How the text is refined

Lens runs `attn model call` with its paired token, its own instructions
(`src/lens/refiner.md`) and the transcript as JSON (`{"transcript": ...}`, with
the fields [Words on screen](#words-on-screen) adds). It
asks for attn's standard tier and names no harness, so the harness is the one
attn's `model_calls.agent` setting names, or the harness for new sessions. The
limit is 15 s plus a quarter of the recording. attn keeps one model process
started ahead while senders are in use, so a call waits for the model alone.
attn's `attn model log` lists each call, with no text.

A change to the instructions is judged by the refinement corpus, synthetic
dictations each with the phrases that must survive, run against the real model
through a non-production attn:

```sh
eval "$(attn instance-env <name>)"
cargo run --example refine-probe -- --rounds 3 --out refine.jsonl
```

`LENS_PROBE_TOKEN` gives it a paired token; without one it pairs as "Attn Lens
refine probe" and prints the token. attn refuses a pairing from inside an attn
agent session, so pair it from a terminal of your own. It exits 1 when a
refinement lost or changed what was said. The screen-words cases end the run with
a table by kind: cleaned, kept (the spelling or the fix) and replaced (a word the
speaker meant, swapped for a screen word); any of them missing, or any word
replaced, also exits 1.

## Words on screen

While **Use words on screen** is on (Settings, ⌘,; on by default), each dictation
reads the window in front as it starts: one still of the topmost window that is
not Lens's, and text recognition of it on this Mac (Vision, the same as Copy as
OCR). Each other window that comes in front before voice stops is read the same
way as it comes, once per dictation: switching back to a window already read reads
nothing. From the text of every read, Lens keeps two lists, when the refinement
starts:

- `screen_words`: the names and identifiers on screen that the transcript could
  mean, so "scope test environment" comes out as `ScopeTestEnvironment` and
  "source slash lens" as `src/lens`. Only entries whose parts line up with words
  you said go; the rest of the screen never leaves the Mac.
- `maybe_misheard`: pairs of a word you said and a word that sounds like it and
  shows twice on screen, such as "scale" and "skill". The refiner swaps one only
  when the sentence makes little sense as heard.

Both go in the input of the same `attn model call`, each only when it holds
something. The refinement never waits for a read: a read that is not back when
the refinement starts is dropped, and so are the reads of a dictation that is
cancelled or turned into a capture. A capture never refines, so it sends no screen words.
Without Screen Recording, or when the read fails, the dictation runs as it would
with the setting off, and no permission guide shows; the Settings row says when
Screen Recording is not allowed. With the setting off, Lens takes no still and
runs no OCR.

## When the text is not refined

The raw text pastes, and the pill says why, when:

- the model call fails, or attn refuses it or Lens's token. A refused token also
  unpairs Lens, as a send does; pair again from the bar.
- attn is not installed, or cannot be run.
- attn does not answer within the call's limit (15 s plus a quarter of the
  recording) and 2 s more. Lens then stops it.

If nothing was heard, nothing pastes and the pill says **Didn't catch anything**.

Without Accessibility, Lens cannot post ⌘V: the text goes to the clipboard and the
pill says **Copied — allow Accessibility to paste**.

## The clipboard

The paste goes through the clipboard: Lens saves what it holds, writes the text
marked so clipboard managers skip it, posts ⌘V, and puts the saved clipboard back
half a second later, unless something else wrote it meanwhile. In Chromium 155 and
156, which drop a ⌘V that races their clipboard, Lens types the text instead.

## Copy Last Dictation

The Dock menu's **Copy Last Dictation** puts the last dictation's text on the
clipboard, refined or raw, cancelled ones included, in case a paste landed nowhere.
It lasts until Lens quits.

## From dictation to a capture

Press the capture key, **right ⌘**, while dictating, and pick an area. The capture
takes the dictation over: the words so far are its first phrases, its timeline
starts where you began talking, voice keeps running, and nothing pastes. If you
were already waiting for attn, its refinement stops and the raw phrases go to the
capture. Cancel the pick and the dictation carries on.

## Checking it

`LENS_DICTATION_LOG=1` prints each step with the time since launch: the voice key,
the pill showing, voice reporting, each screen read's time and word count (or its
error) with its index in the dictation, from 0, how many screen words matched and
how many sound pairs were offered from how many reads, attn
asked and answering, the paste. It never prints the words themselves. The
answer's line gives the time from the end of voice to the refined text, attn's
part of it, and the harness, model and model time.
