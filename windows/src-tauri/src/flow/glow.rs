// Fork: a soft glow around the mouse pointer while Flow records (a dictation
// being listened to, or a call), so a live microphone is plain to see wherever
// you are looking. Windows only: a small see-through window that never takes
// the mouse or the keyboard, moved onto the pointer about 60 times a second
// while it shows, and parked off screen, its thread asleep, the rest of the time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread::Thread;

use tauri::{AppHandle, Emitter};

pub const LABEL: &str = "glow";

static ON: AtomicBool = AtomicBool::new(false);
static MOVER: OnceLock<Thread> = OnceLock::new();

/// Creates the glow's window, parked, and the thread that moves it.
pub fn setup(app: &AppHandle) {
    #[cfg(windows)]
    imp::setup(app);
    #[cfg(not(windows))]
    let _ = app;
}

/// Shows the glow (recording) or hides it.
pub fn set(app: &AppHandle, on: bool) {
    if ON.swap(on, Ordering::Relaxed) == on {
        return;
    }
    let _ = app.emit_to(LABEL, "glow-on", on);
    if let Some(t) = MOVER.get() {
        t.unpark();
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::time::Duration;
    use tauri::{Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

    /// Window size, logical: room for the glow's soft edge.
    const SIZE: f64 = 120.0;
    const PARKED: (i32, i32) = (-32000, -32000);

    fn page_url(app: &AppHandle) -> WebviewUrl {
        #[cfg(dev)]
        if let Some(mut base) = app.config().build.dev_url.clone() {
            base.set_path("/glow.html");
            return WebviewUrl::External(base);
        }
        let _ = app;
        WebviewUrl::App("glow.html".into())
    }

    pub fn setup(app: &AppHandle) {
        let built = WebviewWindowBuilder::new(app, LABEL, page_url(app))
            .additional_browser_args(crate::BROWSER_ARGS)
            .title("Coucou")
            .inner_size(SIZE, SIZE)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .disable_drag_drop_handler()
            .position(PARKED.0 as f64, PARKED.1 as f64)
            .visible(true)
            .build();
        let win = match built {
            Ok(win) => win,
            Err(err) => {
                crate::log::line(format!("glow window failed: {err}"));
                return;
            }
        };
        crate::platform::make_non_activating(&win);
        let _ = win.set_ignore_cursor_events(true);
        let _ = win.set_position(PhysicalPosition::new(PARKED.0, PARKED.1));

        let app = app.clone();
        let handle = std::thread::spawn(move || {
            let mut last = PARKED;
            loop {
                if !ON.load(Ordering::Relaxed) {
                    if last != PARKED {
                        last = PARKED;
                        if let Some(w) = app.get_webview_window(LABEL) {
                            let _ = w.set_position(PhysicalPosition::new(PARKED.0, PARKED.1));
                        }
                    }
                    std::thread::park();
                    continue;
                }
                if let (Some(w), Some((cx, cy))) = (app.get_webview_window(LABEL), crate::platform::cursor_physical()) {
                    let half = SIZE * w.scale_factor().unwrap_or(1.0) / 2.0;
                    let at = ((cx - half).round() as i32, (cy - half).round() as i32);
                    // Only when the pointer moved: a still mouse costs nothing.
                    if at != last {
                        last = at;
                        let _ = w.set_position(PhysicalPosition::new(at.0, at.1));
                    }
                }
                std::thread::sleep(Duration::from_millis(16));
            }
        });
        let _ = MOVER.set(handle.thread().clone());
    }
}
