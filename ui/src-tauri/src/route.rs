// A screen a by-hand step opens for the user (E-ONBRVP.W12, I1-I3). The set is
// closed and the shell builds every URL and path itself, so the webview names
// a route and never hands the shell something to open.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::setup::SetupError;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub enum Route {
    ProxySettings,
    Keychain { pem: String },
    Browser { mark: String },
}

const MACOS_NETWORK_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.Network-Settings.extension";
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

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(ToString::to_string).collect()
}

// Each route as the commands that open it, in order. The certificate is added
// to the login keychain first: opening the file instead asks which keychain,
// and the default it offers may be one that holds no trust (W12's recording).
fn route_commands(os: &str, route: &Route, cert: &Path) -> Option<Vec<Vec<String>>> {
    if os != "macos" {
        return None;
    }
    match route {
        Route::ProxySettings => Some(vec![argv(&["open", MACOS_NETWORK_SETTINGS])]),
        Route::Keychain { .. } => Some(vec![
            argv(&[
                "security",
                "add-certificates",
                "-k",
                "login.keychain",
                &cert.to_string_lossy(),
            ]),
            argv(&["open", "-a", "Keychain Access"]),
        ]),
        Route::Browser { mark } => {
            valid_mark(mark).then(|| vec![argv(&["open", &format!("{REQUEST_URL}{mark}")])])
        }
    }
}

fn staged(route: &Route) -> Result<PathBuf, SetupError> {
    match route {
        Route::Keychain { pem } => crate::setup::staged_cert(pem),
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
        let out = Command::new(&command[0])
            .args(&command[1..])
            .output()
            .map_err(|e| crate::setup::setup_error(e.to_string()))?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !succeeded(out.status.success(), &stderr) {
            return Err(crate::setup::setup_error(stderr.trim().to_string()));
        }
    }
    Ok(())
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
        }
    }

    #[test]
    fn no_route_opens_off_macos_yet() {
        for os in ["windows", "linux"] {
            assert_eq!(route_commands(os, &Route::ProxySettings, &cert()), None);
        }
    }

    #[test]
    fn a_certificate_already_in_the_keychain_is_not_a_failure() {
        assert!(succeeded(false, "security: /tmp/trawler-ca.pem: already in login.keychain"));
        assert!(!succeeded(false, "security: SecKeychainItemImport: The user name or passphrase you entered is not correct."));
        assert!(succeeded(true, ""));
    }

    #[test]
    fn only_the_three_routes_deserialize() {
        let parsed: Route = serde_json::from_str(r#"{"name":"proxy-settings"}"#).unwrap();
        assert_eq!(parsed, Route::ProxySettings);
        let parsed: Route = serde_json::from_str(r#"{"name":"browser","mark":"a.1"}"#).unwrap();
        assert_eq!(parsed, Route::Browser { mark: "a.1".into() });
        assert!(serde_json::from_str::<Route>(r#"{"name":"url","url":"https://evil"}"#).is_err());
    }
}
