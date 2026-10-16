// Fork: a glow traced around the mouse pointer itself while Flow records (a
// dictation being listened to, or a call), so a live microphone is plain to
// see wherever you are looking. Windows only: a small see-through window that
// never takes the mouse or the keyboard, kept on the pointer about 60 times a
// second while it shows, and parked off screen, its thread asleep, the rest of
// the time. The page draws the pointer's own image (sent whenever its shape
// changes: arrow, text beam, hand) with a glow round its outline, lined up on
// the real one.

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
    use base64::Engine;
    use serde::Serialize;
    use std::time::Duration;
    use tauri::{Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetDC,
        GetDIBits, GetObjectW, ReleaseDC, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, DI_NORMAL, HCURSOR, HICON, ICONINFO,
    };

    /// The pointer's image and where its tip is, in physical pixels.
    #[derive(Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CursorImage {
        src: String,
        hot_x: i32,
        hot_y: i32,
        width: i32,
        height: i32,
    }

    /// The icon drawn on a plain background, as top-down BGRA rows.
    unsafe fn draw(icon: HICON, w: i32, h: i32, background: u32) -> Option<Vec<u8>> {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bitmap.into());
        let brush = CreateSolidBrush(COLORREF(background));
        FillRect(mem, &RECT { left: 0, top: 0, right: w, bottom: h }, brush);
        let drawn = DrawIconEx(mem, 0, 0, icon, w, h, 0, None, DI_NORMAL);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let rows = GetDIBits(mem, bitmap, 0, h as u32, Some(pixels.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        (drawn.is_ok() && rows > 0).then_some(pixels)
    }

    /// The pointer as a PNG with its transparency: drawn once on black and once
    /// on white, the difference gives each pixel's alpha.
    unsafe fn cursor_image(cursor: HCURSOR) -> Option<CursorImage> {
        let icon = HICON(cursor.0);
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let color = !info.hbmColor.is_invalid();
        let mut bm = BITMAP::default();
        let probe = if color { info.hbmColor } else { info.hbmMask };
        GetObjectW(probe.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut bm as *mut BITMAP).cast()));
        let (w, h) = (bm.bmWidth, if color { bm.bmHeight } else { bm.bmHeight / 2 });
        if color {
            let _ = DeleteObject(info.hbmColor.into());
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(info.hbmMask.into());
        }
        if w <= 0 || h <= 0 || w > 256 || h > 256 {
            return None;
        }
        let on_black = draw(icon, w, h, 0x000000)?;
        let on_white = draw(icon, w, h, 0xFFFFFF)?;
        let mut rgba = Vec::with_capacity(on_black.len());
        for (b, wh) in on_black.chunks_exact(4).zip(on_white.chunks_exact(4)) {
            let a = 255 - (wh[1] as i32 - b[1] as i32).clamp(0, 255);
            let un = |c: u8| if a == 0 { 0 } else { (c as i32 * 255 / a).min(255) as u8 };
            rgba.extend_from_slice(&[un(b[2]), un(b[1]), un(b[0]), a as u8]);
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
        }
        Some(CursorImage {
            src: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png_bytes)),
            hot_x: info.xHotspot as i32,
            hot_y: info.yHotspot as i32,
            width: w,
            height: h,
        })
    }

    /// The pointer's current shape, when it shows.
    fn current_cursor() -> Option<HCURSOR> {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        unsafe { GetCursorInfo(&mut ci) }.ok()?;
        (!ci.hCursor.is_invalid()).then_some(ci.hCursor)
    }

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
            let mut shape: *mut std::ffi::c_void = std::ptr::null_mut();
            loop {
                if !ON.load(Ordering::Relaxed) {
                    if last != PARKED {
                        last = PARKED;
                        if let Some(w) = app.get_webview_window(LABEL) {
                            let _ = w.set_position(PhysicalPosition::new(PARKED.0, PARKED.1));
                        }
                    }
                    shape = std::ptr::null_mut();
                    std::thread::park();
                    continue;
                }
                // A new pointer shape: send its image to the page.
                if let Some(cursor) = current_cursor() {
                    if cursor.0 != shape {
                        shape = cursor.0;
                        if let Some(image) = unsafe { cursor_image(cursor) } {
                            let _ = app.emit_to(LABEL, "glow-cursor", image);
                        }
                    }
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
