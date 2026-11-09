//! The window's place and size: put back at start, and kept in
//! `workspace.toml` when they change.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use blenima_core::workspace::{Area, WindowState, Workspace};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window};

use crate::Saved;

/// How long the window must stay still before its place and size are kept.
/// While it is being maximized it moves and changes size a moment before it
/// says it is maximized, so reading it at once would keep the wrong size.
const SETTLE: Duration = Duration::from_millis(500);

/// When the window last moved or changed size, while that is not kept yet.
#[derive(Default)]
pub struct Unsaved {
    changed: Mutex<Option<Instant>>,
    /// Set once the window has been put in place at start.
    restored: AtomicBool,
}

/// Puts the window where it was, if that is still on a screen, then shows
/// it. The window starts hidden, so it never jumps.
pub fn restore(window: &WebviewWindow, saved: Option<WindowState>) {
    if let Some(state) = saved {
        let screens: Vec<Area> = window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(|monitor| {
                let area = monitor.work_area();
                Area {
                    x: area.position.x,
                    y: area.position.y,
                    width: area.size.width,
                    height: area.size.height,
                }
            })
            .collect();
        let _ = window.set_size(PhysicalSize::new(state.width, state.height));
        if state.reachable(&screens) {
            let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
        } else {
            // Its screen is gone: the middle of the main screen instead.
            let _ = window.center();
        }
        if state.maximized {
            let _ = window.maximize();
        }
    }
    let _ = window.show();
    window
        .state::<Unsaved>()
        .restored
        .store(true, Ordering::Relaxed);
}

/// Notes that the window moved or changed size. It is kept once it settles.
pub fn changed<R: Runtime>(window: &Window<R>) {
    let unsaved = window.state::<Unsaved>();
    // The moves of putting it in place at start are not the user's.
    if unsaved.restored.load(Ordering::Relaxed) {
        *unsaved
            .changed
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
    }
}

/// Keeps the window's place and size once it has been still for a moment,
/// checking twice a second.
pub fn keep_saving(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(250));
            let settled = {
                let unsaved = app.state::<Unsaved>();
                let mut changed = unsaved
                    .changed
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                match *changed {
                    Some(at) if at.elapsed() >= SETTLE => changed.take().is_some(),
                    _ => false,
                }
            };
            if settled && let Some(window) = app.get_webview_window("main") {
                keep(&window.as_ref().window());
            }
        }
    });
}

/// Keeps the window's place and size now, as when it closes.
pub fn keep<R: Runtime>(window: &Window<R>) {
    // A minimized window reports a place far off every screen.
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let file = window.state::<Saved<Workspace>>();
    let mut workspace = file.current.lock().unwrap_or_else(PoisonError::into_inner);
    let maximized = window.is_maximized().unwrap_or(false);
    let state = match (maximized, workspace.window) {
        // Maximized: keep the place and size it goes back to.
        (true, Some(before)) => Some(WindowState {
            maximized: true,
            ..before
        }),
        (true, None) => place_and_size(window).map(|now| WindowState {
            maximized: true,
            ..now
        }),
        (false, _) => place_and_size(window),
    };
    if state.is_some() && state != workspace.window {
        workspace.window = state;
        // Saved under the lock, so it never crosses a save of the tabs.
        let _ = workspace.save(&file.path);
    }
}

fn place_and_size<R: Runtime>(window: &Window<R>) -> Option<WindowState> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}
