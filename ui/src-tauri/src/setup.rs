// Trust boundary: the webview passes only PEM bytes; the privileged command is
// built here from fixed per-OS templates — never from IPC input.

use std::collections::HashMap;
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

fn quoted(argv: &[String]) -> String {
    argv.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" ")
}

fn elevated_script(script: &str) -> Vec<String> {
    vec![
        "osascript".into(),
        "-e".into(),
        format!("do shell script \"{script}\" with administrator privileges"),
    ]
}

fn elevated_argv(os: &str, argv: &[String]) -> Vec<String> {
    match os {
        "macos" => elevated_script(&quoted(argv)),
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

// Why an elevated command did not run, split into the one fact the screen words
// for itself and the tool's own text. The OS installer's stderr is still the
// only actionable signal on a real failure, so it travels whole — but a prompt
// the user dismissed is not a failure to report in the OS's words, and `refused`
// is what lets the screen say so (F18).
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct SetupError {
    refused: bool,
    detail: String,
}

// macOS reports a dismissed authorization dialog as `User cancelled. (-128)` and
// one it could not show at all as `(-60007)`; Windows' UAC refusal comes back as
// "The operation was canceled by the user". All three are the same fact: nothing
// on this machine changed, and the manual route is still there.
fn refused_prompt(stderr: &str) -> bool {
    let text = stderr.to_ascii_lowercase();
    text.contains("-128") || text.contains("-60007") || text.contains("cancel")
}

pub(crate) fn command_failed(stderr: &str, no_detail: &str) -> SetupError {
    let detail = stderr.trim();
    SetupError {
        refused: refused_prompt(detail),
        detail: if detail.is_empty() { no_detail.to_string() } else { detail.to_string() },
    }
}

fn setup_error(detail: impl Into<String>) -> SetupError {
    SetupError { refused: false, detail: detail.into() }
}

fn run_elevated(argv: &[String], no_detail: &str) -> Result<(), SetupError> {
    let out =
        Command::new(&argv[0]).args(&argv[1..]).output().map_err(|e| setup_error(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(command_failed(&String::from_utf8_lossy(&out.stderr), no_detail))
    }
}

fn staged_cert(pem: &str) -> Result<std::path::PathBuf, SetupError> {
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err(setup_error("not a PEM certificate"));
    }
    let cert = std::env::temp_dir().join("trawler-ca.pem");
    std::fs::write(&cert, pem).map_err(|e| setup_error(e.to_string()))?;
    Ok(cert)
}

pub(crate) const NO_INSTALL_DETAIL: &str =
    "install failed — the OS reported no detail; use the steps below";

// Trawler's CA is issued with this CN; the store dumps below are matched on it,
// the same key W2's removal steps use.
const CA_COMMON_NAME: &str = "Trawler Proxy CA";

// Read-only probe: is Trawler's CA in this machine's system trust store? Feeds
// the onboarding Trust step so the ~90% local case gets an instant, honest
// status instead of waiting on a captured flow. macOS/Windows only — the same
// systems that have an auto-install path; elsewhere we can't conclude.
//
// ponytail: matches on the CA's Common Name, not a key fingerprint. A stale cert
// left over from an earlier run with the same CN would read as trusted. Upgrade
// path if that bites: compute the SHA-256 of the live /ca.crt DER (add `sha2`)
// and match the store's fingerprint line instead of the CN.
fn ca_is_trusted(os: &str) -> Option<bool> {
    match os {
        "macos" => {
            let out = Command::new("security")
                .args(["find-certificate", "-a", "-c", CA_COMMON_NAME])
                .output()
                .ok()?;
            Some(out.status.success() && !out.stdout.is_empty())
        }
        "windows" => {
            let out = Command::new("certutil").args(["-store", "ROOT"]).output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).contains(CA_COMMON_NAME))
        }
        _ => None,
    }
}

// "trusted" / "untrusted" ⇒ probed this machine and concluded; "unknown" ⇒ no
// reliable probe on this OS, so the UI falls back to capture-will-confirm wording.
#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn ca_trusted() -> String {
    trust_word(ca_is_trusted(std::env::consts::OS)).into()
}

pub(crate) fn trust_word(reading: Option<bool>) -> &'static str {
    match reading {
        Some(true) => "trusted",
        Some(false) => "untrusted",
        None => "unknown",
    }
}

// The kernel calls a device `en1`; its owner calls it Wi-Fi, in the same
// Settings pane they are about to open. Only the host can map one to the other,
// and only the shell may ask it — the core is kept out of shelling to the OS
// (DEC-13, and E-ONBOARD.W1's reading of it when it took if-addrs instead).
// A host with no answer renames nothing, which leaves the device name standing.
#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn interface_names() -> HashMap<String, String> {
    hardware_ports(std::env::consts::OS)
}

fn hardware_ports(os: &str) -> HashMap<String, String> {
    if os != "macos" {
        return HashMap::new();
    }
    Command::new("networksetup")
        .arg("-listallhardwareports")
        .output()
        .ok()
        .filter(|listing| listing.status.success())
        .map(|listing| parse_hardware_ports(&String::from_utf8_lossy(&listing.stdout)))
        .unwrap_or_default()
}

// `networksetup -listallhardwareports` prints a stanza per port: the port's
// name, then its device, then its MAC. The pairing is positional, so a stanza
// that names a port and never names a device contributes nothing.
fn parse_hardware_ports(listing: &str) -> HashMap<String, String> {
    let mut ports = HashMap::new();
    let mut port: Option<&str> = None;
    for line in listing.lines() {
        if let Some(name) = line.strip_prefix("Hardware Port:") {
            port = Some(name.trim());
        } else if let Some(device) = line.strip_prefix("Device:") {
            if let Some(name) = port.take() {
                ports.insert(device.trim().to_owned(), name.to_owned());
            }
        }
    }
    ports
}

const WININET_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

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

fn macos_proxy_script(services: &[String], host: &str, port: u16) -> String {
    services
        .iter()
        .flat_map(|svc| {
            let svc = shell_quote(svc);
            [
                format!("networksetup -setwebproxy {svc} {host} {port}"),
                format!("networksetup -setsecurewebproxy {svc} {host} {port}"),
            ]
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn set_proxy_commands(
    os: &str,
    services: &[String],
    host: &str,
    port: u16,
) -> Option<Vec<Vec<String>>> {
    match os {
        "macos" => Some(vec![elevated_script(&macos_proxy_script(services, host, port))]),
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
            Some(vec![elevated_script(&script)])
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
    !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn macos_services() -> Result<Vec<String>, SetupError> {
    let out = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .map_err(|e| setup_error(e.to_string()))?;
    let services = parse_services(&String::from_utf8_lossy(&out.stdout));
    if services.is_empty() {
        return Err(setup_error("no network services to configure"));
    }
    Ok(services)
}

fn services_to_configure(os: &str) -> Result<Vec<String>, SetupError> {
    if os == "macos" {
        macos_services()
    } else {
        Ok(Vec::new())
    }
}

fn run_all(commands: Vec<Vec<String>>, no_detail: &str) -> Result<(), SetupError> {
    for argv in commands {
        run_elevated(&argv, no_detail)?;
    }
    Ok(())
}

fn checked_address(host: &str, port: u16) -> Result<(), SetupError> {
    if !valid_host(host) {
        return Err(setup_error("invalid proxy host"));
    }
    if port == 0 {
        return Err(setup_error("invalid proxy port"));
    }
    Ok(())
}

const NO_PROXY_AUTO: &str = "no automatic proxy setup on this system — follow the guided steps";
pub(crate) const NO_SET_PROXY_DETAIL: &str =
    "couldn't set the proxy — no detail; use the steps below";
pub(crate) const NO_UNSET_PROXY_DETAIL: &str =
    "couldn't turn the proxy off — no detail; use the steps below";

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn set_proxy(host: String, port: u16) -> Result<(), SetupError> {
    checked_address(&host, port)?;
    let os = std::env::consts::OS;
    let services = services_to_configure(os)?;
    let commands =
        set_proxy_commands(os, &services, &host, port).ok_or_else(|| setup_error(NO_PROXY_AUTO))?;
    run_all(commands, NO_SET_PROXY_DETAIL)
}

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn unset_proxy() -> Result<(), SetupError> {
    let os = std::env::consts::OS;
    let services = services_to_configure(os)?;
    let commands = unset_proxy_commands(os, &services).ok_or_else(|| setup_error(NO_PROXY_AUTO))?;
    run_all(commands, NO_UNSET_PROXY_DETAIL)
}

// The one thing most users want, and the one prompt it is worth: on macOS the
// trust write and the proxy write go inside a single `do shell script … with
// administrator privileges`, joined so that a refused trust never leaves the
// machine routing through a proxy it does not trust. On Windows only the store
// write is elevated — the proxy lives under HKCU — so one UAC prompt is already
// the whole cost and the commands run in sequence (F27).
fn set_up_commands(
    os: &str,
    cert: &Path,
    services: &[String],
    host: &str,
    port: u16,
) -> Option<Vec<Vec<String>>> {
    let install = install_command(os, cert)?;
    match os {
        "macos" => {
            let script =
                format!("{} && {}", quoted(&install), macos_proxy_script(services, host, port));
            Some(vec![elevated_script(&script)])
        }
        "windows" => {
            let mut commands = vec![elevated_argv(os, &install)];
            commands.extend(set_proxy_commands(os, services, host, port)?);
            Some(commands)
        }
        _ => None,
    }
}

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn set_up_this_machine(pem: String, host: String, port: u16) -> Result<(), SetupError> {
    checked_address(&host, port)?;
    let os = std::env::consts::OS;
    let cert = staged_cert(&pem)?;
    let services = services_to_configure(os)?;
    let commands = set_up_commands(os, &cert, &services, &host, port).ok_or_else(|| {
        setup_error("no automatic setup on this system — follow the guided steps")
    })?;
    run_all(commands, NO_INSTALL_DETAIL)
}

// The actual system proxy state, classified against Trawler's own address — the
// quit-time confirm keys off this, NOT off whether we set it this session, so a
// proxy the user pointed at Trawler by hand is caught too. "ours" ⇒ an active
// proxy on this system routes through Trawler; "foreign" ⇒ some other active
// proxy on this system — also actionable, since unset_proxy turns the OS proxy
// state off without reading whose address is set; "none" ⇒ this system is clean
// (the remote-device notice — a device elsewhere may point here); "unknown" ⇒
// nothing here could read it, which quit treats as "none" and setup words as a
// promise rather than a false "off" (R19).
#[derive(Debug, PartialEq, Eq)]
enum ProxyStatus {
    Ours,
    Foreign,
    None,
    Unknown,
}

impl ProxyStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ProxyStatus::Ours => "ours",
            ProxyStatus::Foreign => "foreign",
            ProxyStatus::None => "none",
            ProxyStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ProxyRead {
    Set(String, u16),
    Off,
    Unreadable,
}

// A command's outcome detached from its process, so every branch of a reader
// runs in a unit test without the host's tools.
struct Ran {
    ok: bool,
    stdout: String,
}

fn ran(program: &str, args: &[&str]) -> Option<Ran> {
    let out = Command::new(program).args(args).output().ok()?;
    Some(Ran {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    })
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

fn classify(read: &ProxyRead, host: &str, port: u16) -> ProxyStatus {
    match read {
        ProxyRead::Set(s, p) if s == host && *p == port => ProxyStatus::Ours,
        ProxyRead::Set(..) => ProxyStatus::Foreign,
        ProxyRead::Off => ProxyStatus::None,
        ProxyRead::Unreadable => ProxyStatus::Unknown,
    }
}

// HTTPS (securewebproxy) is read before HTTP — Trawler sets both, but a
// hand-configured proxy or one externally half-cleared could have only one on,
// and catching either keeps a routing proxy from being missed. A read that
// failed means the services that did answer cannot vouch for a clean system.
fn macos_proxy(
    listing: Option<&Ran>,
    mut read: impl FnMut(&str, &str) -> Option<Ran>,
) -> ProxyRead {
    let services = match listing {
        Some(listing) if listing.ok => parse_services(&listing.stdout),
        _ => return ProxyRead::Unreadable,
    };
    if services.is_empty() {
        return ProxyRead::Unreadable;
    }
    let mut unreadable = false;
    for svc in &services {
        for flag in ["-getsecurewebproxy", "-getwebproxy"] {
            match read(flag, svc) {
                Some(out) if out.ok => {
                    if let Some((host, port)) = parse_active_proxy(&out.stdout) {
                        return ProxyRead::Set(host, port);
                    }
                }
                _ => unreadable = true,
            }
        }
    }
    if unreadable {
        ProxyRead::Unreadable
    } else {
        ProxyRead::Off
    }
}

// `reg query` exits non-zero for a value never written, and a ProxyEnable never
// written is a proxy never enabled — so only `reg` failing to run leaves the
// switch unread. A server not in the one shape the setter writes (host:port),
// such as a per-scheme list, is one this reader cannot place.
fn windows_proxy(enable: Option<&Ran>, server: impl FnOnce() -> Option<Ran>) -> ProxyRead {
    let Some(enable) = enable else { return ProxyRead::Unreadable };
    let on = enable
        .stdout
        .lines()
        .find(|l| l.contains("ProxyEnable"))
        .and_then(|l| l.rsplit("0x").next())
        .is_some_and(|hex| hex.trim() != "0");
    if !on {
        return ProxyRead::Off;
    }
    server()
        .and_then(|out| {
            out.stdout
                .lines()
                .find(|l| l.contains("ProxyServer"))
                .and_then(|l| l.split_whitespace().last())
                .and_then(host_and_port)
        })
        .map_or(ProxyRead::Unreadable, |(host, port)| ProxyRead::Set(host, port))
}

fn host_and_port(server: &str) -> Option<(String, u16)> {
    let (host, port) = server.rsplit_once(':')?;
    if host.is_empty() || host.contains(['=', ';']) {
        return None;
    }
    Some((host.to_string(), port.parse().ok()?))
}

fn active_system_proxy(os: &str) -> ProxyRead {
    match os {
        "macos" => {
            macos_proxy(ran("networksetup", &["-listallnetworkservices"]).as_ref(), |flag, svc| {
                ran("networksetup", &[flag, svc])
            })
        }
        "windows" => {
            windows_proxy(ran("reg", &["query", WININET_KEY, "/v", "ProxyEnable"]).as_ref(), || {
                ran("reg", &["query", WININET_KEY, "/v", "ProxyServer"])
            })
        }
        _ => ProxyRead::Unreadable,
    }
}

// One read, both readers. The quit-time confirm calls this with Trawler's
// configured address to decide which of the three quit paths to take; the setup
// screen needs the same reading plus the address it found, because a button
// that replaces a proxy the user set may not hide which one (F17). Read-only,
// no elevation, and the address is None wherever nothing is set or this build
// cannot read the host's settings.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct ProxyReading {
    status: &'static str,
    address: Option<String>,
}

pub(crate) fn read_proxy(read: &ProxyRead, host: &str, port: u16) -> ProxyReading {
    let address = match read {
        ProxyRead::Set(h, p) => Some(format!("{h}:{p}")),
        ProxyRead::Off | ProxyRead::Unreadable => None,
    };
    ProxyReading { status: classify(read, host, port).as_str(), address }
}

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn proxy_status(host: String, port: u16) -> ProxyReading {
    read_proxy(&active_system_proxy(std::env::consts::OS), &host, port)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NoticeKey {
    RemoteNotice,
    ClearAsk,
}

impl TryFrom<&str> for NoticeKey {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "remote-notice" => Ok(Self::RemoteNotice),
            "clear-ask" => Ok(Self::ClearAsk),
            _ => Err(format!("unknown notice key: {value}")),
        }
    }
}

fn notice_dismissed_path(config_dir: &Path, key: NoticeKey) -> std::path::PathBuf {
    let filename = match key {
        NoticeKey::RemoteNotice => "remote-notice-dismissed",
        NoticeKey::ClearAsk => "clear-ask-dismissed",
    };
    config_dir.join(filename)
}

fn notice_is_dismissed(config_dir: &Path, key: NoticeKey) -> bool {
    notice_dismissed_path(config_dir, key).exists()
}

fn mark_notice_dismissed(config_dir: &Path, key: NoticeKey) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(notice_dismissed_path(config_dir, key), [])
}

fn config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    tauri::Manager::path(app).app_config_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notice_dismissed(app: tauri::AppHandle, key: String) -> Result<bool, String> {
    let key = NoticeKey::try_from(key.as_str())?;
    Ok(config_dir(&app).map(|dir| notice_is_dismissed(&dir, key)).unwrap_or(false))
}

#[tauri::command]
pub fn dismiss_notice(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let key = NoticeKey::try_from(key.as_str())?;
    let dir = config_dir(&app)?;
    mark_notice_dismissed(&dir, key).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn a_real_failure_carries_the_os_installers_own_stderr_and_is_not_a_refusal() {
        let err = command_failed(
            "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied because of an unknown error. (1)\n",
            NO_INSTALL_DETAIL,
        );
        assert!(!err.refused);
        assert!(err.detail.contains("authorization was denied"));
    }

    #[test]
    fn a_failure_with_no_stderr_falls_back_to_the_route_that_still_works() {
        assert!(command_failed("   ", NO_INSTALL_DETAIL).detail.contains("use the steps below"));
    }

    // The three ways the OS says "you were asked and nothing happened". Each
    // one buys the screen a sentence of its own instead of the tool's text.
    #[test]
    fn a_dismissed_or_unshowable_prompt_is_a_refusal_in_every_wording_the_os_uses() {
        for stderr in [
            "0:36: execution error: User cancelled. (-128)",
            "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (-60007)",
            "Start-Process : The operation was canceled by the user.",
        ] {
            let err = command_failed(stderr, NO_INSTALL_DETAIL);
            assert!(err.refused, "{stderr}");
            assert_eq!(err.detail, stderr, "the tool's own text travels whole");
        }
    }

    #[test]
    fn setting_up_rejects_non_certificate_input() {
        let err = set_up_this_machine("BEGIN RSA PRIVATE KEY".into(), "127.0.0.1".into(), 7420)
            .unwrap_err();
        assert!(err.detail.contains("not a PEM certificate"));
        assert!(!err.refused);
    }

    #[test]
    fn hardware_ports_map_a_device_to_the_name_its_owner_sees() {
        let listing =
            "Hardware Port: Ethernet\nDevice: en0\nEthernet Address: a1:b2:c3:d4:e5:f6\n\n\
                       Hardware Port: Wi-Fi\nDevice: en1\nEthernet Address: f6:e5:d4:c3:b2:a1\n\n\
                       VLAN Configurations\n===================\n";
        let ports = parse_hardware_ports(listing);

        assert_eq!(ports.get("en1").map(String::as_str), Some("Wi-Fi"));
        assert_eq!(ports.get("en0").map(String::as_str), Some("Ethernet"));
        assert_eq!(ports.get("bridge0"), None);
    }

    // Every host that cannot say - each one that is not macOS, and a macOS that
    // answered something else - renames nothing, and the device name stands.
    #[test]
    fn a_listing_that_names_nothing_renames_nothing() {
        assert!(parse_hardware_ports("").is_empty());
        assert!(parse_hardware_ports("eth0: flags=4163<UP,BROADCAST>\n").is_empty());
        assert!(parse_hardware_ports("Hardware Port: Wi-Fi\n").is_empty());
    }

    #[test]
    fn only_macos_is_asked_what_it_calls_its_devices() {
        assert!(hardware_ports("linux").is_empty());
        assert!(hardware_ports("windows").is_empty());
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
        assert!(
            script.contains("networksetup -setsecurewebproxy 'Thunderbolt Bridge' 127.0.0.1 7420")
        );
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
        assert!(set_proxy("127.0.0.1".into(), 0).unwrap_err().detail.contains("port"));
    }

    // F27: one prompt, not two. The trust write and the proxy write share a
    // single elevation, and the `&&` is what keeps a refused trust from leaving
    // the machine routing through a proxy it does not trust.
    #[test]
    fn macos_sets_up_the_whole_machine_under_one_elevation() {
        let svcs = ["Wi-Fi".into()];
        let cmds = set_up_commands("macos", &cert(), &svcs, "127.0.0.1", 7420).unwrap();

        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0][0], "osascript");
        let script = &cmds[0][2];
        assert_eq!(script.matches("with administrator privileges").count(), 1);
        assert!(script.contains("'security' 'add-trusted-cert'"));
        assert!(script.contains("'/tmp/trawler-ca.pem'"));
        assert!(script.contains("networksetup -setwebproxy 'Wi-Fi' 127.0.0.1 7420"));
        assert!(script.contains("networksetup -setsecurewebproxy 'Wi-Fi' 127.0.0.1 7420"));
        assert!(script.contains("' && networksetup"), "the proxy write follows the trust write");
    }

    // On Windows the proxy lives under HKCU and needs no elevation at all, so
    // one UAC prompt — the store write's — is already the whole cost.
    #[test]
    fn windows_sets_up_the_whole_machine_with_one_uac_prompt() {
        let cmds = set_up_commands("windows", &cert(), &[], "127.0.0.1", 7420).unwrap();

        let elevated: Vec<_> =
            cmds.iter().filter(|c| c.iter().any(|a| a.contains("RunAs"))).collect();
        assert_eq!(elevated.len(), 1);
        assert!(elevated[0].iter().any(|a| a.contains("certutil")));
        assert_eq!(cmds[1], reg_add("ProxyServer", "REG_SZ", "127.0.0.1:7420"));
    }

    #[test]
    fn a_system_with_no_automatic_install_has_no_combined_act_either() {
        assert_eq!(set_up_commands("linux", &cert(), &[], "127.0.0.1", 7420), None);
    }

    // The two single commands are the manual route's secondaries and did not
    // move: whatever the combined act does, these still do exactly one thing.
    #[test]
    fn the_single_commands_are_unchanged_by_the_combined_one() {
        let svcs = ["Wi-Fi".into()];
        let proxy_only = set_proxy_commands("macos", &svcs, "127.0.0.1", 7420).unwrap();
        assert_eq!(proxy_only.len(), 1);
        assert!(!proxy_only[0][2].contains("security"));

        let install_only = elevated_argv("macos", &install_command("macos", &cert()).unwrap());
        assert!(!install_only[2].contains("networksetup"));
    }

    // F17: the screen names the proxy it is about to replace, so the reading
    // that classifies it has to carry the address it already read.
    #[test]
    fn the_reading_carries_the_address_it_classified() {
        let foreign = read_proxy(&ProxyRead::Set("10.1.1.1".into(), 8080), "127.0.0.1", 7420);
        assert_eq!(
            foreign,
            ProxyReading { status: "foreign", address: Some("10.1.1.1:8080".into()) }
        );

        let ours = read_proxy(&ProxyRead::Set("127.0.0.1".into(), 7420), "127.0.0.1", 7420);
        assert_eq!(ours, ProxyReading { status: "ours", address: Some("127.0.0.1:7420".into()) });
    }

    #[test]
    fn a_machine_with_no_proxy_reports_no_address_to_name() {
        assert_eq!(
            read_proxy(&ProxyRead::Off, "127.0.0.1", 7420),
            ProxyReading { status: "none", address: None }
        );
    }

    #[test]
    fn a_proxy_nothing_could_read_is_unknown_and_names_no_address() {
        assert_eq!(
            read_proxy(&ProxyRead::Unreadable, "127.0.0.1", 7420),
            ProxyReading { status: "unknown", address: None }
        );
    }

    fn answered(stdout: &str) -> Option<Ran> {
        Some(Ran { ok: true, stdout: stdout.into() })
    }

    fn exited_non_zero(stdout: &str) -> Option<Ran> {
        Some(Ran { ok: false, stdout: stdout.into() })
    }

    const MAC_SERVICES: &str =
        "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\nThunderbolt Bridge\n";
    const MAC_ON: &str = "Enabled: Yes\nServer: 127.0.0.1\nPort: 7420\n";
    const MAC_OFF: &str = "Enabled: No\nServer: \nPort: 0\n";

    #[test]
    fn macos_reads_the_first_enabled_proxy_of_any_service() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), |flag, svc| {
            if svc == "Thunderbolt Bridge" && flag == "-getwebproxy" {
                answered(MAC_ON)
            } else {
                answered(MAC_OFF)
            }
        });
        assert_eq!(read, ProxyRead::Set("127.0.0.1".into(), 7420));
    }

    #[test]
    fn macos_is_off_only_when_every_service_answered_off() {
        assert_eq!(
            macos_proxy(answered(MAC_SERVICES).as_ref(), |_, _| answered(MAC_OFF)),
            ProxyRead::Off
        );
    }

    #[test]
    fn macos_cannot_read_without_its_service_list() {
        let never = |_: &str, _: &str| -> Option<Ran> { unreachable!("no service to read") };
        assert_eq!(macos_proxy(None, never), ProxyRead::Unreadable);
        assert_eq!(macos_proxy(exited_non_zero("").as_ref(), never), ProxyRead::Unreadable);
        assert_eq!(macos_proxy(answered("").as_ref(), never), ProxyRead::Unreadable);
    }

    #[test]
    fn macos_cannot_vouch_for_off_when_a_service_failed_to_answer() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), |_, svc| {
            if svc == "Wi-Fi" {
                None
            } else {
                answered(MAC_OFF)
            }
        });
        assert_eq!(read, ProxyRead::Unreadable);
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), |_, svc| {
            if svc == "Wi-Fi" {
                exited_non_zero("** Error")
            } else {
                answered(MAC_OFF)
            }
        });
        assert_eq!(read, ProxyRead::Unreadable);
    }

    #[test]
    fn macos_reads_a_proxy_one_service_reports_beside_one_that_failed() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), |_, svc| {
            if svc == "Wi-Fi" {
                None
            } else {
                answered(MAC_ON)
            }
        });
        assert_eq!(read, ProxyRead::Set("127.0.0.1".into(), 7420));
    }

    const WIN_ENABLED: &str =
        "\r\nHKEY_CURRENT_USER\\...\r\n    ProxyEnable    REG_DWORD    0x1\r\n";
    const WIN_DISABLED: &str =
        "\r\nHKEY_CURRENT_USER\\...\r\n    ProxyEnable    REG_DWORD    0x0\r\n";

    fn win_server(server: &str) -> Option<Ran> {
        answered(&format!(
            "\r\nHKEY_CURRENT_USER\\...\r\n    ProxyServer    REG_SZ    {server}\r\n"
        ))
    }

    #[test]
    fn windows_is_off_when_the_switch_is_off_or_was_never_written() {
        let never = || -> Option<Ran> { unreachable!("an off switch names no server") };
        assert_eq!(windows_proxy(answered(WIN_DISABLED).as_ref(), never), ProxyRead::Off);
        let absent = exited_non_zero(
            "ERROR: The system was unable to find the specified registry key or value.",
        );
        assert_eq!(windows_proxy(absent.as_ref(), never), ProxyRead::Off);
    }

    #[test]
    fn windows_reads_an_enabled_host_and_port() {
        let read = windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server("10.0.0.9:3128"));
        assert_eq!(read, ProxyRead::Set("10.0.0.9".into(), 3128));
    }

    #[test]
    fn windows_cannot_read_when_reg_cannot_run_or_the_server_is_not_one_address() {
        let never = || -> Option<Ran> { unreachable!("an unread switch names no server") };
        assert_eq!(windows_proxy(None, never), ProxyRead::Unreadable);
        assert_eq!(windows_proxy(answered(WIN_ENABLED).as_ref(), || None), ProxyRead::Unreadable);
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), || exited_non_zero("ERROR")),
            ProxyRead::Unreadable
        );
        let per_scheme = || win_server("http=127.0.0.1:7420;https=127.0.0.1:7420");
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), per_scheme),
            ProxyRead::Unreadable
        );
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server("7420")),
            ProxyRead::Unreadable
        );
    }

    #[test]
    fn a_host_with_no_reader_cannot_read_its_proxy() {
        assert_eq!(active_system_proxy("linux"), ProxyRead::Unreadable);
        assert_eq!(active_system_proxy("freebsd"), ProxyRead::Unreadable);
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
        let ours = ProxyRead::Set("127.0.0.1".into(), 7420);
        assert_eq!(classify(&ours, "127.0.0.1", 7420), ProxyStatus::Ours);
        let other_port = ProxyRead::Set("127.0.0.1".into(), 8080);
        assert_eq!(classify(&other_port, "127.0.0.1", 7420), ProxyStatus::Foreign);
        let other_host = ProxyRead::Set("10.0.0.1".into(), 7420);
        assert_eq!(classify(&other_host, "127.0.0.1", 7420), ProxyStatus::Foreign);
        assert_eq!(classify(&ProxyRead::Off, "127.0.0.1", 7420), ProxyStatus::None);
        assert_eq!(classify(&ProxyRead::Unreadable, "127.0.0.1", 7420), ProxyStatus::Unknown);
    }

    #[test]
    fn notice_dismissals_are_keyed_and_keep_the_remote_filename() {
        let dir =
            std::env::temp_dir().join(format!("trawler-test-notice-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!notice_is_dismissed(&dir, NoticeKey::RemoteNotice));
        assert!(!notice_is_dismissed(&dir, NoticeKey::ClearAsk));

        mark_notice_dismissed(&dir, NoticeKey::ClearAsk).unwrap();
        assert!(notice_is_dismissed(&dir, NoticeKey::ClearAsk));
        assert!(!notice_is_dismissed(&dir, NoticeKey::RemoteNotice));

        std::fs::write(dir.join("remote-notice-dismissed"), []).unwrap();
        assert!(notice_is_dismissed(&dir, NoticeKey::RemoteNotice));
        assert_eq!(NoticeKey::try_from("remote-notice"), Ok(NoticeKey::RemoteNotice));
        assert_eq!(NoticeKey::try_from("clear-ask"), Ok(NoticeKey::ClearAsk));
        assert!(NoticeKey::try_from("other").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
