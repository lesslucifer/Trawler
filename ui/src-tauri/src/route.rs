// A screen a by-hand step opens for the user (E-ONBRVP.W12, I1-I3). The set is
// closed and the shell builds every URL and path itself, so the webview names
// a route and never hands the shell something to open.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::setup::{
    argv, simctl, Emulators, SetupError, Simulators, LOGIN_KEYCHAIN, NO_EMULATOR_RUNNING,
    NO_SIMULATOR_RUNNING,
};

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub enum Route {
    ProxySettings,
    Keychain { pem: String },
    Certificate { pem: String },
    Browser { mark: String },
    Simulator,
    SimulatorRequest { mark: String },
    Emulator,
    EmulatorRequest { mark: String },
    EmulatorCertificate { pem: String },
    TrustedCredentials,
}

const MACOS_NETWORK_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.Network-Settings.extension";
const WINDOWS_PROXY_SETTINGS: &str = "ms-settings:network-proxy";
const WINDOWS_CER: &str = "trawler-ca.cer";
const GNOME_NETWORK_PANEL: [&str; 12] = [
    "gdbus",
    "call",
    "--session",
    "--dest",
    "org.gnome.Settings",
    "--object-path",
    "/org/gnome/Settings",
    "--method",
    "org.gtk.Actions.Activate",
    "launch-panel",
    "[<('network', @av [])>]",
    "{}",
];
const REQUEST_URL: &str = "https://example.com/?trawler-check=";
// Settings' own exported screens: the installer itself is not, so the closest is
// the page that lists it (W17's I5, I6).
const ANDROID_SECURITY_SETTINGS: &str = "com.android.settings.MORE_SECURITY_PRIVACY_SETTINGS";
const ANDROID_USER_CREDENTIALS: &str = "com.android.settings.TRUSTED_CREDENTIALS_USER";
const ANDROID_DOWNLOADED_CER: &str = "/sdcard/Download/trawler-ca.cer";
const ALREADY_IN_KEYCHAIN: &str = "already in";

// The mark is the window's own `<window>.<n>` (DEC-62), so anything else is
// refused before it reaches a URL.
fn valid_mark(mark: &str) -> bool {
    mark.split_once('.').is_some_and(|(window, n)| {
        !window.is_empty()
            && window.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
    })
}

// A device's mark is the four characters `deviceMark` hashes to (DEC-63).
fn valid_device_mark(mark: &str) -> bool {
    (1..=8).contains(&mark.len())
        && mark.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

// Each route as the commands that open it, in order. The certificate is added
// to the login keychain first: opening the file instead asks which keychain,
// and the default it offers may be one that holds no trust (W12's recording).
// Windows opens each through `start`, which returns once the screen is up: the
// certificate dialog runs inside its opener and would hold the command open.
// GNOME's Settings is asked over D-Bus for its Network panel, so it starts
// outside the shell's process.
// The emulator's routes reach every one running through adb, on any host; its
// start is the AVD used last, which the shell chose, never the webview.
fn am_start(action: &str) -> [&str; 5] {
    ["shell", "am", "start", "-a", action]
}

fn emulator_commands(route: &Route, cert: &str, emus: &Emulators) -> Option<Vec<Vec<String>>> {
    let each = |args: &[&str]| -> Vec<Vec<String>> {
        emus.running.iter().map(|serial| emus.on(serial, args)).collect()
    };
    match route {
        Route::Emulator => {
            Some(emus.last_avd.iter().map(|avd| argv(&[&emus.emulator, "-avd", &avd.id])).collect())
        }
        Route::EmulatorRequest { mark } => {
            let url = format!("'{REQUEST_URL}{mark}'");
            Some(each(&[&am_start("android.intent.action.VIEW")[..], &["-d", &url]].concat()))
        }
        Route::EmulatorCertificate { .. } => Some(
            emus.running
                .iter()
                .flat_map(|serial| {
                    [
                        emus.on(serial, &["push", cert, ANDROID_DOWNLOADED_CER]),
                        emus.on(serial, &am_start(ANDROID_SECURITY_SETTINGS)),
                    ]
                })
                .collect(),
        ),
        Route::TrustedCredentials => Some(each(&am_start(ANDROID_USER_CREDENTIALS))),
        _ => None,
    }
}

fn route_commands(
    os: &str,
    route: &Route,
    cert: &Path,
    sims: &Simulators,
    emus: &Emulators,
) -> Option<Vec<Vec<String>>> {
    match route {
        Route::Browser { mark } if !valid_mark(mark) => return None,
        Route::SimulatorRequest { mark } | Route::EmulatorRequest { mark }
            if !valid_device_mark(mark) =>
        {
            return None
        }
        _ => {}
    }
    let cert = cert.to_string_lossy();
    if let Some(commands) = emulator_commands(route, &cert, emus) {
        return Some(commands);
    }
    match (os, route) {
        ("macos", Route::ProxySettings) => Some(vec![argv(&["open", MACOS_NETWORK_SETTINGS])]),
        ("macos", Route::Keychain { .. }) => Some(vec![
            argv(&["security", "add-certificates", "-k", LOGIN_KEYCHAIN, &cert]),
            argv(&["open", "-a", "Keychain Access"]),
        ]),
        ("macos", Route::Browser { mark }) => {
            Some(vec![argv(&["open", &format!("{REQUEST_URL}{mark}")])])
        }
        ("macos", Route::Simulator) => Some(vec![argv(&["open", "-a", "Simulator"])]),
        ("macos", Route::SimulatorRequest { mark }) => {
            let url = format!("{REQUEST_URL}{mark}");
            let developer_dir = sims.developer_dir.as_deref();
            Some(
                sims.running
                    .iter()
                    .map(|udid| simctl(developer_dir, &["openurl", udid, &url]))
                    .collect(),
            )
        }
        ("windows", Route::ProxySettings) => Some(vec![windows_start(WINDOWS_PROXY_SETTINGS)]),
        ("windows", Route::Certificate { .. }) => Some(vec![windows_start(&cert)]),
        ("windows", Route::Browser { mark }) => {
            Some(vec![windows_start(&format!("{REQUEST_URL}{mark}"))])
        }
        ("linux", Route::ProxySettings) => Some(vec![argv(&GNOME_NETWORK_PANEL)]),
        ("linux", Route::Browser { mark }) => {
            Some(vec![argv(&["xdg-open", &format!("{REQUEST_URL}{mark}")])])
        }
        _ => None,
    }
}

fn windows_start(target: &str) -> Vec<String> {
    argv(&["cmd", "/C", "start", "", target])
}

fn staged(route: &Route) -> Result<PathBuf, SetupError> {
    match route {
        Route::Keychain { pem } => crate::setup::staged_cert(pem),
        Route::Certificate { pem } => crate::setup::staged_cert_as(pem, WINDOWS_CER),
        Route::EmulatorCertificate { pem } => crate::setup::staged_der(pem, WINDOWS_CER),
        _ => Ok(PathBuf::new()),
    }
}

fn succeeded(status_ok: bool, stderr: &str) -> bool {
    status_ok || stderr.contains(ALREADY_IN_KEYCHAIN)
}

const NO_AVD: &str = "no emulator to start — create one in Android Studio's Device Manager";

fn nothing_to_open(route: &Route) -> &'static str {
    match route {
        Route::Emulator => NO_AVD,
        Route::EmulatorRequest { .. }
        | Route::EmulatorCertificate { .. }
        | Route::TrustedCredentials => NO_EMULATOR_RUNNING,
        _ => NO_SIMULATOR_RUNNING,
    }
}

// Off the main thread: the Simulator's request reads xcode-select, may ask
// Spotlight, and runs simctl once per running Simulator; the emulator's run adb
// once per running emulator.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn open_route(route: Route) -> Result<(), SetupError> {
    let cert = staged(&route)?;
    let sims = match route {
        Route::SimulatorRequest { .. } => Simulators::read(),
        _ => Simulators::default(),
    };
    let emus = match route {
        Route::Emulator
        | Route::EmulatorRequest { .. }
        | Route::EmulatorCertificate { .. }
        | Route::TrustedCredentials => Emulators::read(),
        _ => Emulators::default(),
    };
    let commands = route_commands(std::env::consts::OS, &route, &cert, &sims, &emus)
        .ok_or_else(|| crate::setup::setup_error("this route does not open on this system"))?;
    if commands.is_empty() {
        return Err(crate::setup::setup_error(nothing_to_open(&route)));
    }
    if route == Route::Emulator {
        return started(&commands[0]);
    }
    for command in commands {
        run(&command, std::env::consts::OS)?;
    }
    Ok(())
}

// The emulator runs until the user closes it, so it is started and left, and
// reaped on a thread of its own when it exits.
fn started(command: &[String]) -> Result<(), SetupError> {
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| crate::setup::setup_error(e.to_string()))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

// A browser xdg-open starts holds every pipe it inherits until the user quits
// it, so on Linux the shell waits on the opener's exit alone.
fn run(command: &[String], os: &str) -> Result<(), SetupError> {
    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..]);
    if os == "linux" {
        let status = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| crate::setup::setup_error(e.to_string()))?;
        return if status.success() {
            Ok(())
        } else {
            Err(crate::setup::setup_error(format!("{} failed: {status}", command[0])))
        };
    }
    let out = cmd.output().map_err(|e| crate::setup::setup_error(e.to_string()))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if succeeded(out.status.success(), &stderr) {
        Ok(())
    } else {
        Err(crate::setup::setup_error(stderr.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert() -> PathBuf {
        PathBuf::from("/tmp/trawler-ca.pem")
    }

    fn opens(os: &str, route: &Route) -> Option<Vec<Vec<String>>> {
        route_commands(os, route, &cert(), &Simulators::default(), &Emulators::default())
    }

    #[test]
    fn macos_proxy_settings_opens_the_network_pane() {
        assert_eq!(
            opens("macos", &Route::ProxySettings),
            Some(vec![argv(&["open", MACOS_NETWORK_SETTINGS])])
        );
    }

    #[test]
    fn macos_keychain_adds_to_login_then_opens_keychain_access() {
        let route = Route::Keychain { pem: String::new() };
        assert_eq!(
            opens("macos", &route),
            Some(vec![
                argv(&[
                    "security",
                    "add-certificates",
                    "-k",
                    "login.keychain",
                    "/tmp/trawler-ca.pem"
                ]),
                argv(&["open", "-a", "Keychain Access"]),
            ])
        );
    }

    #[test]
    fn macos_browser_opens_the_marked_address() {
        let route = Route::Browser { mark: "k3m9x2.4".into() };
        assert_eq!(
            opens("macos", &route),
            Some(vec![argv(&["open", "https://example.com/?trawler-check=k3m9x2.4"])])
        );
    }

    #[test]
    fn a_mark_that_is_not_the_windows_own_opens_nothing() {
        for mark in ["", "k3m9", "k3m9.", ".4", "K3M9.4", "k3m9.4&x=1", "k3m9.4 --args", "../a.1"] {
            let route = Route::Browser { mark: mark.into() };
            assert_eq!(opens("macos", &route), None, "{mark}");
            assert_eq!(opens("windows", &route), None, "{mark}");
            assert_eq!(opens("linux", &route), None, "{mark}");
        }
    }

    #[test]
    fn macos_opens_the_simulator_app() {
        assert_eq!(
            opens("macos", &Route::Simulator),
            Some(vec![argv(&["open", "-a", "Simulator"])])
        );
    }

    #[test]
    fn the_simulators_request_opens_its_marked_address_on_every_running_simulator() {
        let sims = Simulators {
            developer_dir: Some("/Applications/Xcode.app/Contents/Developer".into()),
            running: vec!["A".into(), "B".into()],
        };
        let route = Route::SimulatorRequest { mark: "b6qv".into() };
        let open = |udid: &str| {
            argv(&[
                "env",
                "DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer",
                "xcrun",
                "simctl",
                "openurl",
                udid,
                "https://example.com/?trawler-check=b6qv",
            ])
        };
        assert_eq!(
            route_commands("macos", &route, &cert(), &sims, &Emulators::default()),
            Some(vec![open("A"), open("B")])
        );
        assert_eq!(opens("macos", &route), Some(vec![]));
    }

    #[test]
    fn a_device_mark_that_is_not_one_opens_nothing() {
        let sims = Simulators { developer_dir: None, running: vec!["A".into()] };
        for mark in ["", "B6QV", "b6qv&x=1", "b6qv --args", "../a", "k3m9x2.4", "123456789"] {
            let route = Route::SimulatorRequest { mark: mark.into() };
            assert_eq!(
                route_commands("macos", &route, &cert(), &sims, &Emulators::default()),
                None,
                "{mark}"
            );
        }
    }

    #[test]
    fn the_simulator_routes_open_on_macos_only() {
        for os in ["windows", "linux"] {
            assert_eq!(opens(os, &Route::Simulator), None);
        }
    }

    #[test]
    fn windows_proxy_settings_starts_the_proxy_page() {
        assert_eq!(
            opens("windows", &Route::ProxySettings),
            Some(vec![argv(&["cmd", "/C", "start", "", "ms-settings:network-proxy"])])
        );
    }

    #[test]
    fn windows_certificate_starts_the_saved_cer_in_its_own_dialog() {
        let route = Route::Certificate { pem: String::new() };
        let cer = PathBuf::from("C:\\Temp\\trawler-ca.cer");
        assert_eq!(
            route_commands("windows", &route, &cer, &Simulators::default(), &Emulators::default()),
            Some(vec![argv(&["cmd", "/C", "start", "", "C:\\Temp\\trawler-ca.cer"])])
        );
    }

    #[test]
    fn windows_browser_starts_the_marked_address() {
        let route = Route::Browser { mark: "k3m9x2.4".into() };
        assert_eq!(
            opens("windows", &route),
            Some(vec![argv(&[
                "cmd",
                "/C",
                "start",
                "",
                "https://example.com/?trawler-check=k3m9x2.4"
            ])])
        );
    }

    #[test]
    fn each_route_opens_only_on_the_host_it_was_written_for() {
        let keychain = Route::Keychain { pem: String::new() };
        let certificate = Route::Certificate { pem: String::new() };
        assert_eq!(opens("windows", &keychain), None);
        assert_eq!(opens("macos", &certificate), None);
        assert_eq!(opens("linux", &keychain), None);
        assert_eq!(opens("linux", &certificate), None);
        for route in [Route::ProxySettings, Route::Browser { mark: "a.1".into() }] {
            assert_eq!(opens("freebsd", &route), None);
        }
    }

    #[test]
    fn linux_proxy_settings_asks_gnome_settings_for_its_network_panel() {
        assert_eq!(opens("linux", &Route::ProxySettings), Some(vec![argv(&GNOME_NETWORK_PANEL)]));
    }

    #[test]
    fn linux_browser_opens_the_marked_address() {
        let route = Route::Browser { mark: "k3m9x2.4".into() };
        assert_eq!(
            opens("linux", &route),
            Some(vec![argv(&["xdg-open", "https://example.com/?trawler-check=k3m9x2.4"])])
        );
    }

    #[test]
    fn on_linux_an_opener_is_done_when_it_exits_though_what_it_started_runs_on() {
        let started = std::time::Instant::now();
        assert!(run(&argv(&["sh", "-c", "sleep 5 & exit 0"]), "linux").is_ok());
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn on_linux_an_opener_that_fails_is_an_error() {
        assert!(run(&argv(&["sh", "-c", "exit 3"]), "linux").is_err());
    }

    #[test]
    fn a_certificate_already_in_the_keychain_is_not_a_failure() {
        assert!(succeeded(false, "security: /tmp/trawler-ca.pem: already in login.keychain"));
        assert!(!succeeded(false, "security: SecKeychainItemImport: The user name or passphrase you entered is not correct."));
        assert!(succeeded(true, ""));
    }

    fn emulators() -> Emulators {
        Emulators {
            adb: "/sdk/platform-tools/adb".into(),
            emulator: "/sdk/emulator/emulator".into(),
            running: vec!["emulator-5554".into(), "emulator-5556".into()],
            last_avd: Some(crate::setup::Avd {
                id: "Pixel_7_API_35".into(),
                name: "Pixel 7 API 35".into(),
                play_store: false,
            }),
        }
    }

    fn on_emulators(os: &str, route: &Route) -> Option<Vec<Vec<String>>> {
        route_commands(os, route, &cert(), &Simulators::default(), &emulators())
    }

    fn adb(serial: &str, args: &[&str]) -> Vec<String> {
        argv(&[&["/sdk/platform-tools/adb", "-s", serial], args].concat())
    }

    #[test]
    fn the_emulators_request_opens_its_marked_address_in_every_running_emulator() {
        let route = Route::EmulatorRequest { mark: "b6qv".into() };
        let view = |serial: &str| {
            adb(
                serial,
                &[
                    "shell",
                    "am",
                    "start",
                    "-a",
                    "android.intent.action.VIEW",
                    "-d",
                    "'https://example.com/?trawler-check=b6qv'",
                ],
            )
        };
        for os in ["macos", "windows", "linux"] {
            assert_eq!(
                on_emulators(os, &route),
                Some(vec![view("emulator-5554"), view("emulator-5556")])
            );
        }
        for mark in ["", "B6QV", "b6qv'; reboot", "b6qv --args", "k3m9x2.4"] {
            let route = Route::EmulatorRequest { mark: mark.into() };
            assert_eq!(on_emulators("macos", &route), None, "{mark}");
        }
    }

    #[test]
    fn sending_the_certificate_pushes_it_to_downloads_then_opens_the_screen_that_lists_the_installer(
    ) {
        let route = Route::EmulatorCertificate { pem: String::new() };
        let sent = |serial: &str| {
            [
                adb(serial, &["push", "/tmp/trawler-ca.pem", "/sdcard/Download/trawler-ca.cer"]),
                adb(serial, &["shell", "am", "start", "-a", ANDROID_SECURITY_SETTINGS]),
            ]
        };
        assert_eq!(
            on_emulators("linux", &route),
            Some([sent("emulator-5554"), sent("emulator-5556")].concat())
        );
    }

    #[test]
    fn the_way_back_opens_the_user_tab_of_trusted_credentials() {
        let open =
            |serial: &str| adb(serial, &["shell", "am", "start", "-a", ANDROID_USER_CREDENTIALS]);
        assert_eq!(
            on_emulators("windows", &Route::TrustedCredentials),
            Some(vec![open("emulator-5554"), open("emulator-5556")])
        );
    }

    #[test]
    fn the_emulator_starts_the_avd_used_last_and_nothing_where_there_is_none() {
        assert_eq!(
            on_emulators("macos", &Route::Emulator),
            Some(vec![argv(&["/sdk/emulator/emulator", "-avd", "Pixel_7_API_35"])])
        );
        assert_eq!(opens("macos", &Route::Emulator), Some(vec![]));
        assert_eq!(nothing_to_open(&Route::Emulator), NO_AVD);
        assert_eq!(nothing_to_open(&Route::TrustedCredentials), NO_EMULATOR_RUNNING);
        assert_eq!(
            nothing_to_open(&Route::SimulatorRequest { mark: "a".into() }),
            NO_SIMULATOR_RUNNING
        );
    }

    #[test]
    fn with_no_emulator_running_each_device_route_opens_nothing() {
        for route in [
            Route::EmulatorRequest { mark: "b6qv".into() },
            Route::EmulatorCertificate { pem: String::new() },
            Route::TrustedCredentials,
        ] {
            assert_eq!(opens("macos", &route), Some(vec![]));
        }
    }

    #[test]
    fn a_started_emulator_is_left_running() {
        let begun = std::time::Instant::now();
        assert!(started(&argv(&["sh", "-c", "sleep 5"])).is_ok());
        assert!(begun.elapsed() < std::time::Duration::from_secs(3));
        assert!(started(&argv(&["/no/such/emulator"])).is_err());
    }

    #[test]
    fn only_the_named_routes_deserialize() {
        let parsed: Route =
            serde_json::from_str(r#"{"name":"emulator-request","mark":"b6qv"}"#).unwrap();
        assert_eq!(parsed, Route::EmulatorRequest { mark: "b6qv".into() });
        let parsed: Route = serde_json::from_str(r#"{"name":"trusted-credentials"}"#).unwrap();
        assert_eq!(parsed, Route::TrustedCredentials);
        let parsed: Route = serde_json::from_str(r#"{"name":"emulator","avd":"x"}"#).unwrap();
        assert_eq!(parsed, Route::Emulator, "the webview never names the AVD it starts");
        let parsed: Route = serde_json::from_str(r#"{"name":"proxy-settings"}"#).unwrap();
        assert_eq!(parsed, Route::ProxySettings);
        let parsed: Route = serde_json::from_str(r#"{"name":"browser","mark":"a.1"}"#).unwrap();
        assert_eq!(parsed, Route::Browser { mark: "a.1".into() });
        let parsed: Route = serde_json::from_str(r#"{"name":"certificate","pem":"p"}"#).unwrap();
        assert_eq!(parsed, Route::Certificate { pem: "p".into() });
        assert!(serde_json::from_str::<Route>(r#"{"name":"url","url":"https://evil"}"#).is_err());
    }
}
