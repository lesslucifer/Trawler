use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::supervisor::{CoreEvent, CoreState, Launch, Logged, Observer, StartError, Supervisor};

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

pub fn launch_ports(ephemeral: bool) -> (u16, u16) {
    if ephemeral {
        return (0, 0);
    }
    let defaults = trawler::Config::default();
    (defaults.proxy_addr.port(), defaults.api_addr.port())
}

/// The single-instance socket is one fixed path per machine, so only the app
/// whose core holds the fixed ports may claim it; an e2e app on ephemeral ports
/// handing over to the developer's own Trawler would test that one instead.
pub fn hands_over(mode: &Mode, (proxy_port, api_port): (u16, u16)) -> bool {
    *mode == Mode::Own && proxy_port != 0 && api_port != 0
}

pub enum Core {
    Owned(Arc<Supervisor>),
    Attached(String),
}

impl Core {
    pub fn state(&self) -> CoreState {
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

pub fn restart_refusal(err: &StartError) -> String {
    match err {
        StartError::Conflict { port } => {
            format!("Another program is using port {port}. Quit it, then try again.")
        }
        other => {
            let sentence = other.to_string();
            let mut chars = sentence.chars();
            chars.next().map_or_else(String::new, |first| {
                format!("{}{}.", first.to_uppercase(), chars.as_str())
            })
        }
    }
}

pub fn observer<R: Runtime>(app: &AppHandle<R>) -> Observer {
    let emitter = app.clone();
    Arc::new(move |event| {
        let _ = match event {
            CoreEvent::State(state) => {
                crate::tray::on_state(&emitter, &state);
                emitter.emit(STATE_EVENT, state)
            }
            CoreEvent::Log(logged) => emitter.emit(LOG_EVENT, logged),
        };
    })
}

pub fn setup<R: Runtime>(
    app: &mut tauri::App<R>,
    mode: Mode,
    background: bool,
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
    crate::tray::install(&handle);
    crate::lifecycle::open_at_launch(&handle, background)?;
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
    let (proxy_port, api_port) = launch_ports(crate::EPHEMERAL_PORTS);
    let launch = Launch {
        program,
        leading_args: vec![crate::CORE_FLAG.into()],
        ca_dir,
        proxy_port,
        api_port,
        ready_timeout: READY_TIMEOUT,
        grace: STOP_GRACE,
    };
    Supervisor::start(launch, observer(app))
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

#[tauri::command]
pub fn core_status(core: State<'_, Core>) -> CoreState {
    core.state()
}

#[tauri::command]
pub fn core_log(core: State<'_, Core>) -> Vec<Logged> {
    match core.inner() {
        Core::Owned(supervisor) => supervisor.log(),
        Core::Attached(_) => Vec::new(),
    }
}

fn owned_supervisor(core: &Core) -> Result<Arc<Supervisor>, String> {
    match core {
        Core::Owned(supervisor) => Ok(Arc::clone(supervisor)),
        Core::Attached(api) => {
            Err(format!("the capture service at {api} is not managed by this app"))
        }
    }
}

pub fn restart_in_background<R: Runtime>(app: &AppHandle<R>) -> std::thread::JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        let restarted = app
            .try_state::<Core>()
            .ok_or_else(|| "this app has no capture service".to_string())
            .and_then(|core| owned_supervisor(&core))
            .and_then(|supervisor| supervisor.restart().map_err(|err| restart_refusal(&err)));
        if let Err(why) = restarted {
            report(&format!("restarting the capture service failed: {why}"));
        }
    })
}

#[tauri::command]
pub async fn restart_core(core: State<'_, Core>) -> Result<CoreState, String> {
    let supervisor = owned_supervisor(core.inner())?;
    tauri::async_runtime::spawn_blocking(move || supervisor.restart())
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| restart_refusal(&err))
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
    fn a_production_core_takes_the_fixed_ports() {
        assert_eq!(launch_ports(false), (7420, 7421));
    }

    #[test]
    fn an_ephemeral_core_lets_the_os_choose() {
        assert_eq!(launch_ports(true), (0, 0));
    }

    #[test]
    fn only_an_owned_core_on_fixed_ports_hands_a_second_launch_over() {
        assert!(hands_over(&Mode::Own, (7420, 7421)));
        assert!(!hands_over(&Mode::Own, (0, 0)));
        assert!(!hands_over(&Mode::Attach("http://127.0.0.1:9000".into()), (7420, 7421)));
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
    fn a_restart_refused_for_a_port_says_to_quit_its_holder_and_try_again() {
        assert_eq!(
            restart_refusal(&StartError::Conflict { port: 7420 }),
            "Another program is using port 7420. Quit it, then try again."
        );
    }

    #[test]
    fn any_other_restart_refusal_is_one_capitalised_sentence() {
        assert_eq!(
            restart_refusal(&StartError::Exited { code: Some(1), signal: None }),
            "The capture service exited with code 1 before it was ready."
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
