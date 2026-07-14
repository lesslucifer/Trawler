// Trust boundary: the webview passes only PEM bytes; the privileged command is
// built here from fixed per-OS templates — never from IPC input.

use std::path::Path;
use std::process::Command;

fn install_command(os: &str, cert: &Path) -> Option<Vec<String>> {
    let cert = cert.to_string_lossy().into_owned();
    let argv: &[&str] = match os {
        "macos" => &[
            "security",
            "add-trusted-cert",
            "-d",
            "-r",
            "trustRoot",
            "-k",
            "/Library/Keychains/System.keychain",
        ],
        "windows" => &["certutil", "-addstore", "-f", "ROOT"],
        _ => return None,
    };
    let mut argv: Vec<String> = argv.iter().map(ToString::to_string).collect();
    argv.push(cert);
    Some(argv)
}

fn elevated_argv(os: &str, argv: &[String]) -> Vec<String> {
    match os {
        "macos" => {
            let script = argv.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" ");
            vec![
                "osascript".into(),
                "-e".into(),
                format!("do shell script \"{script}\" with administrator privileges"),
            ]
        }
        "windows" => {
            let args = argv[1..].iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(",");
            vec![
                "powershell".into(),
                "-NoProfile".into(),
                "-Command".into(),
                format!(
                    "$p = Start-Process -FilePath '{}' -ArgumentList {args} -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
                    argv[0]
                ),
            ]
        }
        _ => argv.to_vec(),
    }
}

#[tauri::command]
pub fn install_ca(pem: String) -> Result<(), String> {
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err("not a PEM certificate".into());
    }
    let os = std::env::consts::OS;
    let cert = std::env::temp_dir().join("trawler-ca.pem");
    let inner = install_command(os, &cert)
        .ok_or("no automatic install on this system — follow the guided steps")?;
    std::fs::write(&cert, &pem).map_err(|e| e.to_string())?;
    let argv = elevated_argv(os, &inner);
    let out = Command::new(&argv[0]).args(&argv[1..]).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(failure_message(&String::from_utf8_lossy(&out.stderr)))
    }
}

// The OS installer's own stderr is the only actionable signal on failure (e.g.
// macOS "authorization was denied since no user interaction was possible") — a
// bare exit code tells the user nothing, so surface the tool's message verbatim.
fn failure_message(stderr: &str) -> String {
    match stderr.trim() {
        "" => "install failed — the OS reported no detail; use the steps below".into(),
        detail => format!("install failed: {detail}"),
    }
}

const WININET_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

// `networksetup -listallnetworkservices` prints a header line then one service
// per line; a leading '*' marks a disabled service — those are skipped.
fn parse_services(list: &str) -> Vec<String> {
    list.lines()
        .skip(1)
        .filter(|l| !l.starts_with('*'))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn set_proxy_commands(
    os: &str,
    services: &[String],
    host: &str,
    port: u16,
) -> Option<Vec<Vec<String>>> {
    match os {
        "macos" => {
            let script = services
                .iter()
                .flat_map(|svc| {
                    let svc = shell_quote(svc);
                    [
                        format!("networksetup -setwebproxy {svc} {host} {port}"),
                        format!("networksetup -setsecurewebproxy {svc} {host} {port}"),
                    ]
                })
                .collect::<Vec<_>>()
                .join(" && ");
            Some(vec![vec![
                "osascript".into(),
                "-e".into(),
                format!("do shell script \"{script}\" with administrator privileges"),
            ]])
        }
        "windows" => Some(vec![
            reg_add("ProxyServer", "REG_SZ", &format!("{host}:{port}")),
            reg_add("ProxyEnable", "REG_DWORD", "1"),
            internet_set_option_refresh(),
        ]),
        _ => None,
    }
}

// Inverse of set_proxy_commands, but state-off keeps the saved address (matching
// setup.ts proxyOffSteps) rather than clearing it, so re-enabling is one toggle.
fn unset_proxy_commands(os: &str, services: &[String]) -> Option<Vec<Vec<String>>> {
    match os {
        "macos" => {
            let script = services
                .iter()
                .flat_map(|svc| {
                    let svc = shell_quote(svc);
                    [
                        format!("networksetup -setwebproxystate {svc} off"),
                        format!("networksetup -setsecurewebproxystate {svc} off"),
                    ]
                })
                .collect::<Vec<_>>()
                .join(" && ");
            Some(vec![vec![
                "osascript".into(),
                "-e".into(),
                format!("do shell script \"{script}\" with administrator privileges"),
            ]])
        }
        "windows" => {
            Some(vec![reg_add("ProxyEnable", "REG_DWORD", "0"), internet_set_option_refresh()])
        }
        _ => None,
    }
}

fn reg_add(value: &str, kind: &str, data: &str) -> Vec<String> {
    ["reg", "add", WININET_KEY, "/v", value, "/t", kind, "/d", data, "/f"]
        .map(ToString::to_string)
        .to_vec()
}

// SETTINGS_CHANGED (39) then REFRESH (37) so running WinINET apps pick the new
// proxy up now, not at their next settings poll.
fn internet_set_option_refresh() -> Vec<String> {
    vec![
        "powershell".into(),
        "-NoProfile".into(),
        "-Command".into(),
        concat!(
            "$s='[DllImport(\"wininet.dll\")]public static extern bool ",
            "InternetSetOption(IntPtr h,int o,IntPtr b,int l);';",
            "$t=Add-Type -MemberDefinition $s -Name W -Namespace I -PassThru;",
            "$t::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0);",
            "$t::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0)"
        )
        .into(),
    ]
}

// Service names come from networksetup output, not IPC, but still get quoted
// (names contain spaces, e.g. "Wi-Fi", "Thunderbolt Bridge") and single quotes
// escaped so they splice safely into the osascript shell string.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn macos_services() -> Result<Vec<String>, String> {
    let out = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .map_err(|e| e.to_string())?;
    let services = parse_services(&String::from_utf8_lossy(&out.stdout));
    if services.is_empty() {
        return Err("no network services to configure".into());
    }
    Ok(services)
}

fn run_all(commands: Vec<Vec<String>>, verb: &str) -> Result<(), String> {
    for argv in commands {
        let out = Command::new(&argv[0]).args(&argv[1..]).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            let detail = String::from_utf8_lossy(&out.stderr);
            return Err(match detail.trim() {
                "" => format!("couldn't {verb} the proxy — no detail; use the steps below"),
                d => format!("couldn't {verb} the proxy: {d}"),
            });
        }
    }
    Ok(())
}

// Presence of this file means "a Trawler run switched the system proxy on and
// hasn't turned it back off" — the sole gate on every revert path (W5). Temp
// survives a crash without threading a Tauri path into the pure builders; the
// off-commands re-derive everything else at run time, so no state is stored.
fn marker_path() -> std::path::PathBuf {
    std::env::temp_dir().join("trawler-proxy-set")
}

#[tauri::command]
pub fn set_proxy(host: String, port: u16) -> Result<(), String> {
    if !valid_host(&host) {
        return Err("invalid proxy host".into());
    }
    if port == 0 {
        return Err("invalid proxy port".into());
    }
    let os = std::env::consts::OS;
    let services = if os == "macos" { macos_services()? } else { Vec::new() };
    let commands = set_proxy_commands(os, &services, &host, port)
        .ok_or("no automatic proxy setup on this system — follow the guided steps")?;
    run_all(commands, "set")?;
    let _ = std::fs::write(marker_path(), []);
    Ok(())
}

#[tauri::command]
pub fn unset_proxy() -> Result<(), String> {
    let os = std::env::consts::OS;
    let services = if os == "macos" { macos_services()? } else { Vec::new() };
    let commands = unset_proxy_commands(os, &services)
        .ok_or("no automatic proxy setup on this system — follow the guided steps")?;
    run_all(commands, "turn off")?;
    let _ = std::fs::remove_file(marker_path());
    Ok(())
}

// Launch reconciliation: revert iff a prior run set the proxy and died without
// reverting, then clear the marker so a revert can never touch a proxy Trawler
// didn't set. Best-effort — the result is ignored so a dead-proxy revert failure
// never blocks launch; the OS off-command can spawn an elevation prompt, so this
// blocks until it returns and is run off the main thread (see lib.rs). The
// clean-quit path is user-driven in the webview instead (App.tsx onCloseRequested).
pub fn revert_if_marked() {
    if marker_path().exists() {
        let _ = unset_proxy();
    }
}

// The actual system proxy state, classified against Trawler's own address — the
// quit-time confirm keys off this, NOT off whether we set it this session, so a
// proxy the user pointed at Trawler by hand is caught too. "ours" ⇒ an active
// proxy on this system routes through Trawler (Remove offered); "foreign" ⇒ some
// other active proxy (only the remote-device notice); "none" ⇒ this system is
// clean (still the remote-device notice — a device elsewhere may point here).
#[derive(Debug, PartialEq, Eq)]
enum ProxyStatus {
    Ours,
    Foreign,
    None,
}

impl ProxyStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ProxyStatus::Ours => "ours",
            ProxyStatus::Foreign => "foreign",
            ProxyStatus::None => "none",
        }
    }
}

// `networksetup -get{,secure}webproxy <svc>` prints `Enabled: Yes/No`, `Server: …`,
// `Port: …`. Returns the active server:port, or None when disabled/absent.
fn parse_active_proxy(out: &str) -> Option<(String, u16)> {
    let mut enabled = false;
    let mut server = None;
    let mut port = None;
    for line in out.lines() {
        let Some((key, val)) = line.split_once(':') else { continue };
        match key.trim() {
            "Enabled" => enabled = val.trim().eq_ignore_ascii_case("yes"),
            "Server" => server = Some(val.trim().to_string()),
            "Port" => port = val.trim().parse().ok(),
            _ => {}
        }
    }
    match (enabled, server, port) {
        (true, Some(s), Some(p)) if !s.is_empty() => Some((s, p)),
        _ => None,
    }
}

fn classify(active: Option<(String, u16)>, host: &str, port: u16) -> ProxyStatus {
    match active {
        Some((s, p)) if s == host && p == port => ProxyStatus::Ours,
        Some(_) => ProxyStatus::Foreign,
        None => ProxyStatus::None,
    }
}

fn active_system_proxy(os: &str) -> Option<(String, u16)> {
    match os {
        // Check HTTPS (securewebproxy) first, then HTTP — Trawler sets both, but a
        // hand-configured proxy or one externally half-cleared could have only one
        // on; catching either keeps a routing proxy from being missed.
        "macos" => macos_services().ok()?.into_iter().find_map(|svc| {
            let read = |flag: &str| {
                let out = Command::new("networksetup").args([flag, &svc]).output().ok()?;
                parse_active_proxy(&String::from_utf8_lossy(&out.stdout))
            };
            read("-getsecurewebproxy").or_else(|| read("-getwebproxy"))
        }),
        "windows" => {
            let out = Command::new("reg")
                .args(["query", WININET_KEY, "/v", "ProxyEnable"])
                .output()
                .ok()?;
            let on = String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.contains("ProxyEnable"))
                .and_then(|l| l.rsplit("0x").next())
                .map(|hex| hex.trim() != "0")
                .unwrap_or(false);
            if !on {
                return None;
            }
            let out = Command::new("reg")
                .args(["query", WININET_KEY, "/v", "ProxyServer"])
                .output()
                .ok()?;
            let server = String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.contains("ProxyServer"))
                .and_then(|l| l.split_whitespace().last().map(ToString::to_string))?;
            let (h, p) = server.rsplit_once(':')?;
            Some((h.to_string(), p.parse().ok()?))
        }
        _ => None,
    }
}

// The quit-time confirm in App.tsx calls this with Trawler's configured address to
// decide which of the three quit paths to take. Read-only, no elevation.
#[tauri::command]
pub fn proxy_status(host: String, port: u16) -> String {
    classify(active_system_proxy(std::env::consts::OS), &host, port).as_str().into()
}

// The remote-device notice's "don't show again" is a durable user preference, so
// it lives in the app config dir (survives reboots), NOT temp like the set-marker.
// The file logic is pure (dir injected) so it stays unit-testable off a real app.
fn notice_dismissed_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join("remote-notice-dismissed")
}

fn notice_is_dismissed(config_dir: &Path) -> bool {
    notice_dismissed_path(config_dir).exists()
}

fn mark_notice_dismissed(config_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(notice_dismissed_path(config_dir), [])
}

fn config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    tauri::Manager::path(app).app_config_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remote_notice_dismissed(app: tauri::AppHandle) -> bool {
    config_dir(&app).map(|d| notice_is_dismissed(&d)).unwrap_or(false)
}

#[tauri::command]
pub fn dismiss_remote_notice(app: tauri::AppHandle) {
    if let Ok(dir) = config_dir(&app) {
        let _ = mark_notice_dismissed(&dir);
    }
}

// The user chose to keep the proxy on quit: drop the marker without touching the
// proxy, so their choice is durable and the next launch won't reconcile it away.
#[tauri::command]
pub fn clear_proxy_marker() {
    let _ = std::fs::remove_file(marker_path());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    // The marker is one shared temp path, so tests that mutate it serialize here.
    static MARKER_LOCK: Mutex<()> = Mutex::new(());

    fn cert() -> PathBuf {
        PathBuf::from("/tmp/trawler-ca.pem")
    }

    #[test]
    fn macos_argv_targets_the_system_keychain() {
        assert_eq!(
            install_command("macos", &cert()).unwrap(),
            [
                "security",
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
                "-k",
                "/Library/Keychains/System.keychain",
                "/tmp/trawler-ca.pem",
            ]
            .map(String::from)
        );
    }

    #[test]
    fn windows_argv_targets_the_root_store() {
        assert_eq!(
            install_command("windows", &cert()).unwrap(),
            ["certutil", "-addstore", "-f", "ROOT", "/tmp/trawler-ca.pem"].map(String::from)
        );
    }

    #[test]
    fn other_systems_have_no_auto_path() {
        assert_eq!(install_command("linux", &cert()), None);
        assert_eq!(install_command("freebsd", &cert()), None);
    }

    #[test]
    fn macos_elevation_wraps_the_command_in_osascript() {
        let argv = elevated_argv("macos", &["security".into(), "-k".into()]);
        assert_eq!(argv[0], "osascript");
        assert_eq!(argv[1], "-e");
        assert_eq!(argv[2], "do shell script \"'security' '-k'\" with administrator privileges");
    }

    #[test]
    fn windows_elevation_wraps_the_command_in_a_uac_start_process() {
        let argv = elevated_argv("windows", &["certutil".into(), "-f".into(), "ROOT".into()]);
        assert_eq!(argv[0], "powershell");
        assert!(argv[3]
            .contains("Start-Process -FilePath 'certutil' -ArgumentList '-f','ROOT' -Verb RunAs"));
    }

    #[test]
    fn failure_message_surfaces_the_os_installer_stderr() {
        let msg = failure_message(
            "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (1)\n",
        );
        assert!(msg.contains("authorization was denied"));
    }

    #[test]
    fn failure_message_falls_back_when_stderr_is_empty() {
        assert!(failure_message("   ").contains("use the steps below"));
    }

    #[test]
    fn install_ca_rejects_non_certificate_input() {
        let err = install_ca("BEGIN RSA PRIVATE KEY".into()).unwrap_err();
        assert!(err.contains("not a PEM certificate"));
    }

    #[test]
    fn parse_services_drops_the_header_and_disabled_entries() {
        let list = "An asterisk (*) denotes that a network service is disabled.\n\
                    Wi-Fi\n\
                    *Thunderbolt Bridge\n\
                    Ethernet\n";
        assert_eq!(parse_services(list), ["Wi-Fi", "Ethernet"].map(String::from));
    }

    #[test]
    fn macos_set_proxy_is_one_elevated_script_covering_every_service() {
        let svcs = ["Wi-Fi".into(), "Thunderbolt Bridge".into()];
        let cmds = set_proxy_commands("macos", &svcs, "127.0.0.1", 7420).unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0][0], "osascript");
        let script = &cmds[0][2];
        assert!(script.contains("with administrator privileges"));
        assert!(script.contains("networksetup -setwebproxy 'Wi-Fi' 127.0.0.1 7420"));
        assert!(script.contains("networksetup -setsecurewebproxy 'Wi-Fi' 127.0.0.1 7420"));
        assert!(script.contains("networksetup -setwebproxy 'Thunderbolt Bridge' 127.0.0.1 7420"));
        assert!(script.contains("networksetup -setsecurewebproxy 'Thunderbolt Bridge' 127.0.0.1 7420"));
    }

    #[test]
    fn windows_set_proxy_writes_hkcu_and_refreshes_without_elevation() {
        let cmds = set_proxy_commands("windows", &[], "127.0.0.1", 7420).unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0], reg_add("ProxyServer", "REG_SZ", "127.0.0.1:7420"));
        assert_eq!(cmds[1], reg_add("ProxyEnable", "REG_DWORD", "1"));
        assert_eq!(cmds[2][0], "powershell");
        assert!(cmds[2].last().unwrap().contains("InternetSetOption"));
        for cmd in &cmds {
            assert_ne!(cmd[0], "osascript");
            assert!(!cmd.iter().any(|a| a.contains("RunAs")));
        }
    }

    #[test]
    fn other_systems_have_no_auto_proxy_path() {
        assert_eq!(set_proxy_commands("linux", &[], "127.0.0.1", 7420), None);
    }

    #[test]
    fn set_proxy_rejects_hosts_with_shell_metacharacters() {
        for bad in ["127.0.0.1 x", "a;rm -rf", "a'b", "a\"b", "$(x)", ""] {
            assert!(set_proxy(bad.into(), 7420).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn set_proxy_rejects_port_zero() {
        assert!(set_proxy("127.0.0.1".into(), 0).unwrap_err().contains("port"));
    }

    #[test]
    fn macos_unset_turns_state_off_per_service_keeping_the_address() {
        let svcs = ["Wi-Fi".into(), "Thunderbolt Bridge".into()];
        let cmds = unset_proxy_commands("macos", &svcs).unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0][0], "osascript");
        let script = &cmds[0][2];
        assert!(script.contains("with administrator privileges"));
        assert!(script.contains("networksetup -setwebproxystate 'Wi-Fi' off"));
        assert!(script.contains("networksetup -setsecurewebproxystate 'Wi-Fi' off"));
        assert!(script.contains("networksetup -setwebproxystate 'Thunderbolt Bridge' off"));
        assert!(script.contains("networksetup -setsecurewebproxystate 'Thunderbolt Bridge' off"));
        assert!(!script.contains("127.0.0.1"), "off keeps the saved address, never rewrites it");
    }

    #[test]
    fn windows_unset_disables_proxyenable_and_refreshes_without_elevation() {
        let cmds = unset_proxy_commands("windows", &[]).unwrap();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], reg_add("ProxyEnable", "REG_DWORD", "0"));
        assert_eq!(cmds[1][0], "powershell");
        assert!(cmds[1].last().unwrap().contains("InternetSetOption"));
        for cmd in &cmds {
            assert_ne!(cmd[0], "osascript");
            assert!(!cmd.iter().any(|a| a.contains("RunAs")));
        }
    }

    #[test]
    fn other_systems_have_no_auto_unset_path() {
        assert_eq!(unset_proxy_commands("linux", &[]), None);
    }

    #[test]
    fn revert_is_a_no_op_when_the_marker_is_absent() {
        let _guard = MARKER_LOCK.lock().unwrap();
        let marker = marker_path();
        let _ = std::fs::remove_file(&marker);
        revert_if_marked();
        assert!(!marker.exists(), "no marker ⇒ nothing to revert, nothing created");
    }

    #[test]
    fn clear_proxy_marker_drops_the_marker_without_reverting() {
        let _guard = MARKER_LOCK.lock().unwrap();
        let marker = marker_path();
        std::fs::write(&marker, []).unwrap();
        clear_proxy_marker();
        assert!(!marker.exists(), "clear drops the marker (proxy itself untouched — no OS command run)");
    }

    #[test]
    fn parse_active_proxy_reads_enabled_server_port() {
        let on = "Enabled: Yes\nServer: 127.0.0.1\nPort: 7420\nAuthenticated Proxy Enabled: 0\n";
        assert_eq!(parse_active_proxy(on), Some(("127.0.0.1".into(), 7420)));
    }

    #[test]
    fn parse_active_proxy_is_none_when_disabled_or_empty() {
        assert_eq!(parse_active_proxy("Enabled: No\nServer: 127.0.0.1\nPort: 7420\n"), None);
        assert_eq!(parse_active_proxy("Enabled: Yes\nServer: \nPort: 7420\n"), None);
        assert_eq!(parse_active_proxy(""), None);
    }

    #[test]
    fn classify_maps_the_active_proxy_against_trawlers_address() {
        let ours = Some(("127.0.0.1".to_string(), 7420));
        assert_eq!(classify(ours, "127.0.0.1", 7420), ProxyStatus::Ours);
        let other_port = Some(("127.0.0.1".to_string(), 8080));
        assert_eq!(classify(other_port, "127.0.0.1", 7420), ProxyStatus::Foreign);
        let other_host = Some(("10.0.0.1".to_string(), 7420));
        assert_eq!(classify(other_host, "127.0.0.1", 7420), ProxyStatus::Foreign);
        assert_eq!(classify(None, "127.0.0.1", 7420), ProxyStatus::None);
    }

    #[test]
    fn remote_notice_dismissal_persists_in_the_given_config_dir() {
        let dir = std::env::temp_dir().join("trawler-test-notice-cfg");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!notice_is_dismissed(&dir), "fresh install ⇒ notice shows");
        mark_notice_dismissed(&dir).unwrap();
        assert!(notice_is_dismissed(&dir), "don't-show-again persists (created the dir)");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
