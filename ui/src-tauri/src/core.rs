use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::supervisor::{CoreEvent, CoreState, Launch, StartError, Supervisor};

pub const ATTACH_VAR: &str = "TRAWLER_API";
pub const STATE_EVENT: &str = "core://state";
pub const LOG_EVENT: &str = "core://log";
const READY_TIMEOUT: Duration = Duration::from_secs(20);
const STOP_GRACE: Duration = Duration::from_secs(5);
const CA_FILES: [&str; 2] = ["trawler-ca.pem", "trawler-ca.key"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Attach(String),
    Own,
}

pub fn mode(attach: Option<&str>) -> Mode {
    match attach.map(str::trim).filter(|api| !api.is_empty()) {
        Some(api) => Mode::Attach(api.to_string()),
        None => Mode::Own,
    }
}

pub enum Core {
    Owned(Arc<Supervisor>),
    Attached(String),
}

impl Core {
    fn state(&self) -> CoreState {
        match self {
            Self::Owned(supervisor) => supervisor.snapshot(),
            Self::Attached(api) => CoreState::Attached { api: api.clone() },
        }
    }

    fn api_url(&self) -> Option<String> {
        match self.state() {
            CoreState::Running { api, .. } => Some(format!("http://{api}")),
            CoreState::Attached { api } => Some(api),
            CoreState::Starting | CoreState::Exited { .. } => None,
        }
    }

    pub fn stop(&self) {
        if let Self::Owned(supervisor) = self {
            supervisor.stop();
        }
    }
}

pub fn ca_dir(app_data: &Path) -> PathBuf {
    app_data.join("ca")
}

/// The CA a pre-app core minted in `./ca` is the one devices already trust, so
/// the app's first launch carries it over instead of minting a stranger. Only a
/// whole CA moves, and an existing one is never overwritten.
pub fn adopt_ca(legacy: &Path, ca_dir: &Path) -> std::io::Result<bool> {
    let whole = CA_FILES.iter().all(|file| legacy.join(file).is_file());
    let present = CA_FILES.iter().any(|file| ca_dir.join(file).exists());
    if !whole || present {
        return Ok(false);
    }
    std::fs::create_dir_all(ca_dir)?;
    for file in CA_FILES {
        std::fs::copy(legacy.join(file), ca_dir.join(file))?;
    }
    Ok(true)
}

pub fn refusal(err: &StartError) -> String {
    match err {
        StartError::Conflict { port } => format!(
            "Another program is using port {port}, which Trawler's capture service needs. \
             Quit that program, then open Trawler again."
        ),
        other => format!("{other}. Open Trawler again to retry."),
    }
}

pub fn setup<R: Runtime>(
    app: &mut tauri::App<R>,
    mode: Mode,
) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let core = match mode {
        Mode::Attach(api) => Core::Attached(api),
        Mode::Own => match own(&handle) {
            Ok(supervisor) => Core::Owned(Arc::new(supervisor)),
            Err(err) => {
                refuse(&handle, &err);
                return Ok(());
            }
        },
    };
    if let Some(api) = core.api_url() {
        handle.plugin(crate::core_address::init(&api))?;
    }
    app.manage(core);
    let window =
        app.config().app.windows.first().ok_or("tauri.conf.json declares no window")?.clone();
    WebviewWindowBuilder::from_config(&handle, &window)?.build()?;
    Ok(())
}

fn own<R: Runtime>(app: &AppHandle<R>) -> Result<Supervisor, StartError> {
    let program = std::env::current_exe().map_err(|err| StartError::Spawn(err.to_string()))?;
    let app_data = app.path().app_data_dir().map_err(|err| StartError::Spawn(err.to_string()))?;
    let ca_dir = ca_dir(&app_data);
    if let Ok(cwd) = std::env::current_dir() {
        if let Err(err) = adopt_ca(&cwd.join(trawler::Config::default().ca_dir), &ca_dir) {
            report(&format!("adopting ./ca into {} failed: {err}", ca_dir.display()));
        }
    }
    let defaults = trawler::Config::default();
    let launch = Launch {
        program,
        leading_args: vec![crate::CORE_FLAG.into()],
        ca_dir,
        proxy_port: defaults.proxy_addr.port(),
        api_port: defaults.api_addr.port(),
        ready_timeout: READY_TIMEOUT,
        grace: STOP_GRACE,
    };
    let emitter = app.clone();
    Supervisor::start(
        launch,
        Arc::new(move |event| {
            let _ = match event {
                CoreEvent::State(state) => emitter.emit(STATE_EVENT, state),
                CoreEvent::Log(line) => emitter.emit(LOG_EVENT, line),
            };
        }),
    )
}

fn refuse<R: Runtime>(app: &AppHandle<R>, err: &StartError) {
    let message = refusal(err);
    report(&message);
    let quit = app.clone();
    app.dialog()
        .message(message)
        .title("Trawler can't start its capture service")
        .kind(MessageDialogKind::Error)
        .show(move |_| quit.exit(1));
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
fn report(line: &str) {
    eprintln!("{line}");
}

pub fn focus_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn core_status(core: State<'_, Core>) -> CoreState {
    core.state()
}

#[tauri::command]
pub fn core_log(core: State<'_, Core>) -> Vec<String> {
    match core.inner() {
        Core::Owned(supervisor) => supervisor.log(),
        Core::Attached(_) => Vec::new(),
    }
}

#[tauri::command]
pub async fn restart_core(core: State<'_, Core>) -> Result<CoreState, String> {
    let supervisor = match core.inner() {
        Core::Owned(supervisor) => Arc::clone(supervisor),
        Core::Attached(api) => {
            return Err(format!("the capture service at {api} is not managed by this app"))
        }
    };
    tauri::async_runtime::spawn_blocking(move || supervisor.restart())
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_in_the_variable_attaches() {
        assert_eq!(
            mode(Some(" http://127.0.0.1:9000 ")),
            Mode::Attach("http://127.0.0.1:9000".into())
        );
    }

    #[test]
    fn an_absent_or_blank_variable_owns_a_core() {
        assert_eq!(mode(None), Mode::Own);
        assert_eq!(mode(Some("  ")), Mode::Own);
    }

    #[test]
    fn a_conflict_names_the_port_and_what_to_do() {
        let sentence = refusal(&StartError::Conflict { port: 7420 });
        assert!(sentence.contains("port 7420"), "{sentence}");
        assert!(sentence.contains("Quit that program"), "{sentence}");
    }

    #[test]
    fn any_other_refusal_says_why_and_to_retry() {
        let sentence = refusal(&StartError::Exited { code: Some(1), signal: None });
        assert_eq!(
            sentence,
            "the capture service exited with code 1 before it was ready. Open Trawler again to retry."
        );
    }

    #[test]
    fn the_ca_lives_in_the_app_data_dir() {
        assert_eq!(
            ca_dir(Path::new("/Users/u/Library/Application Support/dev.trawler.app")),
            PathBuf::from("/Users/u/Library/Application Support/dev.trawler.app/ca")
        );
    }

    fn mint(dir: &Path, contents: &str) {
        std::fs::create_dir_all(dir).unwrap();
        for file in CA_FILES {
            std::fs::write(dir.join(file), format!("{contents} {file}")).unwrap();
        }
    }

    #[test]
    fn a_whole_legacy_ca_is_adopted_once() {
        let tmp = tempfile::tempdir().unwrap();
        let (legacy, target) = (tmp.path().join("cwd/ca"), tmp.path().join("data/ca"));
        mint(&legacy, "legacy");
        assert!(adopt_ca(&legacy, &target).unwrap());
        for file in CA_FILES {
            assert_eq!(
                std::fs::read_to_string(target.join(file)).unwrap(),
                format!("legacy {file}")
            );
        }
        assert!(!adopt_ca(&legacy, &target).unwrap());
    }

    #[test]
    fn an_existing_app_ca_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let (legacy, target) = (tmp.path().join("cwd/ca"), tmp.path().join("data/ca"));
        mint(&legacy, "legacy");
        mint(&target, "app");
        assert!(!adopt_ca(&legacy, &target).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join("trawler-ca.pem")).unwrap(),
            "app trawler-ca.pem"
        );
    }

    #[test]
    fn half_a_legacy_ca_is_left_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let (legacy, target) = (tmp.path().join("cwd/ca"), tmp.path().join("data/ca"));
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("trawler-ca.pem"), "cert only").unwrap();
        assert!(!adopt_ca(&legacy, &target).unwrap());
        assert!(!target.exists());
    }

    #[test]
    fn no_legacy_ca_adopts_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!adopt_ca(&tmp.path().join("cwd/ca"), &tmp.path().join("data/ca")).unwrap());
    }
}
