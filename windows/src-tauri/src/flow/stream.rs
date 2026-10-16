// Fork: a dictation transcribed while you speak. Every half second the audio
// recorded so far is looked at: once at least two seconds wait past what is
// already written, it is cut at a pause (the quietest 100 ms of the last three
// seconds, if quiet enough), and that stretch goes to Parakeet in the
// background. The words so far show in the pill as they come; when you let
// go, only the stretch since the last cut is left to transcribe, so the paste
// comes about as fast for a long dictation as for a short one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use super::pill;
use crate::speech;

const RATE: usize = 16_000;
/// The shortest stretch sent ahead.
const MIN_CHUNK: usize = RATE * 2;
/// Never cut in the last half second: a word may be under way.
const KEEP_TAIL: usize = RATE / 2;
/// How far back a pause is looked for.
const SEARCH: usize = RATE * 3;
const WINDOW: usize = RATE / 10;
/// A pause: quieter than about -40 dBFS on average over 100 ms.
const PAUSE_POWER: f64 = 1e-4;
const EVERY: Duration = Duration::from_millis(500);

#[derive(Default)]
struct Progress {
    /// Samples already transcribed, from the start.
    upto: usize,
    text: String,
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

    /// Called on the dictation thread's ticks: sends a finished stretch ahead.
    pub fn tick(&self, app: &AppHandle, samples: &[f32]) {
        {
            let mut last = self.last.lock().unwrap();
            if last.elapsed() < EVERY || self.busy.load(Ordering::Relaxed) {
                return;
            }
            *last = Instant::now();
        }
        let upto = self.progress.lock().unwrap().upto;
        if samples.len() < upto + MIN_CHUNK + KEEP_TAIL {
            return;
        }
        let end = samples.len() - KEEP_TAIL;
        let from = (upto + MIN_CHUNK / 2).max(end.saturating_sub(SEARCH));
        let Some(cut) = quiet_cut(samples, from, end) else { return };
        let chunk = samples[upto..cut].to_vec();
        self.busy.store(true, Ordering::Relaxed);
        let (progress, busy, app) = (self.progress.clone(), self.busy.clone(), app.clone());
        std::thread::spawn(move || {
            let words = speech::transcribe(&chunk).map(|t| t.text).unwrap_or_default();
            let so_far = {
                let mut p = progress.lock().unwrap();
                p.upto = cut;
                let words = words.trim();
                if !words.is_empty() {
                    if !p.text.is_empty() {
                        p.text.push(' ');
                    }
                    p.text.push_str(words);
                }
                p.text.clone()
            };
            busy.store(false, Ordering::Relaxed);
            if !so_far.is_empty() {
                pill::update(&app, |s| s.live = Some(so_far));
            }
        });
    }

    /// After the recording: what is already written and where it ends, once a
    /// stretch still being transcribed is done (a few seconds at most).
    pub fn finish(&self) -> (String, usize) {
        let started = Instant::now();
        while self.busy.load(Ordering::Relaxed) && started.elapsed() < Duration::from_secs(8) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let p = self.progress.lock().unwrap();
        (p.text.clone(), p.upto)
    }
}

/// The middle of the quietest 100 ms between `from` and `to`, if it is a pause.
fn quiet_cut(samples: &[f32], from: usize, to: usize) -> Option<usize> {
    let to = to.min(samples.len());
    let mut best: Option<(f64, usize)> = None;
    let mut at = from;
    while at + WINDOW <= to {
        let power = samples[at..at + WINDOW].iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / WINDOW as f64;
        if best.is_none_or(|(p, _)| power < p) {
            best = Some((power, at + WINDOW / 2));
        }
        at += WINDOW / 2;
    }
    best.filter(|&(power, _)| power < PAUSE_POWER).map(|(_, cut)| cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE as f64) as usize).map(|i| (i as f32 * 0.05).sin() * 0.3).collect()
    }

    #[test]
    fn cuts_in_the_pause_between_two_phrases() {
        let mut audio = tone(2.0);
        let pause_at = audio.len();
        audio.extend(std::iter::repeat_n(0.0, RATE / 2));
        audio.extend(tone(1.5));
        let cut = quiet_cut(&audio, RATE, audio.len()).expect("a pause");
        assert!(cut > pause_at && cut < pause_at + RATE / 2, "cut at {cut}");
    }

    #[test]
    fn no_cut_without_a_pause() {
        let audio = tone(4.0);
        assert_eq!(quiet_cut(&audio, RATE, audio.len()), None);
    }
}
