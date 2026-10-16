// Fork: a dictation transcribed while you speak. Two things happen as the
// audio comes in, one at a time in the background:
//
// - Stretches are written for good ("committed") at pauses: the quietest
//   100 ms of the last few seconds, when it is clearly quieter than the speech
//   around it (relative to your own level, so a loud mic or a noisy room still
//   has pauses). Past MAX_UNCOMMITTED seconds without one, the quietest moment
//   is taken anyway, so letting go never leaves much to transcribe.
// - In between, the part not yet written is transcribed again about every
//   0.6 s as a preview, so the pill shows the words a couple at a time.
//
// When you let go only the audio after the last committed cut is left for the
// model, so a long dictation pastes about as fast as a short one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use super::pill;
use crate::speech;

const RATE: usize = 16_000;
/// The shortest stretch committed at a pause.
const MIN_CHUNK: usize = RATE * 3 / 2;
/// Past this much unwritten audio, a cut is made at the quietest moment.
const MAX_UNCOMMITTED: usize = RATE * 6;
/// Never cut in the last 0.4 s: a word may be under way.
const KEEP_TAIL: usize = RATE * 2 / 5;
/// How far back a pause is looked for.
const SEARCH: usize = RATE * 3;
const WINDOW: usize = RATE / 10;
/// A pause is this much quieter than the speech it sits in.
const PAUSE_RATIO: f64 = 0.12;
/// Below this it is a pause whatever the speech around it.
const SILENCE: f64 = 1e-4;
const EVERY: Duration = Duration::from_millis(300);
/// New audio needed before the preview is redone.
const PREVIEW_STEP: usize = RATE * 3 / 5;
const PREVIEW_MIN: usize = RATE * 4 / 5;

#[derive(Default)]
struct Progress {
    /// Samples written for good, from the start.
    upto: usize,
    text: String,
    /// Where the last preview ended (to redo it only when there is more).
    previewed: usize,
}

pub struct Live {
    progress: Arc<Mutex<Progress>>,
    busy: Arc<AtomicBool>,
    last: Mutex<Instant>,
}

impl Live {
    pub fn new() -> Self {
        Self { progress: Arc::default(), busy: Arc::new(AtomicBool::new(false)), last: Mutex::new(Instant::now()) }
    }

    /// Called on the dictation thread's ticks: commits a stretch at a pause,
    /// or refreshes the preview of what is not written yet.
    pub fn tick(&self, app: &AppHandle, samples: &[f32]) {
        {
            let mut last = self.last.lock().unwrap();
            if last.elapsed() < EVERY || self.busy.load(Ordering::Relaxed) {
                return;
            }
            *last = Instant::now();
        }
        let (upto, previewed) = {
            let p = self.progress.lock().unwrap();
            (p.upto, p.previewed)
        };
        let len = samples.len();
        if len <= upto + KEEP_TAIL {
            return;
        }
        let end = len - KEEP_TAIL;
        let unwritten = end - upto;
        // A pause to commit at, or the quietest moment once too much waits.
        let cut = if unwritten >= MIN_CHUNK {
            let from = (upto + MIN_CHUNK / 2).max(end.saturating_sub(SEARCH));
            pause_in(&samples[upto..end], from - upto).map(|c| upto + c).or_else(|| {
                (unwritten >= MAX_UNCOMMITTED).then(|| upto + quietest(&samples[upto..end], from - upto))
            })
        } else {
            None
        };
        let (progress, busy, app) = (self.progress.clone(), self.busy.clone(), app.clone());
        if let Some(cut) = cut {
            let chunk = samples[upto..cut].to_vec();
            self.busy.store(true, Ordering::Relaxed);
            std::thread::spawn(move || {
                let words = speech::transcribe(&chunk).map(|t| t.text).unwrap_or_default();
                let shown = {
                    let mut p = progress.lock().unwrap();
                    p.upto = cut;
                    p.previewed = cut;
                    append(&mut p.text, &words);
                    p.text.clone()
                };
                busy.store(false, Ordering::Relaxed);
                show(&app, shown);
            });
        } else if len - upto >= PREVIEW_MIN && len >= previewed + PREVIEW_STEP {
            // The words not written yet, as a preview only.
            let chunk = samples[upto..len].to_vec();
            self.busy.store(true, Ordering::Relaxed);
            std::thread::spawn(move || {
                let words = speech::transcribe(&chunk).map(|t| t.text).unwrap_or_default();
                let shown = {
                    let mut p = progress.lock().unwrap();
                    // A commit landed meanwhile: this preview is out of date.
                    if p.upto != upto {
                        None
                    } else {
                        p.previewed = len;
                        let mut text = p.text.clone();
                        append(&mut text, &words);
                        Some(text)
                    }
                };
                busy.store(false, Ordering::Relaxed);
                if let Some(shown) = shown {
                    show(&app, shown);
                }
            });
        }
    }

    /// After the recording: what is written for good and where it ends, once
    /// the stretch being transcribed is done (a few seconds at most).
    pub fn finish(&self) -> (String, usize) {
        let started = Instant::now();
        while self.busy.load(Ordering::Relaxed) && started.elapsed() < Duration::from_secs(8) {
            std::thread::sleep(Duration::from_millis(15));
        }
        let p = self.progress.lock().unwrap();
        (p.text.clone(), p.upto)
    }
}

fn append(text: &mut String, words: &str) {
    let words = words.trim();
    if words.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(words);
}

fn show(app: &AppHandle, text: String) {
    if !text.is_empty() {
        pill::update(app, |s| s.live = Some(text));
    }
}

/// Mean power of each 100 ms window from `from`, with where it starts.
fn windows(audio: &[f32], from: usize) -> Vec<(f64, usize)> {
    let mut out = Vec::new();
    let mut at = from;
    while at + WINDOW <= audio.len() {
        let power = audio[at..at + WINDOW].iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / WINDOW as f64;
        out.push((power, at));
        at += WINDOW / 2;
    }
    out
}

/// The middle of the quietest window from `from`.
fn quietest(audio: &[f32], from: usize) -> usize {
    windows(audio, from)
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(audio.len(), |(_, at)| at + WINDOW / 2)
}

/// A pause from `from`: the quietest window, if it is silent or clearly
/// quieter than the speech in `audio` (judged on its loudest quarter).
fn pause_in(audio: &[f32], from: usize) -> Option<usize> {
    let (power, at) = windows(audio, from).into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
    let mut all: Vec<f64> = windows(audio, 0).into_iter().map(|(p, _)| p).collect();
    all.sort_by(|a, b| b.total_cmp(a));
    let loud = all.iter().take(all.len().div_ceil(4)).sum::<f64>() / all.len().div_ceil(4).max(1) as f64;
    (power < SILENCE || power < loud * PAUSE_RATIO).then_some(at + WINDOW / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64, level: f32) -> Vec<f32> {
        (0..(seconds * RATE as f64) as usize).map(|i| (i as f32 * 0.05).sin() * level).collect()
    }

    #[test]
    fn cuts_in_the_pause_between_two_phrases() {
        let mut audio = tone(2.0, 0.3);
        let pause_at = audio.len();
        audio.extend(std::iter::repeat_n(0.0, RATE / 2));
        audio.extend(tone(1.5, 0.3));
        let cut = pause_in(&audio, RATE).expect("a pause");
        assert!(cut > pause_at && cut < pause_at + RATE / 2, "cut at {cut}");
    }

    #[test]
    fn a_noisy_room_still_has_pauses() {
        // Loud speech, then a dip to a noise floor well above true silence.
        let mut audio = tone(2.0, 0.9);
        let pause_at = audio.len();
        audio.extend(tone(0.4, 0.05));
        audio.extend(tone(1.5, 0.9));
        let cut = pause_in(&audio, RATE).expect("a pause");
        assert!(cut > pause_at && cut < pause_at + RATE * 2 / 5, "cut at {cut}");
    }

    #[test]
    fn no_pause_in_steady_speech() {
        let audio = tone(4.0, 0.3);
        assert_eq!(pause_in(&audio, RATE), None);
        // But the quietest moment is always found, for the hard cap.
        assert!(quietest(&audio, RATE) >= RATE);
    }
}
