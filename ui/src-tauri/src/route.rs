// A screen a by-hand step opens for the user (E-ONBRVP.W12, I1-I3). The set is
// closed and the shell builds every URL and path itself, so the webview names
// a route and never hands the shell something to open.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::setup::{argv, SetupError, LOGIN_KEYCHAIN};

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub enum Route {
    ProxySettings,
    Keychain { pem: String },
    Certificate { pem: String },
    Browser { mark: String },
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

// Each route as the commands that open it, in order. The certificate is added
// to the login keychain first: opening the file instead asks which keychain,
// and the default it offers may be one that holds no trust (W12's recording).
// Windows opens each through `start`, which returns once the screen is up: the
// certificate dialog runs inside its opener and would hold the command open.
// GNOME's Settings is asked over D-Bus for its Network panel, so it starts
// outside the shell's process.
fn route_commands(os: &str, route: &Route, cert: &Path) -> Option<Vec<Vec<String>>> {
    if let Route::Browser { mark } = route {
        if !valid_mark(mark) {
            return None;
        }
    }
    let cert = cert.to_string_lossy();
    match (os, route) {
        ("macos", Route::ProxySettings) => Some(vec![argv(&["open", MACOS_NETWORK_SETTINGS])]),
        ("macos", Route::Keychain { .. }) => Some(vec![
            argv(&["security", "add-certificates", "-k", LOGIN_KEYCHAIN, &cert]),
            argv(&["open", "-a", "Keychain Access"]),
        ]),
        ("macos", Route::Browser { mark }) => {
            Some(vec![argv(&["open", &format!("{REQUEST_URL}{mark}")])])
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
        _ => Ok(PathBuf::new()),
    }
}

fn succeeded(status_ok: bool, stderr: &str) -> bool {
    status_ok || stderr.contains(ALREADY_IN_KEYCHAIN)
}

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn open_route(route: Route) -> Result<(), SetupError> {
    let cert = staged(&route)?;
    let commands = route_commands(std::env::consts::OS, &route, &cert)
        .ok_or_else(|| crate::setup::setup_error("this route does not open on this system"))?;
    for command in commands {
        run(&command, std::env::consts::OS)?;
    }
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

    #[test]
    fn macos_proxy_settings_opens_the_network_pane() {
        assert_eq!(
            route_commands("macos", &Route::ProxySettings, &cert()),
            Some(vec![argv(&["open", MACOS_NETWORK_SETTINGS])])
        );
    }

    #[test]
    fn macos_keychain_adds_to_login_then_opens_keychain_access() {
        let route = Route::Keychain { pem: String::new() };
        assert_eq!(
            route_commands("macos", &route, &cert()),
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
            route_commands("macos", &route, &cert()),
            Some(vec![argv(&["open", "https://example.com/?trawler-check=k3m9x2.4"])])
        );
    }

    #[test]
    fn a_mark_that_is_not_the_windows_own_opens_nothing() {
        for mark in ["", "k3m9", "k3m9.", ".4", "K3M9.4", "k3m9.4&x=1", "k3m9.4 --args", "../a.1"] {
            let route = Route::Browser { mark: mark.into() };
            assert_eq!(route_commands("macos", &route, &cert()), None, "{mark}");
            assert_eq!(route_commands("windows", &route, &cert()), None, "{mark}");
            assert_eq!(route_commands("linux", &route, &cert()), None, "{mark}");
        }
    }

    #[test]
    fn windows_proxy_settings_starts_the_proxy_page() {
        assert_eq!(
            route_commands("windows", &Route::ProxySettings, &cert()),
            Some(vec![argv(&["cmd", "/C", "start", "", "ms-settings:network-proxy"])])
        );
    }

    #[test]
    fn windows_certificate_starts_the_saved_cer_in_its_own_dialog() {
        let route = Route::Certificate { pem: String::new() };
        let cer = PathBuf::from("C:\\Temp\\trawler-ca.cer");
        assert_eq!(
            route_commands("windows", &route, &cer),
            Some(vec![argv(&["cmd", "/C", "start", "", "C:\\Temp\\trawler-ca.cer"])])
        );
    }

    #[test]
    fn windows_browser_starts_the_marked_address() {
        let route = Route::Browser { mark: "k3m9x2.4".into() };
        assert_eq!(
            route_commands("windows", &route, &cert()),
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
        assert_eq!(route_commands("windows", &keychain, &cert()), None);
        assert_eq!(route_commands("macos", &certificate, &cert()), None);
        assert_eq!(route_commands("linux", &keychain, &cert()), None);
        assert_eq!(route_commands("linux", &certificate, &cert()), None);
        for route in [Route::ProxySettings, Route::Browser { mark: "a.1".into() }] {
            assert_eq!(route_commands("freebsd", &route, &cert()), None);
        }
    }

    #[test]
    fn linux_proxy_settings_asks_gnome_settings_for_its_network_panel() {
        assert_eq!(
            route_commands("linux", &Route::ProxySettings, &cert()),
            Some(vec![argv(&GNOME_NETWORK_PANEL)])
        );
    }

    #[test]
    fn linux_browser_opens_the_marked_address() {
        let route = Route::Browser { mark: "k3m9x2.4".into() };
        assert_eq!(
            route_commands("linux", &route, &cert()),
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

    #[test]
    fn only_the_named_routes_deserialize() {
        let parsed: Route = serde_json::from_str(r#"{"name":"proxy-settings"}"#).unwrap();
        assert_eq!(parsed, Route::ProxySettings);
        let parsed: Route = serde_json::from_str(r#"{"name":"browser","mark":"a.1"}"#).unwrap();
        assert_eq!(parsed, Route::Browser { mark: "a.1".into() });
        let parsed: Route = serde_json::from_str(r#"{"name":"certificate","pem":"p"}"#).unwrap();
        assert_eq!(parsed, Route::Certificate { pem: "p".into() });
        assert!(serde_json::from_str::<Route>(r#"{"name":"url","url":"https://evil"}"#).is_err());
    }
}
