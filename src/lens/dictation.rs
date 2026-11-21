//! Dictation: the voice key with no capture open. The words show in the pill
//! while you speak; when voice stops, a model refines them through attn (`attn
//! model call`, with Lens's own instructions from `refiner`) and the result pastes once at the cursor of the app in front (D5, D6 on the plan).
//!
//! While the refinement runs, ↩ pastes the raw text at once and Esc pastes
//! nothing. A failed or late refinement, or no attn to ask, pastes the raw text
//! and the pill says why. Each dictation has a run id, so a result that arrives
//! for one already settled is dropped: a dictation pastes at most once.
//!
//! The capture key during a dictation hands it over (D9): the capture that
//! opens takes the running voice and the words so far, and nothing pastes.
//!
//! While "Use words on screen" is on, a dictation reads the front window as it
//! starts, and the words the transcript could mean go with it to the refiner
//! (`s-0ww1ak`). Refinement never waits for the read: one not back by then is
//! dropped, as is the read of a dictation that ended any other way.
use super::{
    AttnStatus, Lens, State, attn, refiner, screen_words,
    session::{CaptureVoice, DisplayFrame, HostTime, RunningVoice, Segment, VoiceOwner},
};
use crate::platform_port::{PillKey, Press, Sink, VoiceEvent, VoiceSource, VoiceStatus};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

/// Lens stops waiting this long after attn's own limit, so attn's error, which
/// names its limit, arrives first when attn is alive (Design › Rules).
const PAST_ATTNS_LIMIT: Duration = Duration::from_secs(2);

/// How long the pill shows how a dictation ended: a feel choice, judged on the
/// user's Mac (C64).
const OUTCOME_STAYS: Duration = Duration::from_secs(3);

pub enum Dictation {
    Recording {
        voice: RunningVoice,
        segments: Vec<Segment>,
        started: HostTime,
        screen: ScreenRead,
    },
    /// Voice stopped; waiting for the phrase in progress.
    Finishing {
        run: u64,
        owner: Arc<Mutex<VoiceOwner>>,
        segments: Vec<Segment>,
        started: HostTime,
        stopped: HostTime,
        screen: ScreenRead,
    },
    /// Waiting for attn since `asked`; voice stopped at `stopped`. `call` is
    /// `None` until the command has started.
    Refining {
        run: u64,
        segments: Vec<Segment>,
        started: HostTime,
        call: Option<Arc<attn::ModelCall>>,
        stopped: HostTime,
        asked: HostTime,
        deadline: HostTime,
    },
    /// Over. `until` is `None` while the paste runs, then when the pill hides.
    Settled {
        run: u64,
        text: String,
        how: Settled,
        until: Option<HostTime>,
    },
}

/// The screen read of a dictation, until refinement takes it.
pub enum ScreenRead {
    /// "Use words on screen" was off when the dictation started.
    Off,
    /// Still reading.
    Reading,
    /// The front window's text.
    Read(String),
    /// The read failed; it logged why.
    Failed,
}

/// How a dictation ended, as the pill tells it.
#[derive(Clone, Debug, PartialEq)]
pub enum Settled {
    /// attn's refinement pasted.
    Refined,
    /// The raw text pasted: on ↩ when `None`, otherwise why it was not refined.
    Raw(Option<String>),
    /// Esc: nothing pasted.
    Cancelled,
    /// No words: nothing pasted.
    Empty,
    /// Voice could not start or stopped with nothing said.
    VoiceFailed(String),
    /// Without Accessibility the text went to the clipboard instead.
    Copied,
    NotPasted(String),
    /// From the Dock menu.
    CopiedLast,
    NothingToCopy,
}

impl Settled {
    /// Whether the text goes to the app in front.
    fn pastes(&self) -> bool {
        matches!(self, Settled::Refined | Settled::Raw(_))
    }

    pub fn note(&self) -> String {
        match self {
            Settled::Refined => "Pasted".into(),
            Settled::Raw(None) => "Pasted as is".into(),
            Settled::Raw(Some(why)) => format!("Not refined: {why} · pasted as is"),
            Settled::Cancelled => "Cancelled · nothing pasted".into(),
            Settled::Empty => "Didn't catch anything".into(),
            Settled::VoiceFailed(error) => error.clone(),
            Settled::Copied => "Copied — allow Accessibility to paste".into(),
            Settled::NotPasted(error) => {
                format!("Not pasted: {error} · Copy Last Dictation is in the Dock menu")
            }
            Settled::CopiedLast => "Copied the last dictation".into(),
            Settled::NothingToCopy => "No dictation yet".into(),
        }
    }
}

/// What the pill shows.
#[derive(Clone, Debug, PartialEq)]
pub enum DictationSummary {
    Recording {
        status: Option<VoiceStatus>,
        words: String,
    },
    Finishing {
        words: String,
    },
    Refining {
        words: String,
    },
    Settled {
        text: String,
        how: Settled,
        pasting: bool,
    },
}

impl Dictation {
    pub fn summary(&self) -> DictationSummary {
        match self {
            Dictation::Recording {
                voice, segments, ..
            } => DictationSummary::Recording {
                status: voice.status,
                words: words(segments),
            },
            Dictation::Finishing { segments, .. } => DictationSummary::Finishing {
                words: words(segments),
            },
            Dictation::Refining { segments, .. } => DictationSummary::Refining {
                words: words(segments),
            },
            Dictation::Settled {
                text, how, until, ..
            } => DictationSummary::Settled {
                text: text.clone(),
                how: how.clone(),
                pasting: until.is_none(),
            },
        }
    }
}

/// The raw text: every phrase, in order.
fn words(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a capture takes from a dictation it starts during (D9).
pub struct HandOver {
    pub segments: Vec<Segment>,
    /// When the dictation started: the capture's timeline starts there too.
    pub started: HostTime,
    pub voice: CaptureVoice,
}

impl Lens {
    /// The voice key with no capture open: a tap toggles dictation, a hold talks
    /// until release.
    pub(super) fn dictation(self: &Arc<Self>, press: Press) {
        let recording = matches!(
            self.state.lock().unwrap().dictation,
            Some(Dictation::Recording { .. })
        );
        match press {
            Press::Tap => self.toggle_dictation(),
            Press::HoldStart if !recording => self.toggle_dictation(),
            Press::HoldEnd if recording => self.toggle_dictation(),
            Press::HoldStart | Press::HoldEnd => {}
        }
    }

    fn toggle_dictation(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap();
        match state.dictation.take() {
            Some(Dictation::Recording {
                voice,
                segments,
                started,
                screen,
            }) => {
                let RunningVoice {
                    run, handle, owner, ..
                } = voice;
                state.dictation = Some(Dictation::Finishing {
                    run,
                    owner: owner.clone(),
                    segments,
                    started,
                    stopped: self.port.now(),
                    screen,
                });
                self.release(state);
                let lens = self.clone();
                self.port.run(Box::new(move || {
                    let finished = handle.finish();
                    lens.dictation_finished(run, &owner, finished);
                }));
                return;
            }
            // The speech helper holds one recording at a time.
            finishing @ Some(Dictation::Finishing { .. }) => {
                state.dictation = finishing;
                return;
            }
            // A new dictation: the one waiting for attn pastes as it is.
            Some(Dictation::Refining {
                run,
                segments,
                call,
                ..
            }) => {
                if let Some(call) = call {
                    call.stop();
                }
                state = self.settle(state, run, words(&segments), Settled::Raw(None));
            }
            Some(Dictation::Settled { .. }) | None => {}
        }
        self.start_dictation(state);
    }

    /// The display with the pointer, where the pill shows.
    fn pointer_display(&self) -> Option<DisplayFrame> {
        let pointer = self.port.pointer_display().ok()?.display;
        self.port.displays().into_iter().find(|d| d.id == pointer)
    }

    fn start_dictation(self: &Arc<Self>, mut state: MutexGuard<'_, State>) {
        state.dictation_display = self.pointer_display();
        state.voice_run += 1;
        let run = state.voice_run;
        let started = self.port.now();
        let source = state
            .voice_wav
            .clone()
            .map_or(VoiceSource::Microphone, VoiceSource::Wav);
        let owner = Arc::new(Mutex::new(VoiceOwner::Dictation));
        let read = state.settings.screen_words;
        let screen = if read {
            ScreenRead::Reading
        } else {
            ScreenRead::Off
        };
        // `voice` reports from elsewhere, and the sink waits for this lock, so
        // the first report finds the dictation stored.
        state.dictation = Some(
            match self.port.voice(source, self.voice_sink(run, owner.clone())) {
                Ok(handle) => Dictation::Recording {
                    voice: RunningVoice {
                        run,
                        handle,
                        status: None,
                        owner,
                    },
                    segments: vec![],
                    started,
                    screen,
                },
                Err(error) => Dictation::Settled {
                    run,
                    text: String::new(),
                    how: Settled::VoiceFailed(error),
                    until: Some(started.after(OUTCOME_STAYS)),
                },
            },
        );
        let recording = matches!(state.dictation, Some(Dictation::Recording { .. }));
        dictation_log(format_args!("voice key: dictation {run} starts"));
        self.release(state);
        if read && recording {
            let lens = self.clone();
            self.port.run(Box::new(move || lens.read_screen(run)));
        }
    }

    /// Reads the front window for dictation `run`, and leaves the text on it if
    /// it is still recording or finishing. Logs the time and the size, never the words.
    fn read_screen(self: &Arc<Self>, run: u64) {
        let start = std::time::Instant::now();
        let read = self.port.screen_text();
        // Loads the word list here, off the path to refinement.
        self.port.ordinary_word("the");
        let ms = start.elapsed().as_millis();
        match &read {
            Ok(text) => dictation_log(format_args!(
                "dictation {run}: screen read in {ms} ms, {} words",
                text.split_whitespace().count()
            )),
            Err(error) => dictation_log(format_args!(
                "dictation {run}: screen read failed in {ms} ms: {error}"
            )),
        }
        let mut state = self.state.lock().unwrap();
        match &mut state.dictation {
            Some(
                Dictation::Recording {
                    voice: RunningVoice { run: r, .. },
                    screen,
                    ..
                }
                | Dictation::Finishing { run: r, screen, .. },
            ) if *r == run => {
                *screen = read.map_or(ScreenRead::Failed, ScreenRead::Read);
            }
            _ => dictation_log(format_args!(
                "dictation {run}: screen read came back after the dictation moved on, dropped"
            )),
        }
    }

    /// A report from the voice of dictation `run`.
    pub(super) fn dictation_voice_event(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        run: u64,
        event: VoiceEvent,
    ) {
        match (&mut state.dictation, event) {
            (
                Some(Dictation::Recording {
                    voice, segments, ..
                }),
                event,
            ) if voice.run == run => match event {
                VoiceEvent::Segment(segment) => segments.push(segment),
                VoiceEvent::Status(status) => {
                    if voice.status.is_none() {
                        dictation_log(format_args!("dictation {run}: voice reports {status:?}"));
                    }
                    voice.status = Some(status);
                }
                // Voice stopped on its own: what was said so far still counts.
                VoiceEvent::Failed(error) => {
                    let Some(Dictation::Recording {
                        voice,
                        segments,
                        started,
                        screen,
                    }) = state.dictation.take()
                    else {
                        unreachable!("matched above");
                    };
                    state.dictation = Some(Dictation::Finishing {
                        run,
                        owner: voice.owner.clone(),
                        segments,
                        started,
                        stopped: self.port.now(),
                        screen,
                    });
                    drop(state);
                    self.dictation_finished(run, &voice.owner, Err(error));
                    return;
                }
            },
            (
                Some(Dictation::Finishing {
                    run: finishing,
                    segments,
                    ..
                }),
                VoiceEvent::Segment(segment),
            ) if *finishing == run => segments.push(segment),
            _ => return,
        }
        self.release(state);
    }

    /// The finish of dictation `run`'s voice returned: its last phrase is in.
    /// Asks attn to refine the words, unless there are none.
    fn dictation_finished(
        self: &Arc<Self>,
        run: u64,
        owner: &Mutex<VoiceOwner>,
        finished: Result<(), String>,
    ) {
        let mut state = self.state.lock().unwrap();
        // Handed over while finishing: the capture's voice stops here.
        if let VoiceOwner::Capture(id) = *owner.lock().unwrap() {
            drop(state);
            return self.voice_stopped(id, run, finished);
        }
        let Some(Dictation::Finishing {
            run: finishing,
            segments,
            started,
            stopped,
            screen,
            ..
        }) = state
            .dictation
            .take_if(|d| matches!(d, Dictation::Finishing { run: r, .. } if *r == run))
        else {
            return;
        };
        debug_assert_eq!(finishing, run);
        let raw = words(&segments);
        if raw.is_empty() {
            let how = finished.err().map_or(Settled::Empty, Settled::VoiceFailed);
            state = self.settle(state, run, raw, how);
            return self.release(state);
        }
        let recording = stopped.since(started);
        let (words, pairs) = match screen {
            ScreenRead::Read(text) => {
                let picked = screen_words::pick(&text, |w| self.port.ordinary_word(w));
                let words = screen_words::matching(&picked, &raw);
                let pairs = screen_words::sound_pairs(&text, &raw);
                dictation_log(format_args!(
                    "dictation {run}: {} of {} screen words match, {} sound pairs",
                    words.len(),
                    picked.len(),
                    pairs.len()
                ));
                (words, pairs)
            }
            ScreenRead::Reading => {
                dictation_log(format_args!(
                    "dictation {run}: screen read not back, refining without it"
                ));
                Default::default()
            }
            ScreenRead::Off | ScreenRead::Failed => Default::default(),
        };
        let input = refiner::input(&raw, &words, &pairs);
        let request = refiner::request(&input, recording);
        let asked = self.port.now();
        state.dictation = Some(Dictation::Refining {
            run,
            segments,
            started,
            call: None,
            stopped,
            asked,
            deadline: asked.after(request.limit + PAST_ATTNS_LIMIT),
        });
        self.sync_pill_keys(&state);
        self.release(state);
        dictation_log(format_args!(
            "dictation {run}: {} ms recorded, asking attn",
            recording.as_millis()
        ));
        // The Keychain read may wait on the user, so it runs outside the lock.
        let started = self.port.token().ok_or(attn::Failure::NotPaired(
            "Attn Lens is not paired with attn".into(),
        ));
        let started = started.and_then(|token| {
            let bin = attn::locate()?;
            attn::ModelCall::start(&bin, &token, &request)
        });
        let mut state = self.state.lock().unwrap();
        let Some(Dictation::Refining { call, .. }) = state
            .dictation
            .as_mut()
            .filter(|d| matches!(d, Dictation::Refining { run: r, .. } if *r == run))
        else {
            // ↩ or Esc came first; dropping the command stops it.
            return;
        };
        match started {
            Ok(started) => {
                let started = Arc::new(started);
                *call = Some(started.clone());
                drop(state);
                let lens = self.clone();
                self.port.run(Box::new(move || {
                    let result = started.wait();
                    lens.refined(run, result);
                }));
            }
            Err(failure) => {
                state = self.not_refined(state, failure.clone());
                let why = failure_text(failure);
                state = self.settle_refining(state, run, |raw| (raw, Settled::Raw(Some(why))));
                self.release(state);
            }
        }
    }

    /// attn answered dictation `run`. A late answer finds it settled and is dropped.
    fn refined(self: &Arc<Self>, run: u64, result: Result<attn::Answer, attn::Failure>) {
        let mut state = self.state.lock().unwrap();
        let Some(Dictation::Refining { stopped, asked, .. }) = state
            .dictation
            .as_ref()
            .filter(|d| matches!(d, Dictation::Refining { run: r, .. } if *r == run))
        else {
            dictation_log(format_args!("dictation {run}: attn answered late, dropped"));
            return;
        };
        let now = self.port.now();
        dictation_log(format_args!(
            "dictation {run}: attn answered {} ms after voice stopped (attn {} ms, {})",
            now.since(*stopped).as_millis(),
            now.since(*asked).as_millis(),
            match &result {
                Ok(answer) => format!(
                    "{} {} {}",
                    answer.harness,
                    answer.model,
                    answer
                        .model_ms
                        .map_or("model time unreported".into(), |ms| format!(
                            "model {ms} ms"
                        ))
                ),
                Err(_) => "failed".into(),
            }
        ));
        state = match result.map(|answer| answer.text) {
            Ok(text) if !text.trim().is_empty() => {
                self.settle_refining(state, run, |_| (text, Settled::Refined))
            }
            Ok(_) => self.settle_refining(state, run, |raw| {
                (raw, Settled::Raw(Some("attn gave back no text".into())))
            }),
            Err(failure) => {
                let state = self.not_refined(state, failure.clone());
                let why = failure_text(failure);
                self.settle_refining(state, run, |raw| (raw, Settled::Raw(Some(why))))
            }
        };
        self.release(state);
    }

    /// A refusal of the token unpairs Lens, as a send's does.
    fn not_refined<'a>(
        &self,
        mut state: MutexGuard<'a, State>,
        failure: attn::Failure,
    ) -> MutexGuard<'a, State> {
        if let attn::Failure::NotPaired(text) = failure {
            self.port.forget_token();
            state.attn = AttnStatus::Unpaired(text);
        }
        state
    }

    /// Takes ↩ and Esc from the app in front exactly while a dictation waits for
    /// attn and no area picker owns the keyboard. Called under the state lock after
    /// every change to either, so interception never outlives what asked for it.
    pub(super) fn sync_pill_keys(&self, state: &State) {
        let refining = matches!(state.dictation, Some(Dictation::Refining { .. }));
        self.port.pill_keys(refining && state.picking.is_none());
    }

    /// ↩ or Esc while attn refines.
    pub(super) fn pill_key(self: &Arc<Self>, key: PillKey) {
        let state = self.state.lock().unwrap();
        let Some(Dictation::Refining { run, .. }) = state.dictation else {
            return;
        };
        let how = match key {
            PillKey::Enter => Settled::Raw(None),
            PillKey::Esc => Settled::Cancelled,
        };
        let state = self.settle_refining(state, run, |raw| (raw, how));
        self.release(state);
    }

    /// Settles dictation `run` if it is refining: stops attn, and `outcome` turns
    /// the raw text into what to paste and how it went.
    fn settle_refining<'a>(
        self: &Arc<Self>,
        mut state: MutexGuard<'a, State>,
        run: u64,
        outcome: impl FnOnce(String) -> (String, Settled),
    ) -> MutexGuard<'a, State> {
        let Some(Dictation::Refining { segments, call, .. }) = state
            .dictation
            .take_if(|d| matches!(d, Dictation::Refining { run: r, .. } if *r == run))
        else {
            return state;
        };
        if let Some(call) = call {
            call.stop();
        }
        let (text, how) = outcome(words(&segments));
        self.settle(state, run, text, how)
    }

    /// Ends dictation `run` with `text`, pasting it if `how` says so. The one
    /// place a dictation pastes: the state lock makes it once.
    fn settle<'a>(
        self: &Arc<Self>,
        mut state: MutexGuard<'a, State>,
        run: u64,
        text: String,
        how: Settled,
    ) -> MutexGuard<'a, State> {
        dictation_log(format_args!("dictation {run}: settled, {}", how.note()));
        if !text.is_empty() {
            state.last_dictation = Some(text.clone());
        }
        let pastes = how.pastes();
        let now = self.port.now();
        state.dictation = Some(Dictation::Settled {
            run,
            text: text.clone(),
            how,
            until: (!pastes).then(|| now.after(OUTCOME_STAYS)),
        });
        self.sync_pill_keys(&state);
        if pastes {
            state.paste = Some((run, text));
        }
        state
    }

    /// Lets go of the state, then tells the windows and starts the paste a
    /// settle queued, which takes the lock itself.
    fn release(self: &Arc<Self>, mut state: MutexGuard<'_, State>) {
        let paste = state.paste.take();
        drop(state);
        self.changed();
        if let Some((run, text)) = paste {
            let lens = self.clone();
            self.port.run(Box::new(move || lens.paste(run, text)));
        }
    }

    fn paste(self: &Arc<Self>, run: u64, text: String) {
        let how = if !self.port.accessibility() {
            match self.port.copy_text(&text) {
                Ok(()) => Some(Settled::Copied),
                Err(error) => Some(Settled::NotPasted(error)),
            }
        } else {
            self.port.paste(&text).err().map(Settled::NotPasted)
        };
        dictation_log(format_args!("dictation {run}: pasted"));
        let now = self.port.now();
        let mut state = self.state.lock().unwrap();
        if let Some(Dictation::Settled {
            run: settled,
            how: shown,
            until,
            ..
        }) = &mut state.dictation
            && *settled == run
        {
            if let Some(how) = how {
                *shown = how;
            }
            *until = Some(now.after(OUTCOME_STAYS));
        }
        self.release(state);
    }

    /// Stops a refinement past its deadline and hides an outcome shown long
    /// enough. Returns when to call again.
    pub fn advance_dictation(self: &Arc<Self>) -> Option<Duration> {
        let now = self.port.now();
        let mut state = self.state.lock().unwrap();
        match &state.dictation {
            Some(Dictation::Refining {
                run,
                asked,
                deadline,
                ..
            }) if now >= *deadline => {
                let run = *run;
                let why = format!(
                    "attn did not answer within {} s",
                    deadline.since(*asked).as_secs()
                );
                state = self.settle_refining(state, run, |raw| (raw, Settled::Raw(Some(why))));
                self.release(state);
                Some(OUTCOME_STAYS)
            }
            Some(Dictation::Refining { deadline, .. }) => Some(deadline.since(now)),
            Some(Dictation::Settled {
                until: Some(until), ..
            }) if now >= *until => {
                state.dictation = None;
                self.release(state);
                None
            }
            Some(Dictation::Settled {
                until: Some(until), ..
            }) => Some(until.since(now)),
            _ => None,
        }
    }

    /// Hands the dictation to capture `id`, which is starting (D9). Called
    /// with the state lock held, before the capture is stored.
    pub(super) fn hand_over(&self, state: &mut State, id: u64) -> Option<HandOver> {
        let handed = match state.dictation.take()? {
            // The screen read goes with the dictation: a capture never refines.
            Dictation::Recording {
                voice,
                segments,
                started,
                ..
            } => {
                *voice.owner.lock().unwrap() = VoiceOwner::Capture(id);
                HandOver {
                    segments,
                    started,
                    voice: CaptureVoice::Running(voice),
                }
            }
            Dictation::Finishing {
                run,
                owner,
                segments,
                started,
                ..
            } => {
                *owner.lock().unwrap() = VoiceOwner::Capture(id);
                HandOver {
                    segments,
                    started,
                    voice: CaptureVoice::Finishing {
                        run,
                        send_after: false,
                    },
                }
            }
            Dictation::Refining {
                segments,
                started,
                call,
                ..
            } => {
                if let Some(call) = call {
                    call.stop();
                }
                HandOver {
                    segments,
                    started,
                    voice: CaptureVoice::Off,
                }
            }
            settled @ Dictation::Settled { .. } => {
                state.dictation = Some(settled);
                return None;
            }
        };
        self.sync_pill_keys(state);
        dictation_log(format_args!("dictation handed to capture {id}"));
        Some(handed)
    }

    /// The pill's window came up, for the timings on the seed.
    pub fn pill_shown(&self) {
        dictation_log(format_args!("pill shows"));
    }

    /// Copy Last Dictation in the Dock menu (D8).
    pub fn copy_last_dictation(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap();
        if matches!(
            state.dictation,
            Some(
                Dictation::Recording { .. }
                    | Dictation::Finishing { .. }
                    | Dictation::Refining { .. }
            )
        ) {
            return;
        }
        let text = state.last_dictation.clone();
        let how = match &text {
            Some(text) => match self.port.copy_text(text) {
                Ok(()) => Settled::CopiedLast,
                Err(error) => Settled::NotPasted(error),
            },
            None => Settled::NothingToCopy,
        };
        state.voice_run += 1;
        let run = state.voice_run;
        state.dictation_display = self.pointer_display();
        state.dictation = Some(Dictation::Settled {
            run,
            text: text.unwrap_or_default(),
            how,
            until: Some(self.port.now().after(OUTCOME_STAYS)),
        });
        self.release(state);
    }

    /// Where voice `run` reports: the dictation, or the capture it was handed to.
    pub(super) fn voice_sink(
        self: &Arc<Self>,
        run: u64,
        owner: Arc<Mutex<VoiceOwner>>,
    ) -> Sink<VoiceEvent> {
        let lens = self.clone();
        Arc::new(move |event| {
            let state = lens.state.lock().unwrap();
            // Read under the state lock, which a hand-over holds while it switches.
            let owner = *owner.lock().unwrap();
            match owner {
                VoiceOwner::Dictation => lens.dictation_voice_event(state, run, event),
                VoiceOwner::Capture(id) => lens.capture_voice_event(state, id, run, event),
            }
        })
    }
}

fn failure_text(failure: attn::Failure) -> String {
    match failure {
        attn::Failure::NotPaired(text) | attn::Failure::Error(text) => text,
    }
}

/// `LENS_DICTATION_LOG=1` prints each step with the time since launch, for the
/// timings on the seed.
fn dictation_log(message: std::fmt::Arguments) {
    static START: std::sync::OnceLock<Option<std::time::Instant>> = std::sync::OnceLock::new();
    let start = START.get_or_init(|| {
        std::env::var_os("LENS_DICTATION_LOG")
            .is_some_and(|v| v == "1")
            .then(std::time::Instant::now)
    });
    if let Some(start) = start {
        // The wall clock too, so a check outside Lens can line its own times up.
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        eprintln!(
            "Attn Lens dictation {:>8.1} ms (unix {} ms): {message}",
            start.elapsed().as_secs_f64() * 1e3,
            wall.as_millis()
        );
    }
}
