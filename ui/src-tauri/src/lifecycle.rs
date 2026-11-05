// The app may outlive its main window (DEC-77): never without a menu-bar icon to
// reach it again, and the core lives exactly as long as the process either way.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};

use crate::keep_capturing::{read_keep_capturing, KeepCapturing};
use crate::settings::MAIN_WINDOW;
use crate::window_geometry;

pub struct Lifecycle {
    config_dir: Option<PathBuf>,
    tray: AtomicBool,
    reopening: Mutex<()>,
    quit_requested: AtomicBool,
}

pub const QUIT_REQUEST_EVENT: &str = "quit-requested";
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_millis(150);

impl Lifecycle {
    pub fn new(config_dir: Option<PathBuf>) -> Self {
        Self {
            config_dir,
            tray: AtomicBool::new(false),
            reopening: Mutex::new(()),
            quit_requested: AtomicBool::new(false),
        }
    }

    pub fn config_dir(&self) -> Option<&Path> {
        self.config_dir.as_deref()
    }

    // With nowhere to store an answer there is nothing to ask, so it reads off.
    pub fn keep_capturing(&self) -> KeepCapturing {
        self.config_dir().map_or(KeepCapturing::Off, read_keep_capturing)
    }

    pub fn mark_tray(&self, available: bool) {
        self.tray.store(available, Ordering::Relaxed);
    }

    pub fn tray_available(&self) -> bool {
        self.tray.load(Ordering::Relaxed)
    }

    pub fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::SeqCst);
    }

    pub fn take_quit_request(&self) -> bool {
        self.quit_requested.swap(false, Ordering::SeqCst)
    }

    pub fn stays_windowless(&self) -> bool {
        stays_windowless(self.keep_capturing(), self.tray_available())
    }
}

#[tauri::command]
pub fn tray_available(lifecycle: tauri::State<'_, Lifecycle>) -> bool {
    lifecycle.tray_available()
}

#[tauri::command]
pub fn take_quit_request(lifecycle: tauri::State<'_, Lifecycle>) -> bool {
    lifecycle.take_quit_request()
}

#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    Shown,
    Hidden,
}

pub fn stays_windowless(keep_capturing: KeepCapturing, tray: bool) -> bool {
    keep_capturing == KeepCapturing::On && tray
}

pub fn ends_the_app(label: &str, stays_windowless: bool) -> bool {
    label == MAIN_WINDOW && !stays_windowless
}

pub fn opens_window_at_launch(background: bool, tray: bool) -> bool {
    !(background && tray)
}

pub fn dock_for(windows_open: bool) -> Dock {
    if windows_open {
        Dock::Shown
    } else {
        Dock::Hidden
    }
}

pub fn install<R: Runtime>(app: &AppHandle<R>, config_dir: Option<PathBuf>) {
    app.manage(Lifecycle::new(config_dir));
}

fn lifecycle<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::State<'_, Lifecycle>> {
    app.try_state::<Lifecycle>()
}

/// What the main window's destruction leaves behind: `true` when the app must
/// now exit, as it always did before DEC-77.
pub fn on_window_destroyed<R: Runtime>(app: &AppHandle<R>, label: &str) -> bool {
    let stays = lifecycle(app).is_some_and(|lifecycle| lifecycle.stays_windowless());
    let ends = ends_the_app(label, stays);
    // A request met by a window that was already going away reached nobody who
    // can act on it; a window only a later Open builds must not find it and quit.
    if label == MAIN_WINDOW
        && !ends
        && lifecycle(app).is_some_and(|lifecycle| lifecycle.take_quit_request())
    {
        drop(request_quit(app));
    }
    ends
}

/// Tauri asks to exit once the last window is gone. A windowless app with a
/// menu-bar icon refuses: only the main window's own rule, or a quit, ends it.
pub fn holds_last_window_exit<R: Runtime>(app: &AppHandle<R>) -> bool {
    lifecycle(app).is_some_and(|lifecycle| lifecycle.tray_available())
}

pub fn on_window_gone<R: Runtime>(app: &AppHandle<R>) {
    set_dock(app, dock_for(!app.webview_windows().is_empty()));
}

pub fn open_at_launch<R: Runtime>(app: &AppHandle<R>, background: bool) -> tauri::Result<()> {
    let tray = lifecycle(app).is_some_and(|lifecycle| lifecycle.tray_available());
    if opens_window_at_launch(background, tray) {
        build_main(app)?;
    } else {
        set_dock(app, Dock::Hidden);
    }
    Ok(())
}

pub fn build_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW)
        .ok_or(tauri::Error::WindowNotFound)?
        .clone();
    set_dock(app, Dock::Shown);
    let built =
        WebviewWindowBuilder::from_config(app, &config).and_then(WebviewWindowBuilder::build);
    let window = built.inspect_err(|_| on_window_gone(app))?;
    let window_itself = window.as_ref().window();
    window_geometry::restore(&window_itself, window_geometry::min_size_of(&window_itself));
    window.set_focus()?;
    Ok(window)
}

// A Dock click and a second launch can arrive together; one of them builds.
pub fn reopen_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let lifecycle = lifecycle(app);
    let _reopening = lifecycle
        .as_ref()
        .map(|lifecycle| lifecycle.reopening.lock().unwrap_or_else(PoisonError::into_inner));
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(window);
    }
    build_main(app)
}

// Built off the event loop: WebviewWindowBuilder::build deadlocks on Windows when
// called from an event handler (wry#583), the same reason Settings opens this way.
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn reopen_main_in_background<R: Runtime>(app: &AppHandle<R>) -> JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(err) = reopen_main(&app) {
            eprintln!("reopening the main window failed: {err}");
        }
    })
}

// The window asks: it runs DEC-64's check and then calls `quit_app`. The flag
// outlives a window that is still being built, whose first read takes it; the
// event reaches a window that already listens, and whichever takes the flag acts.
pub fn request_quit<R: Runtime>(app: &AppHandle<R>) -> JoinHandle<()> {
    if let Some(lifecycle) = lifecycle(app) {
        lifecycle.request_quit();
    }
    let app = app.clone();
    std::thread::spawn(move || quit_in_window(&app))
}

// The first attempt can meet a main window that is mid-destroy; the second finds
// it gone and builds. When even that fails the app neither exits (DEC-64's check
// never ran, the proxy may point at this process) nor keeps the request, which a
// window opened later for another reason would answer by quitting unasked.
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
fn quit_in_window<R: Runtime>(app: &AppHandle<R>) {
    match attempt_twice(|| reopen_main(app)) {
        Ok(window) => {
            if let Err(err) = window.emit(QUIT_REQUEST_EVENT, ()) {
                eprintln!("announcing the quit request failed: {err}");
            }
        }
        Err(err) => {
            eprintln!("opening the main window for Quit failed, still running: {err}");
            if let Some(lifecycle) = lifecycle(app) {
                lifecycle.take_quit_request();
            }
        }
    }
}

// Runs on the thread `request_quit` spawned, never a tokio worker.
#[allow(clippy::disallowed_methods)]
fn attempt_twice<T, E>(mut attempt: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    attempt().or_else(|_| {
        std::thread::sleep(RETRY_AFTER);
        attempt()
    })
}

#[cfg(target_os = "macos")]
#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn set_dock<R: Runtime>(app: &AppHandle<R>, dock: Dock) {
    let policy = match dock {
        Dock::Shown => tauri::ActivationPolicy::Regular,
        Dock::Hidden => tauri::ActivationPolicy::Accessory,
    };
    if let Err(err) = app.set_activation_policy(policy) {
        eprintln!("setting the Dock icon to {dock:?} failed: {err}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock<R: Runtime>(_app: &AppHandle<R>, _dock: Dock) {}

#[cfg(test)]
mod tests {
    use super::*;
    use KeepCapturing::{Off, On, Unanswered};

    #[test]
    fn only_a_switched_on_app_with_a_tray_stays_windowless() {
        for (keep_capturing, tray, stays) in [
            (On, true, true),
            (On, false, false),
            (Off, true, false),
            (Off, false, false),
            (Unanswered, true, false),
            (Unanswered, false, false),
        ] {
            assert_eq!(
                stays_windowless(keep_capturing, tray),
                stays,
                "{keep_capturing:?} with tray {tray}"
            );
        }
    }

    #[test]
    fn only_the_main_window_ends_the_app_and_only_when_it_may_not_stay() {
        assert!(ends_the_app(MAIN_WINDOW, false));
        assert!(!ends_the_app(MAIN_WINDOW, true));
        for label in [crate::settings::SETTINGS_WINDOW, "data-6162"] {
            assert!(!ends_the_app(label, false), "{label}");
            assert!(!ends_the_app(label, true), "{label}");
        }
    }

    #[test]
    fn a_background_launch_skips_the_window_only_with_a_tray_to_reach_it() {
        assert!(!opens_window_at_launch(true, true));
        assert!(opens_window_at_launch(true, false));
        assert!(opens_window_at_launch(false, true));
        assert!(opens_window_at_launch(false, false));
    }

    #[test]
    fn the_dock_shows_while_any_window_is_open() {
        assert_eq!(dock_for(true), Dock::Shown);
        assert_eq!(dock_for(false), Dock::Hidden);
    }

    #[test]
    fn no_config_dir_reads_off_and_a_tray_is_absent_until_marked() {
        let lifecycle = Lifecycle::new(None);
        assert_eq!(lifecycle.keep_capturing(), Off);
        assert!(!lifecycle.tray_available());
        lifecycle.mark_tray(true);
        assert!(lifecycle.tray_available());
        assert!(!lifecycle.stays_windowless());
    }

    #[test]
    fn a_quit_request_is_taken_once() {
        let lifecycle = Lifecycle::new(None);
        assert!(!lifecycle.take_quit_request());
        lifecycle.request_quit();
        lifecycle.request_quit();
        assert!(lifecycle.take_quit_request());
        assert!(!lifecycle.take_quit_request());
    }

    #[test]
    fn an_attempt_is_retried_once_and_only_once() {
        let mut calls = 0;
        assert_eq!(
            attempt_twice(|| {
                calls += 1;
                if calls == 1 {
                    Err("mid-destroy")
                } else {
                    Ok(calls)
                }
            }),
            Ok(2)
        );
        let mut calls = 0;
        assert_eq!(
            attempt_twice(|| {
                calls += 1;
                Err::<(), _>(calls)
            }),
            Err(2)
        );
        assert_eq!(attempt_twice(|| Ok::<_, ()>(7)), Ok(7));
    }

    #[test]
    fn the_preference_is_read_from_its_dir_on_every_ask() {
        let dir = tempfile::tempdir().unwrap();
        let lifecycle = Lifecycle::new(Some(dir.path().to_path_buf()));
        lifecycle.mark_tray(true);
        assert_eq!(lifecycle.keep_capturing(), Unanswered);
        assert!(!lifecycle.stays_windowless());
        crate::keep_capturing::write_keep_capturing(dir.path(), true).unwrap();
        assert!(lifecycle.stays_windowless());
    }
}
