// Trust boundary: the webview passes only PEM bytes; the privileged command is
// built here from fixed per-OS templates — never from IPC input.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

const SYSTEM_KEYCHAIN: &str = "/Library/Keychains/System.keychain";
pub(crate) const LOGIN_KEYCHAIN: &str = "login.keychain";

pub(crate) fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(ToString::to_string).collect()
}

fn install_command(os: &str, cert: &Path) -> Option<Vec<String>> {
    let cert = cert.to_string_lossy().into_owned();
    let argv: &[&str] = match os {
        "macos" => {
            &["security", "add-trusted-cert", "-d", "-r", "trustRoot", "-k", SYSTEM_KEYCHAIN]
        }
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
            let argument_list =
                argv[1..].iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(",");
            vec![
                "powershell".into(),
                "-NoProfile".into(),
                "-Command".into(),
                format!(
                    "$p = Start-Process -FilePath '{}' -ArgumentList {argument_list} -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
                    argv[0]
                ),
            ]
        }
        _ => argv.to_vec(),
    }
}

// Why an elevated command did not run, split into the facts the screen words
// for itself and the tool's own text. The OS installer's stderr is still the
// only actionable signal on a real failure, so it travels whole — but a prompt
// the user dismissed is not a failure to report in the OS's words (`refused`,
// F18), and neither is one the OS could not show (`unshowable`), which the user
// never saw and so cannot have dismissed.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct SetupError {
    refused: bool,
    unshowable: bool,
    detail: String,
}

#[derive(Debug, PartialEq, Eq)]
enum Prompt {
    Answered,
    Cancelled,
    Unshowable,
}

// macOS reports a dismissed authorization dialog as `User cancelled. (-128)`;
// Windows' UAC refusal comes back as "The operation was canceled by the user".
// `(-60007)` is neither: the password prompt was answered, and the trust write
// run under it asked for an approval of its own that a process with no terminal
// has nowhere to show.
fn prompt_outcome(stderr: &str) -> Prompt {
    let text = stderr.to_ascii_lowercase();
    if text.contains("-60007") {
        Prompt::Unshowable
    } else if text.contains("-128") || text.contains("cancel") {
        Prompt::Cancelled
    } else {
        Prompt::Answered
    }
}

pub(crate) fn command_failed(stderr: &str, no_detail: &str) -> SetupError {
    let detail = stderr.trim();
    let prompt = prompt_outcome(detail);
    SetupError {
        refused: prompt == Prompt::Cancelled,
        unshowable: prompt == Prompt::Unshowable,
        detail: if detail.is_empty() { no_detail.to_string() } else { detail.to_string() },
    }
}

pub(crate) fn setup_error(detail: impl Into<String>) -> SetupError {
    SetupError { refused: false, unshowable: false, detail: detail.into() }
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

pub(crate) fn staged_cert(pem: &str) -> Result<std::path::PathBuf, SetupError> {
    staged_cert_as(pem, "trawler-ca.pem")
}

pub(crate) fn staged_cert_as(pem: &str, name: &str) -> Result<std::path::PathBuf, SetupError> {
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err(setup_error("not a PEM certificate"));
    }
    let cert = std::env::temp_dir().join(name);
    std::fs::write(&cert, pem).map_err(|e| setup_error(e.to_string()))?;
    Ok(cert)
}

pub(crate) const NO_INSTALL_DETAIL: &str =
    "install failed — the OS reported no detail; use the steps below";

// Trawler's CA is issued with this CN. A name narrows a keychain search and
// never settles one: a core started from another folder mints a new CA under
// the same name (D1), so every store below is matched on the bytes.
const CA_COMMON_NAME: &str = "Trawler Proxy CA";

// A certificate in a macOS keychain is not a trusted one: `add-trusted-cert`
// adds it first and writes its trust second, and the second half can fail on
// its own (-60007). The trust itself is what `verify-cert` evaluates, locally.
fn macos_verify_argv(cert: &Path) -> Vec<String> {
    ["security", "verify-cert", "-c", &cert.to_string_lossy(), "-p", "basic", "-L", "-l", "-q"]
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn pem_blocks(dump: &str) -> Vec<String> {
    const END: &str = "-----END CERTIFICATE-----";
    dump.split_inclusive(END)
        .filter_map(|chunk| {
            chunk.find("-----BEGIN CERTIFICATE-----").map(|at| chunk[at..].to_string())
        })
        .filter(|block| block.ends_with(END))
        .collect()
}

// One DER has one base64 text once the line breaks are gone, so two PEM blocks
// are the same certificate exactly when their bodies are equal.
fn pem_body(block: &str) -> String {
    block
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .flat_map(str::split_whitespace)
        .collect()
}

fn live_body(pem: &str) -> Option<String> {
    pem_blocks(pem).first().map(|block| pem_body(block)).filter(|body| !body.is_empty())
}

fn holds(store: &str, live: &str) -> bool {
    pem_blocks(store).iter().any(|block| pem_body(block) == live)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trust {
    Trusted,
    Installed,
    Absent,
}

// A browser is sent only the leaf, so it must find the CA in a keychain before
// any trust applies — and a trust setting is keyed on a hash and outlives its
// certificate: deleted from the keychain, the bytes handed to `verify-cert`
// still pass. So the keychain answers first, and only a certificate it holds is
// verified, which tells installed-and-untrusted (a -60007, a double-click that
// never chose Always Trust) apart from trusted (D2).
fn macos_trust(
    live: &str,
    keychain: impl FnOnce() -> Option<Ran>,
    verifies: impl FnOnce() -> Option<bool>,
) -> Option<Trust> {
    let keychain = keychain().filter(|out| out.ok)?;
    if !holds(&keychain.stdout, live) {
        return Some(Trust::Absent);
    }
    Some(if verifies()? { Trust::Trusted } else { Trust::Installed })
}

static PROBE_FILES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// Reads overlap — the window's watch and a click can both be in flight — so
// each gets a file of its own.
fn macos_live_verifies(pem: &str) -> Option<bool> {
    let n = PROBE_FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cert =
        std::env::temp_dir().join(format!("trawler-ca-probe-{}-{n}.pem", std::process::id()));
    std::fs::write(&cert, pem).ok()?;
    let argv = macos_verify_argv(&cert);
    let rest: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    let verified = ran(&argv[0], &rest).map(|out| out.ok);
    let _ = std::fs::remove_file(&cert);
    verified
}

// Windows names a certificate by its SHA-1 thumbprint in every store listing,
// in lowercase or upper, spaced or not, and in every display language.
fn thumbprint(live: &str) -> Option<String> {
    use base64::Engine as _;
    let der = base64::engine::general_purpose::STANDARD.decode(live).ok()?;
    Some(sha1_smol::Sha1::from(der).digest().to_string())
}

fn lists_thumbprint(listing: &str, thumbprint: &str) -> bool {
    listing.split_whitespace().collect::<String>().to_ascii_lowercase().contains(thumbprint)
}

// A browser trusts a root from the machine's store or the user's (D4). Absent
// needs both read; a store that did not answer cannot vouch for itself.
fn windows_trust(thumbprint: &str, machine: Option<&Ran>, user: Option<&Ran>) -> Option<Trust> {
    let read: Vec<&Ran> = [machine, user].into_iter().flatten().filter(|out| out.ok).collect();
    if read.iter().any(|out| lists_thumbprint(&out.stdout, thumbprint)) {
        Some(Trust::Trusted)
    } else if read.len() == 2 {
        Some(Trust::Absent)
    } else {
        None
    }
}

// Where each distribution family compiles its trust store; the first one this
// host has is the one its TLS libraries read (D8).
const CA_BUNDLES: [&str; 4] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
];

fn first_readable(paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| std::fs::read_to_string(path).ok())
}

fn bundle_trust(live: &str, bundle: Option<&str>) -> Option<Trust> {
    Some(if holds(bundle?, live) { Trust::Trusted } else { Trust::Absent })
}

fn ca_trust(os: &str, pem: &str) -> Option<Trust> {
    let live = live_body(pem)?;
    match os {
        "macos" => macos_trust(
            &live,
            || ran("security", &["find-certificate", "-a", "-c", CA_COMMON_NAME, "-p"]),
            || macos_live_verifies(pem),
        ),
        "windows" => windows_trust(
            &thumbprint(&live)?,
            ran("certutil", &["-store", "ROOT"]).as_ref(),
            ran("certutil", &["-user", "-store", "ROOT"]).as_ref(),
        ),
        "linux" => bundle_trust(&live, first_readable(&CA_BUNDLES).as_deref()),
        _ => None,
    }
}

// Read-only probe: does this machine trust the certificate the core serves
// now? The webview passes that certificate, so a stale one of the same name
// can never answer for it. "unknown" ⇒ nothing here could read a store, and
// the screen falls back to the request it can make of its own (DEC-62).
// Run off the main thread: the window re-reads it every few seconds.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn ca_trusted(pem: String) -> String {
    trust_word(ca_trust(std::env::consts::OS, &pem)).into()
}

pub(crate) fn trust_word(reading: Option<Trust>) -> &'static str {
    match reading {
        Some(Trust::Trusted) => "trusted",
        Some(Trust::Installed) => "installed",
        Some(Trust::Absent) => "absent",
        None => "unknown",
    }
}

// Each simulated device keeps a trust store of its own, which `simctl keychain
// add-root-cert` writes and no tool Apple ships reads back: a row holding the
// root's DER beside its trust settings. Apple documents neither the file nor
// its table, so a store not where this runtime keeps it reads unknown, never
// absent (E-ONBRVP.W7's Q1).
const SIMULATOR_DEVICES: &str = "Library/Developer/CoreSimulator/Devices";
const SIMULATOR_TRUST_STORE: &str = "data/private/var/protected/trustd/private/TrustStore.sqlite3";
const SIMULATOR_BOOTED: &str = "3";

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct SimulatorTrust {
    name: String,
    trust: &'static str,
}

impl SimulatorTrust {
    pub(crate) fn new(name: String, reading: Option<Trust>) -> Self {
        Self { name, trust: trust_word(reading) }
    }
}

fn plist_value(plist: &str, key: &str) -> Option<String> {
    let after_key = &plist[plist.find(&format!("<key>{key}</key>"))?..];
    let open = after_key.find("</key>")? + "</key>".len();
    let rest = after_key[open..].trim_start().strip_prefix('<')?;
    let tag = &rest[..rest.find('>')?];
    let body = &rest[tag.len() + 1..];
    let value = &body[..body.find(&format!("</{tag}>"))?];
    Some(
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RunningSimulator {
    name: String,
    udid: String,
    store: std::path::PathBuf,
}

// The running devices, by the name the Simulator's own window shows, each with
// the UDID simctl addresses it by and the store it trusts from.
fn running_simulators(devices: &Path) -> Vec<RunningSimulator> {
    let Ok(entries) = std::fs::read_dir(devices) else {
        return Vec::new();
    };
    let mut running: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let plist = std::fs::read_to_string(entry.path().join("device.plist")).ok()?;
            if plist_value(&plist, "state")? != SIMULATOR_BOOTED {
                return None;
            }
            Some(RunningSimulator {
                name: plist_value(&plist, "name")?,
                udid: entry.file_name().to_string_lossy().into_owned(),
                store: entry.path().join(SIMULATOR_TRUST_STORE),
            })
        })
        .collect();
    running.sort();
    running
}

fn der_hex(live: &str) -> Option<String> {
    use base64::Engine as _;
    use std::fmt::Write as _;
    let der = base64::engine::general_purpose::STANDARD.decode(live).ok()?;
    Some(der.iter().fold(String::with_capacity(der.len() * 2), |mut hex, byte| {
        let _ = write!(hex, "{byte:02X}");
        hex
    }))
}

fn simulator_store_trust(
    store: &Path,
    der_hex: &str,
    query: impl FnOnce(&Path, &str) -> Option<Ran>,
) -> Option<Trust> {
    if !store.is_file() {
        return None;
    }
    let read = query(store, &format!("select count(*) from tsettings where data = x'{der_hex}';"))
        .filter(|out| out.ok)?;
    match read.stdout.trim().parse::<u32>().ok()? {
        0 => Some(Trust::Absent),
        _ => Some(Trust::Trusted),
    }
}

// A certificate that does not read leaves every running store unknown, never
// the Simulators unlisted: none listed is the screen's "none running".
fn simulators_trust(
    devices: &Path,
    live: Option<&str>,
    query: impl Fn(&Path, &str) -> Option<Ran>,
) -> Vec<SimulatorTrust> {
    let der_hex = live.and_then(der_hex);
    running_simulators(devices)
        .into_iter()
        .map(|sim| {
            let reading =
                der_hex.as_deref().and_then(|hex| simulator_store_trust(&sim.store, hex, &query));
            SimulatorTrust::new(sim.name, reading)
        })
        .collect()
}

fn sqlite_read(store: &Path, sql: &str) -> Option<Ran> {
    ran("/usr/bin/sqlite3", &["-readonly", &store.to_string_lossy(), sql])
}

// Read-only probe: which Simulators run, and does each one's own store hold the
// certificate the core serves now? None running is the screen's unread, and
// it falls back to the Simulator's marked request (DEC-63).
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn simulator_trust(pem: String) -> Vec<SimulatorTrust> {
    let home = std::env::var_os("HOME");
    match (std::env::consts::OS, home) {
        ("macos", Some(home)) => simulators_trust(
            &Path::new(&home).join(SIMULATOR_DEVICES),
            live_body(&pem).as_deref(),
            sqlite_read,
        ),
        _ => Vec::new(),
    }
}

// `xcrun` follows xcode-select, and the Command Line Tools it may point at hold
// no simctl; the Xcode that holds Simulator.app does (E-ONBRVP.W16's N1).
const XCODE_DEVELOPER: &str = ".app/Contents/Developer";
const SIMULATOR_APP_IN_DEVELOPER: &str = "/Applications/Simulator.app";
const SIMULATOR_BUNDLE_QUERY: &str = "kMDItemCFBundleIdentifier == 'com.apple.iphonesimulator'";

fn simulator_developer_dir(
    selected: &str,
    simulator_apps: impl FnOnce() -> String,
) -> Option<String> {
    if selected.trim().ends_with(XCODE_DEVELOPER) {
        return None;
    }
    simulator_apps()
        .lines()
        .filter_map(|app| app.trim().strip_suffix(SIMULATOR_APP_IN_DEVELOPER))
        .find(|dir| dir.ends_with(XCODE_DEVELOPER))
        .map(ToString::to_string)
}

pub(crate) fn simctl(developer_dir: Option<&str>, args: &[&str]) -> Vec<String> {
    let mut command = developer_dir
        .map_or_else(Vec::new, |dir| vec!["env".to_string(), format!("DEVELOPER_DIR={dir}")]);
    command.extend(["xcrun", "simctl"].iter().chain(args).map(ToString::to_string));
    command
}

#[derive(Debug, Default)]
pub(crate) struct Simulators {
    pub(crate) developer_dir: Option<String>,
    pub(crate) running: Vec<String>,
}

impl Simulators {
    pub(crate) fn read() -> Self {
        let Some(home) = std::env::var_os("HOME") else {
            return Self::default();
        };
        let running = running_simulators(&Path::new(&home).join(SIMULATOR_DEVICES))
            .into_iter()
            .map(|sim| sim.udid)
            .collect();
        let stdout = |program: &str, args: &[&str]| {
            ran(program, args).filter(|r| r.ok).map(|r| r.stdout).unwrap_or_default()
        };
        let developer_dir = simulator_developer_dir(&stdout("xcode-select", &["-p"]), || {
            stdout("mdfind", &[SIMULATOR_BUNDLE_QUERY])
        });
        Self { developer_dir, running }
    }
}

fn simulator_trust_commands(sims: &Simulators, cert: &Path) -> Vec<Vec<String>> {
    let cert = cert.to_string_lossy();
    sims.running
        .iter()
        .map(|udid| {
            simctl(sims.developer_dir.as_deref(), &["keychain", udid, "add-root-cert", &cert])
        })
        .collect()
}

pub(crate) const NO_SIMULATOR_RUNNING: &str = "no Simulator is running — open one, then try again";
pub(crate) const NO_SIMULATOR_TRUST_DETAIL: &str =
    "couldn't trust it in the Simulator — no detail; use the steps below";

// simctl writes a Simulator's store with no password, so the click is the
// whole consent (W16's Q3); every running Simulator gets it, as the command does.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn trust_simulators(pem: String) -> Result<(), SetupError> {
    let cert = staged_cert(&pem)?;
    let sims = Simulators::read();
    if sims.running.is_empty() {
        return Err(setup_error(NO_SIMULATOR_RUNNING));
    }
    run_all(simulator_trust_commands(&sims, &cert), NO_SIMULATOR_TRUST_DETAIL)
}

// Each running Android emulator, read through adb (E-ONBRVP.W8): the name of
// its AVD, whether its user CA store holds the live certificate, and the proxy
// it sends through. The store is root's: only an image that ships `su` lets adb
// read it — a Google Play image is a user build without one — so a store not
// read is unknown, never absent (Q1). The proxy is the shell user's to read on
// every image (Q2).
const EMULATOR_HOST_ALIAS: &str = "10.0.2.2";
const USER_CA_STORE: &str = "/data/misc/user/0/cacerts-added";
const SU_ANSWERED: &str = "trawler-su";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EmulatorProxy {
    Ours,
    // Set on a network the emulator does not send over: AndroidWifi, while its
    // default network is the mobile one (C4).
    WifiOnly,
    Foreign,
    None,
    Unknown,
}

impl EmulatorProxy {
    fn as_str(&self) -> &'static str {
        match self {
            EmulatorProxy::Ours => "ours",
            EmulatorProxy::WifiOnly => "wifi-only",
            EmulatorProxy::Foreign => "foreign",
            EmulatorProxy::None => "none",
            EmulatorProxy::Unknown => "unknown",
        }
    }
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorReading {
    name: String,
    trust: &'static str,
    proxy: &'static str,
}

impl EmulatorReading {
    pub(crate) fn new(name: String, trust: Option<Trust>, proxy: &EmulatorProxy) -> Self {
        Self { name, trust: trust_word(trust), proxy: proxy.as_str() }
    }
}

// A GUI app inherits no shell profile, so the SDK is looked for where Android
// Studio puts it before the PATH is tried.
fn sdk_candidates(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let home = || var("HOME").map(std::path::PathBuf::from);
    let installed = match os {
        "macos" => home().map(|h| h.join("Library/Android/sdk")),
        "windows" => {
            var("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("Android").join("Sdk"))
        }
        "linux" => home().map(|h| h.join("Android/Sdk")),
        _ => None,
    };
    ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|key| var(key).map(std::path::PathBuf::from))
        .chain(installed)
        .collect()
}

fn sdk_tool(os: &str, sdk: &Path, dir: &str, tool: &str) -> std::path::PathBuf {
    let exe = if os == "windows" { format!("{tool}.exe") } else { tool.to_string() };
    sdk.join(dir).join(exe)
}

fn adb_candidates(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    sdk_candidates(os, var).iter().map(|sdk| sdk_tool(os, sdk, "platform-tools", "adb")).collect()
}

// The SDK that holds adb, as the user's own shell writes it, so a printed
// command names a tool the user can run where a stock install put none on the
// PATH (E-ONBRVP.W17's N1). A path the shell's double quotes would change is
// not offered.
fn spelled_sdk(
    os: &str,
    sdk: &Path,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<String> {
    let path = sdk.to_str()?;
    let (key, written, special): (&str, &str, &[char]) = if os == "windows" {
        ("LOCALAPPDATA", "$env:LOCALAPPDATA", &['"', '`', '$'])
    } else {
        ("HOME", "$HOME", &['"', '`', '$', '\\'])
    };
    if path.contains(special) {
        return None;
    }
    let base = var(key).and_then(|b| b.into_string().ok()).filter(|b| !b.is_empty());
    Some(match base.as_deref().and_then(|b| path.strip_prefix(b)) {
        Some(rest) if rest.starts_with(['/', '\\']) => format!("{written}{rest}"),
        _ => path.to_string(),
    })
}

// The AVD used last: each start rewrites `hardware-qemu.ini` in its folder, and
// its `config.ini` carries the name Device Manager shows (W17's Q4) and whether
// its image is a Google Play one, whose production build has no su (W17's S3).
const AVD_LAUNCHED: &str = "hardware-qemu.ini";
const AVD_DISPLAY_NAME: &str = "avd.ini.displayname=";
const AVD_PLAY_STORE: &str = "PlayStore.enabled=";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Avd {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "playStore")]
    pub(crate) play_store: bool,
}

fn avd_home(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<std::path::PathBuf> {
    if let Some(dir) = var("ANDROID_AVD_HOME") {
        return Some(dir.into());
    }
    if let Some(dir) = var("ANDROID_USER_HOME") {
        return Some(std::path::PathBuf::from(dir).join("avd"));
    }
    let home = var(if os == "windows" { "USERPROFILE" } else { "HOME" })?;
    Some(std::path::PathBuf::from(home).join(".android").join("avd"))
}

fn ini_value<'a>(ini: &'a str, key: &str) -> Option<&'a str> {
    ini.lines().find_map(|line| line.trim().strip_prefix(key)).map(str::trim)
}

fn last_avd(avds: &Path) -> Option<Avd> {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    std::fs::read_dir(avds)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let ini = entry.path();
            if ini.extension()? != "ini" {
                return None;
            }
            let id = ini.file_stem()?.to_str()?.to_string();
            let dir =
                std::path::PathBuf::from(ini_value(&std::fs::read_to_string(&ini).ok()?, "path=")?);
            let config = std::fs::read_to_string(dir.join("config.ini")).ok()?;
            let name = ini_value(&config, AVD_DISPLAY_NAME)
                .filter(|n| !n.is_empty())
                .map_or_else(|| id.replace('_', " "), ToString::to_string);
            let play_store = matches!(ini_value(&config, AVD_PLAY_STORE), Some("yes" | "true"));
            let used =
                modified(&dir.join(AVD_LAUNCHED)).or_else(|| modified(&dir.join("config.ini")));
            Some((used, Avd { id, name, play_store }))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, avd)| avd)
}

// Where adb and the emulator are, and which emulators run, for the commands
// that act on them: every one running gets the act, as every printed command
// reaches every one (W8's C2).
#[derive(Debug, Default)]
pub(crate) struct Emulators {
    pub(crate) adb: String,
    pub(crate) emulator: String,
    pub(crate) running: Vec<String>,
    pub(crate) last_avd: Option<Avd>,
}

impl Emulators {
    pub(crate) fn read() -> Self {
        let os = std::env::consts::OS;
        let var = |key: &str| std::env::var_os(key);
        let sdk = sdk_candidates(os, var)
            .into_iter()
            .find(|sdk| sdk_tool(os, sdk, "platform-tools", "adb").is_file());
        let tool = |dir: &str, name: &str| {
            sdk.as_deref().map_or_else(
                || name.to_string(),
                |sdk| sdk_tool(os, sdk, dir, name).to_string_lossy().into_owned(),
            )
        };
        let adb = tool("platform-tools", "adb");
        let running = ran(&adb, &["devices"])
            .filter(|out| out.ok)
            .map(|out| booted_emulators(&out.stdout, |args| ran(&adb, args)))
            .unwrap_or_default();
        Self {
            emulator: tool("emulator", "emulator"),
            running,
            last_avd: avd_home(os, var).as_deref().and_then(last_avd),
            adb,
        }
    }

    pub(crate) fn on(&self, serial: &str, args: &[&str]) -> Vec<String> {
        let mut command = argv(&[&self.adb, "-s", serial]);
        command.extend(args.iter().map(ToString::to_string));
        command
    }

    fn on_each(&self, args: &[&str]) -> Vec<Vec<String>> {
        self.running.iter().map(|serial| self.on(serial, args)).collect()
    }
}

fn running_emulators(devices: &str) -> Vec<String> {
    devices
        .lines()
        .filter_map(|line| match line.split_whitespace().collect::<Vec<_>>()[..] {
            [serial, "device"] if serial.starts_with("emulator-") => Some(serial.to_string()),
            _ => None,
        })
        .collect()
}

fn listed_emulators(devices: &str) -> usize {
    devices.lines().filter(|line| line.starts_with("emulator-")).count()
}

// adb lists an emulator from its first seconds, and its store and proxy read
// nothing until Android is up, so one counts as running once it has booted (W17's S2).
fn booted_emulators(devices: &str, adb: impl Fn(&[&str]) -> Option<Ran>) -> Vec<String> {
    running_emulators(devices)
        .into_iter()
        .filter(|serial| {
            adb(&["-s", serial, "shell", "getprop", "sys.boot_completed"])
                .is_some_and(|out| out.ok && out.stdout.trim() == "1")
        })
        .collect()
}

fn user_ca_script() -> String {
    format!("su 0 sh -c 'echo {SU_ANSWERED}; cat {USER_CA_STORE}/* 2>/dev/null | base64'")
}

// The installer writes each user CA as its own DER, so the store holds the live
// certificate exactly when its files carry those bytes.
fn user_store_trust(out: Option<&Ran>, der: &[u8]) -> Option<Trust> {
    use base64::Engine as _;
    let encoded = out?.stdout.trim_start().strip_prefix(SU_ANSWERED)?;
    let files: String = encoded.split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(files).ok()?;
    let holds = !der.is_empty() && bytes.windows(der.len()).any(|window| window == der);
    Some(if holds { Trust::Trusted } else { Trust::Absent })
}

fn global_proxy(out: &Ran) -> Option<&str> {
    match out.stdout.trim() {
        "" | "null" | ":0" => None,
        set => Some(set),
    }
}

// `dumpsys connectivity` prints each network on one line, its proxy inside its
// link properties as `HttpProxy: [host] port`.
struct Network<'a> {
    id: &'a str,
    proxy: Option<(&'a str, u16)>,
}

struct Networks<'a> {
    default: Option<&'a str>,
    each: Vec<Network<'a>>,
}

fn network_proxies(dump: &str) -> Networks<'_> {
    const AGENT: &str = "NetworkAgentInfo{network{";
    const PROXY: &str = "HttpProxy: [";
    let default = dump
        .lines()
        .find_map(|line| line.trim().strip_prefix("Active default network: "))
        .map(str::trim);
    let each = dump
        .lines()
        .filter_map(|line| {
            let after = &line[line.find(AGENT)? + AGENT.len()..];
            let id = &after[..after.find('}')?];
            let proxy = after.find(PROXY).and_then(|at| {
                let rest = &after[at + PROXY.len()..];
                let host = &rest[..rest.find(']')?];
                let port = rest[host.len() + 1..].split_whitespace().next()?.parse().ok()?;
                Some((host, port))
            });
            Some(Network { id, proxy })
        })
        .collect();
    Networks { default, each }
}

// The global proxy covers every network and outranks a network's own; a proxy
// on a network the emulator does not send over routes nothing (C4).
fn emulator_proxy(global: Option<&Ran>, connectivity: Option<&Ran>, port: u16) -> EmulatorProxy {
    let ours = |host: &str, p: u16| host == EMULATOR_HOST_ALIAS && p == port;
    let global = global.filter(|out| out.ok);
    if let Some(set) = global.and_then(global_proxy) {
        return match host_and_port(set) {
            Some((host, p)) if ours(&host, p) => EmulatorProxy::Ours,
            _ => EmulatorProxy::Foreign,
        };
    }
    let Some(dump) = connectivity.filter(|out| out.ok) else {
        return EmulatorProxy::Unknown;
    };
    let networks = network_proxies(&dump.stdout);
    let on_default = networks
        .each
        .iter()
        .find(|network| Some(network.id) == networks.default)
        .and_then(|network| network.proxy);
    match on_default {
        Some((host, p)) if ours(host, p) => EmulatorProxy::Ours,
        Some(_) => EmulatorProxy::Foreign,
        None if networks.each.iter().any(|n| n.proxy.is_some_and(|(h, p)| ours(h, p))) => {
            EmulatorProxy::WifiOnly
        }
        None if global.is_some() && networks.default.is_some() => EmulatorProxy::None,
        None => EmulatorProxy::Unknown,
    }
}

fn der_bytes(live: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(live).ok()
}

#[derive(Debug, PartialEq, Eq)]
struct EmulatorsNow {
    running: Vec<EmulatorReading>,
    starting: bool,
}

// None where adb did not answer: no emulator is a reading only adb can give.
fn emulators_read(
    live: Option<&str>,
    port: u16,
    adb: impl Fn(&[&str]) -> Option<Ran>,
) -> Option<EmulatorsNow> {
    let devices = adb(&["devices"]).filter(|out| out.ok)?;
    let der = live.and_then(der_bytes);
    let store = user_ca_script();
    let booted = booted_emulators(&devices.stdout, &adb);
    let starting = booted.len() < listed_emulators(&devices.stdout);
    let running = booted
        .into_iter()
        .map(|serial| {
            let on = |args: &[&str]| adb(&[&["-s", serial.as_str()], args].concat());
            let name = on(&["shell", "getprop", "ro.boot.qemu.avd_name"])
                .filter(|out| out.ok)
                .map(|out| out.stdout.trim().replace('_', " "))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| serial.clone());
            let trust = der
                .as_deref()
                .and_then(|der| user_store_trust(on(&["shell", &store]).as_ref(), der));
            let proxy = emulator_proxy(
                on(&["shell", "settings", "get", "global", "http_proxy"]).as_ref(),
                on(&["shell", "dumpsys", "connectivity"]).as_ref(),
                port,
            );
            EmulatorReading::new(name, trust, &proxy)
        })
        .collect();
    Some(EmulatorsNow { running, starting })
}

// What the screen knows of the emulator: whether adb answered, the SDK its
// commands name, the AVD used last, each emulator running, and whether one is
// still booting (W17's W4, I4, S2).
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorRead {
    pub(crate) adb: bool,
    pub(crate) sdk: Option<String>,
    pub(crate) avd: Option<Avd>,
    pub(crate) emulators: Vec<EmulatorReading>,
    pub(crate) starting: bool,
}

// Read-only probe: which emulators run, does each one's own store hold the
// certificate the core serves now, and where does each send its traffic? A
// certificate that does not read leaves each store unknown, never the
// emulators unlisted. The window asks only while the emulator is the platform
// chosen: adb starts a server of its own where none runs.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn emulator_readings(pem: String, port: u16) -> EmulatorRead {
    let os = std::env::consts::OS;
    let var = |key: &str| std::env::var_os(key);
    let found = adb_candidates(os, var).into_iter().find(|path| path.is_file());
    let sdk = found.as_deref().and_then(Path::parent).and_then(Path::parent);
    let adb = found.as_deref().map_or_else(|| "adb".into(), |p| p.to_string_lossy());
    let live = live_body(&pem);
    let now = emulators_read(live.as_deref(), port, |args| ran(&adb, args));
    EmulatorRead {
        adb: now.is_some(),
        sdk: sdk.and_then(|sdk| spelled_sdk(os, sdk, var)),
        avd: avd_home(os, var).as_deref().and_then(last_avd),
        starting: now.as_ref().is_some_and(|now| now.starting),
        emulators: now.map(|now| now.running).unwrap_or_default(),
    }
}

// Android names each user CA by the old OpenSSL hash of its subject, an index
// after it where two subjects share one: every Trawler CA shares its subject,
// so a store may already hold another core's at `.0`.
fn der_element(bytes: &[u8]) -> Option<(usize, usize)> {
    let first = *bytes.get(1)?;
    let (header, len) = if first < 0x80 {
        (2, usize::from(first))
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 {
            return None;
        }
        let len = bytes.get(2..2 + n)?.iter().fold(0usize, |acc, b| (acc << 8) | usize::from(*b));
        (2 + n, len)
    };
    let total = header.checked_add(len)?;
    (bytes.len() >= total).then_some((header, total))
}

fn der_subject(cert: &[u8]) -> Option<&[u8]> {
    let (header, _) = der_element(cert)?;
    let tbs = &cert[header..];
    let (header, total) = der_element(tbs)?;
    let mut rest = &tbs[header..total];
    let mut fields = Vec::new();
    while !rest.is_empty() {
        let (_, len) = der_element(rest)?;
        fields.push(&rest[..len]);
        rest = &rest[len..];
    }
    let versioned = usize::from(fields.first()?.first() == Some(&0xa0));
    fields.get(versioned + 4).copied()
}

fn subject_hash_old(der: &[u8]) -> Option<String> {
    use md5::{Digest as _, Md5};
    let digest = Md5::digest(der_subject(der)?);
    Some(format!("{:08x}", u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]])))
}

const STAGED_DER: &str = "/data/local/tmp/trawler-ca.der";

// Written as root where Android's own installer would write it, then announced,
// so a browser already open trusts it without a restart (W17's I1).
fn user_ca_install_script(hash: &str) -> String {
    format!(
        "su 0 sh -c 'd={USER_CA_STORE}; t={STAGED_DER}; mkdir -p $d && chown system:system $d && chmod 755 $d && i=0 && while [ -e $d/{hash}.$i ] && ! cmp -s $t $d/{hash}.$i; do i=$((i+1)); done && cp $t $d/{hash}.$i && chown system:system $d/{hash}.$i && chmod 644 $d/{hash}.$i && restorecon -R $d && rm $t && am broadcast -a android.security.action.TRUST_STORE_CHANGED >/dev/null'"
    )
}

fn emulator_install_commands(emus: &Emulators, der: &Path, hash: &str) -> Vec<Vec<String>> {
    let script = user_ca_install_script(hash);
    let der = der.to_string_lossy();
    emus.running
        .iter()
        .flat_map(|serial| {
            [emus.on(serial, &["push", &der, STAGED_DER]), emus.on(serial, &["shell", &script])]
        })
        .collect()
}

pub(crate) fn staged_der(pem: &str, name: &str) -> Result<std::path::PathBuf, SetupError> {
    let der = live_body(pem)
        .and_then(|live| der_bytes(&live))
        .ok_or_else(|| setup_error("not a PEM certificate"))?;
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, der).map_err(|e| setup_error(e.to_string()))?;
    Ok(path)
}

fn emulator_route_commands(emus: &Emulators, value: &str) -> Vec<Vec<String>> {
    emus.on_each(&["shell", "settings", "put", "global", "http_proxy", value])
}

pub(crate) const NO_EMULATOR_RUNNING: &str = "no emulator is running — start one, then try again";
pub(crate) const NO_EMULATOR_INSTALL_DETAIL: &str =
    "couldn't install it in the emulator — no detail; use the steps below";
pub(crate) const NO_EMULATOR_ROUTE_DETAIL: &str =
    "couldn't route the emulator — no detail; use the steps below";
pub(crate) const NO_EMULATOR_UNROUTE_DETAIL: &str =
    "couldn't stop routing the emulator — no detail; use the steps below";

fn running(emus: Emulators) -> Result<Emulators, SetupError> {
    if emus.running.is_empty() {
        Err(setup_error(NO_EMULATOR_RUNNING))
    } else {
        Ok(emus)
    }
}

// An emulator is a test device and su asks for nothing, so the click is the
// whole consent (W17's Q3).
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn install_on_emulators(pem: String) -> Result<(), SetupError> {
    let der = staged_der(&pem, "trawler-ca.der")?;
    let hash = std::fs::read(&der)
        .ok()
        .and_then(|bytes| subject_hash_old(&bytes))
        .ok_or_else(|| setup_error("not a certificate Android can name"))?;
    let emus = running(Emulators::read())?;
    run_all(emulator_install_commands(&emus, &der, &hash), NO_EMULATOR_INSTALL_DETAIL)
}

#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn route_emulators(port: u16) -> Result<(), SetupError> {
    checked_address(EMULATOR_HOST_ALIAS, port)?;
    let emus = running(Emulators::read())?;
    let value = format!("{EMULATOR_HOST_ALIAS}:{port}");
    run_all(emulator_route_commands(&emus, &value), NO_EMULATOR_ROUTE_DETAIL)
}

#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn unroute_emulators() -> Result<(), SetupError> {
    let emus = running(Emulators::read())?;
    run_all(emulator_route_commands(&emus, ":0"), NO_EMULATOR_UNROUTE_DETAIL)
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
    ran("networksetup", &["-listallhardwareports"])
        .filter(|listing| listing.ok)
        .map(|listing| parse_hardware_ports(&listing.stdout))
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
// administrator privileges`, joined so that a certificate left untrusted never
// leaves the machine routing through a proxy it does not trust. The join reads
// the trust back rather than the write's exit: the write can report -60007 for
// an approval it could not show and still leave the certificate trusted, and
// then the setup did happen. On Windows only the store
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
            let script = format!(
                "({} || {}) && {}",
                quoted(&install),
                quoted(&macos_verify_argv(cert)),
                macos_proxy_script(services, host, port)
            );
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

pub(crate) const NO_REMOVE_DETAIL: &str =
    "couldn't remove the certificate — no detail; use the steps below";

#[derive(Debug, Default, PartialEq, Eq)]
struct Held {
    login: bool,
    system: bool,
    admin_trust: bool,
}

// A keychain that did not answer cannot vouch for holding nothing. The admin
// trust list names certificates, not bytes, so a name in it only asks for the
// removal that is harmless where it finds nothing.
fn macos_held(
    thumbprint: &str,
    login: Option<&Ran>,
    system: Option<&Ran>,
    admin_trust: Option<&Ran>,
) -> Option<Held> {
    let holds = |out: Option<&Ran>| {
        out.filter(|out| out.ok).map(|out| lists_thumbprint(&out.stdout, thumbprint))
    };
    Some(Held {
        login: holds(login)?,
        system: holds(system)?,
        admin_trust: admin_trust.is_some_and(|out| out.stdout.contains(CA_COMMON_NAME)),
    })
}

// Each copy goes with the trust written for it: a trust setting outlives its
// certificate, and the same bytes added back would read trusted with nobody
// having said so. The login keychain is the user's own; the System keychain and
// the admin trust ask for an administrator, once, in one script.
fn remove_ca_commands(cert: &Path, thumbprint: &str, held: &Held) -> Vec<Vec<String>> {
    let hash = thumbprint.to_ascii_uppercase();
    let delete_from =
        |keychain: &str| argv(&["security", "delete-certificate", "-t", "-Z", &hash, keychain]);
    let mut commands = Vec::new();
    if held.login {
        commands.push(delete_from(LOGIN_KEYCHAIN));
    }
    if held.system || held.admin_trust {
        let untrust = argv(&["security", "remove-trusted-cert", "-d", &cert.to_string_lossy()]);
        let then = if held.system { quoted(&delete_from(SYSTEM_KEYCHAIN)) } else { "true".into() };
        commands.push(elevated_script(&format!("{} ; {then}", quoted(&untrust))));
    }
    commands
}

#[cfg_attr(not(feature = "scenario"), tauri::command)]
pub fn remove_ca(pem: String) -> Result<(), SetupError> {
    if std::env::consts::OS != "macos" {
        return Err(setup_error("no automatic removal on this system — follow the steps"));
    }
    let thumbprint = live_body(&pem)
        .and_then(|live| thumbprint(&live))
        .ok_or_else(|| setup_error("not a PEM certificate"))?;
    let cert = staged_cert(&pem)?;
    let search = |keychain: &str| {
        ran("security", &["find-certificate", "-a", "-c", CA_COMMON_NAME, "-Z", keychain])
    };
    let held = macos_held(
        &thumbprint,
        search(LOGIN_KEYCHAIN).as_ref(),
        search(SYSTEM_KEYCHAIN).as_ref(),
        ran("security", &["dump-trust-settings", "-d"]).as_ref(),
    )
    .ok_or_else(|| setup_error("couldn't read the keychains"))?;
    run_all(remove_ca_commands(&cert, &thumbprint, &held), NO_REMOVE_DETAIL)
}

// The actual system proxy state, classified against Trawler's own address — the
// quit-time confirm keys off this, NOT off whether we set it this session, so a
// proxy the user pointed at Trawler by hand is caught too. "ours" ⇒ an active
// proxy on this system routes through Trawler; "foreign" ⇒ some other active
// proxy, which a quit changes nothing for (W11's D3); "none" ⇒ this system is
// clean; "unknown" ⇒ nothing here could read it, which quit says it could not
// read and setup words as a promise rather than a false "off" (R19).
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

// Every reader runs through here, and the window waits on each one before it
// reads again, so a tool that never answers (an emulator adb lists but cannot
// reach) is stopped and reads as nothing rather than freezing every reading
// after it. Its output drains apart from the wait: a tool blocked on a full
// pipe would otherwise look hung.
const READ_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

fn ran(program: &str, args: &[&str]) -> Option<Ran> {
    ran_within(program, args, READ_PATIENCE)
}

fn ran_within(program: &str, args: &[&str], patience: std::time::Duration) -> Option<Ran> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (drained, output) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = drained.send(std::io::Read::read_to_end(&mut stdout, &mut bytes).map(|_| bytes));
    });
    if let Ok(Ok(bytes)) = output.recv_timeout(patience) {
        let status = child.wait().ok()?;
        return Some(Ran {
            ok: status.success(),
            stdout: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
    let _ = child.kill();
    let _ = child.wait();
    None
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

// localhost resolves to 127.0.0.1 and ::1 and a client tries each, so a proxy
// typed as localhost reaches Trawler's loopback listener. Any other loopback
// address is a listener of its own, which Trawler never binds (D5).
fn localhost_reaches(host: &str) -> bool {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip == std::net::Ipv4Addr::LOCALHOST || ip == std::net::Ipv6Addr::LOCALHOST)
}

// A proxy at one of this machine's own addresses reaches the same listener as
// Trawler's own address does, so it routes here too (W11's D3).
fn names_trawler(set: &str, host: &str, own: &[String]) -> bool {
    let bare = |addr: &str| addr.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    set.eq_ignore_ascii_case(host)
        || (set.eq_ignore_ascii_case("localhost") && localhost_reaches(host))
        || own.iter().any(|addr| bare(addr) == bare(set))
}

fn names_trawler_at(set: &str, at: u16, host: &str, port: u16, own: &[String]) -> bool {
    at == port && names_trawler(set, host, own)
}

fn classify(read: &ProxyRead, host: &str, port: u16, own: &[String]) -> ProxyStatus {
    match read {
        ProxyRead::Set(s, p) if names_trawler_at(s, *p, host, port, own) => ProxyStatus::Ours,
        ProxyRead::Set(..) => ProxyStatus::Foreign,
        ProxyRead::Off => ProxyStatus::None,
        ProxyRead::Unreadable => ProxyStatus::Unknown,
    }
}

// Quit's reader, and deliberately wider than setup's: every service, HTTPS then
// HTTP. A service not in use now that still points at Trawler strands the
// machine the day it becomes the one in use, so quit may not miss it, and a
// foreign proxy on another service asks nothing (DEC-64), so it may not hide
// it either. A read that failed means the services that did answer cannot
// vouch for a clean system.
fn macos_proxy(
    listing: Option<&Ran>,
    ours: impl Fn(&str, u16) -> bool,
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
    let mut foreign = None;
    for svc in &services {
        for flag in ["-getsecurewebproxy", "-getwebproxy"] {
            match read(flag, svc) {
                Some(out) if out.ok => {
                    if let Some((host, port)) = parse_active_proxy(&out.stdout) {
                        if ours(&host, port) {
                            return ProxyRead::Set(host, port);
                        }
                        foreign.get_or_insert(ProxyRead::Set(host, port));
                    }
                }
                _ => unreadable = true,
            }
        }
    }
    match foreign {
        _ if unreadable => ProxyRead::Unreadable,
        Some(set) => set,
        None => ProxyRead::Off,
    }
}

// `reg query` exits non-zero for a value never written, and a ProxyEnable never
// written is a proxy never enabled — so only `reg` failing to run leaves the
// switch unread. The server is one host:port for every scheme, or a
// per-scheme list, where HTTPS goes wherever its own entry says and nowhere
// through a proxy when the list has none (D4).
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
                .and_then(reg_value)
                .and_then(https_server)
        })
        .unwrap_or(ProxyRead::Unreadable)
}

// A `reg query` line is the name, the type and the value, and the value may
// itself hold spaces.
fn reg_value(line: &str) -> Option<&str> {
    let (_name, rest) = line.trim().split_once(char::is_whitespace)?;
    let (_kind, value) = rest.trim_start().split_once(char::is_whitespace)?;
    Some(value.trim())
}

// WinINET separates a per-scheme list's entries by `;` or by whitespace.
fn https_server(server: &str) -> Option<ProxyRead> {
    if !server.contains('=') {
        return host_and_port(server).map(|(host, port)| ProxyRead::Set(host, port));
    }
    let entry = server.split(|c: char| c == ';' || c.is_whitespace()).find_map(|entry| {
        let (scheme, address) = entry.split_once('=')?;
        scheme.trim().eq_ignore_ascii_case("https").then_some(address.trim())
    });
    match entry {
        None => Some(ProxyRead::Off),
        Some(address) => host_and_port(address.strip_prefix("http://").unwrap_or(address))
            .map(|(host, port)| ProxyRead::Set(host, port)),
    }
}

fn host_and_port(server: &str) -> Option<(String, u16)> {
    let (host, port) = server.rsplit_once(':')?;
    if host.is_empty() || host.contains(['=', ';']) {
        return None;
    }
    Some((host.to_string(), port.parse().ok()?))
}

// `scutil --proxy` prints the dictionary macOS resolves for the service it
// routes through now, which a per-service read cannot know: Wi-Fi proxied
// while Ethernet carries the traffic routes nothing (D3). Nested dictionaries
// (exceptions, scoped per-interface copies) are not what applies, so only the
// top level is read, and HTTPS decides — every HTTPS request is one Trawler
// has to decrypt.
fn scutil_proxy(out: Option<&Ran>) -> ProxyRead {
    let Some(out) = out.filter(|out| out.ok && out.stdout.contains("<dictionary>")) else {
        return ProxyRead::Unreadable;
    };
    let entries = top_level_entries(&out.stdout);
    let https = |key: &str| entries.get(key).copied().unwrap_or_default();
    if https("HTTPSEnable") != "1" {
        return ProxyRead::Off;
    }
    match (https("HTTPSProxy"), https("HTTPSPort").parse()) {
        (host, Ok(port)) if !host.is_empty() => ProxyRead::Set(host.to_string(), port),
        _ => ProxyRead::Off,
    }
}

fn top_level_entries(dump: &str) -> HashMap<&str, &str> {
    let mut depth = 0usize;
    let mut entries = HashMap::new();
    for line in dump.lines().map(str::trim) {
        if line == "}" {
            depth = depth.saturating_sub(1);
        } else if let Some((key, value)) = line.split_once(" : ") {
            if value.ends_with('{') {
                depth += 1;
            } else if depth == 1 {
                entries.insert(key, value);
            }
        } else if line.ends_with('{') {
            depth += 1;
        }
    }
    entries
}

// GNOME's own proxy settings, which WebKitGTK and GNOME's apps follow. Another
// desktop keeps its proxy elsewhere, so there gsettings answers for nothing.
// An automatic (PAC) proxy decides per request and is not read here.
fn gnome_proxy(desktop: Option<&str>, mut get: impl FnMut(&str, &str) -> Option<Ran>) -> ProxyRead {
    if !desktop.is_some_and(|d| d.to_ascii_lowercase().contains("gnome")) {
        return ProxyRead::Unreadable;
    }
    let mut value = |schema: &str, key: &str| {
        get(schema, key)
            .filter(|out| out.ok)
            .map(|out| out.stdout.trim().trim_matches('\'').to_string())
    };
    match value("org.gnome.system.proxy", "mode").as_deref() {
        Some("none") => ProxyRead::Off,
        Some("manual") => {
            let host = value("org.gnome.system.proxy.https", "host");
            let port =
                value("org.gnome.system.proxy.https", "port").and_then(|p| p.parse::<u16>().ok());
            match (host, port) {
                (Some(host), Some(port)) if !host.is_empty() && port != 0 => {
                    ProxyRead::Set(host, port)
                }
                (Some(_), Some(_)) => ProxyRead::Off,
                _ => ProxyRead::Unreadable,
            }
        }
        _ => ProxyRead::Unreadable,
    }
}

// The proxy each host applies to HTTPS now — what setup's route task reads.
// Quit asks a wider question (anything left pointing here) and keeps its own
// reader below.
fn proxy_in_use_now(os: &str) -> ProxyRead {
    match os {
        "macos" => scutil_proxy(ran("scutil", &["--proxy"]).as_ref()),
        "windows" => active_system_proxy(os, |_, _| false),
        "linux" => gnome_proxy_now(),
        _ => ProxyRead::Unreadable,
    }
}

fn gnome_proxy_now() -> ProxyRead {
    gnome_proxy(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(), |schema, key| {
        ran("gsettings", &["get", schema, key])
    })
}

fn active_system_proxy(os: &str, ours: impl Fn(&str, u16) -> bool) -> ProxyRead {
    match os {
        "macos" => macos_proxy(
            ran("networksetup", &["-listallnetworkservices"]).as_ref(),
            ours,
            |flag, svc| ran("networksetup", &[flag, svc]),
        ),
        "windows" => {
            windows_proxy(ran("reg", &["query", WININET_KEY, "/v", "ProxyEnable"]).as_ref(), || {
                ran("reg", &["query", WININET_KEY, "/v", "ProxyServer"])
            })
        }
        "linux" => gnome_proxy_now(),
        _ => ProxyRead::Unreadable,
    }
}

// One reading, two questions. The quit-time confirm asks whether anything on
// this system still points at Trawler, and takes one of its three paths; the
// setup screen asks which proxy HTTPS goes through now, and needs the address
// it found, because a button that replaces a proxy the user set may not hide
// which one (F17). Read-only, no elevation, and the address is None wherever
// nothing is set or this build cannot read the host's settings.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct ProxyReading {
    status: &'static str,
    address: Option<String>,
}

pub(crate) fn read_proxy(read: &ProxyRead, host: &str, port: u16) -> ProxyReading {
    read_proxy_among(read, host, port, &[])
}

pub(crate) fn read_proxy_among(
    read: &ProxyRead,
    host: &str,
    port: u16,
    own: &[String],
) -> ProxyReading {
    let address = match read {
        ProxyRead::Set(h, p) => Some(format!("{h}:{p}")),
        ProxyRead::Off | ProxyRead::Unreadable => None,
    };
    ProxyReading { status: classify(read, host, port, own).as_str(), address }
}

// Run off the main thread: a quit waits on it, and a tool that hangs there
// would freeze the window it is deciding about.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn proxy_status(host: String, port: u16, own: Vec<String>) -> ProxyReading {
    let ours = |set: &str, at: u16| names_trawler_at(set, at, &host, port, &own);
    read_proxy_among(&active_system_proxy(std::env::consts::OS, ours), &host, port, &own)
}

// Run off the main thread: the window re-reads it every few seconds.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn proxy_in_use(host: String, port: u16) -> ProxyReading {
    read_proxy(&proxy_in_use_now(std::env::consts::OS), &host, port)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NoticeKey {
    RemoteNotice,
    UnreadNotice,
    ClearAsk,
}

impl TryFrom<&str> for NoticeKey {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "remote-notice" => Ok(Self::RemoteNotice),
            "unread-notice" => Ok(Self::UnreadNotice),
            "clear-ask" => Ok(Self::ClearAsk),
            _ => Err(format!("unknown notice key: {value}")),
        }
    }
}

fn notice_dismissed_path(config_dir: &Path, key: NoticeKey) -> std::path::PathBuf {
    let filename = match key {
        NoticeKey::RemoteNotice => "remote-notice-dismissed",
        NoticeKey::UnreadNotice => "unread-notice-dismissed",
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

pub(crate) fn config_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    tauri::Manager::path(app).app_config_dir().map_err(|e| e.to_string())
}

fn notes_shown_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join("notes-shown")
}

fn notes_are_shown(config_dir: &Path) -> bool {
    notes_shown_path(config_dir).exists()
}

fn remember_notes_shown(config_dir: &Path, shown: bool) -> std::io::Result<()> {
    if shown {
        std::fs::create_dir_all(config_dir)?;
        return std::fs::write(notes_shown_path(config_dir), []);
    }
    match std::fs::remove_file(notes_shown_path(config_dir)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[tauri::command]
pub fn notice_dismissed(app: tauri::AppHandle, key: String) -> Result<bool, String> {
    let key = NoticeKey::try_from(key.as_str())?;
    Ok(config_dir(&app).is_ok_and(|dir| notice_is_dismissed(&dir, key)))
}

#[tauri::command]
pub fn dismiss_notice(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let key = NoticeKey::try_from(key.as_str())?;
    let dir = config_dir(&app)?;
    mark_notice_dismissed(&dir, key).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn notes_shown(app: tauri::AppHandle) -> bool {
    config_dir(&app).is_ok_and(|dir| notes_are_shown(&dir))
}

#[tauri::command]
pub fn show_notes(app: tauri::AppHandle, shown: bool) -> Result<(), String> {
    let dir = config_dir(&app)?;
    remember_notes_shown(&dir, shown).map_err(|error| error.to_string())
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
        assert!(!err.unshowable);
        assert!(err.detail.contains("authorization was denied"));
    }

    #[test]
    fn a_failure_with_no_stderr_falls_back_to_the_route_that_still_works() {
        assert!(command_failed("   ", NO_INSTALL_DETAIL).detail.contains("use the steps below"));
    }

    // The three ways the OS says "you were asked and nothing happened". Each
    // one buys the screen a sentence of its own instead of the tool's text.
    #[test]
    fn a_dismissed_prompt_is_a_refusal_in_every_wording_the_os_uses() {
        for stderr in [
            "0:36: execution error: User cancelled. (-128)",
            "Start-Process : The operation was canceled by the user.",
        ] {
            let err = command_failed(stderr, NO_INSTALL_DETAIL);
            assert!(err.refused, "{stderr}");
            assert!(!err.unshowable, "{stderr}");
            assert_eq!(err.detail, stderr, "the tool's own text travels whole");
        }
    }

    // R7: the user answered the password prompt, and the approval the trust
    // write then asked for never reached them — so it is not their cancel.
    #[test]
    fn an_approval_the_os_could_not_show_is_not_a_refusal() {
        let stderr = "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (-60007)";
        let err = command_failed(stderr, NO_INSTALL_DETAIL);

        assert!(err.unshowable);
        assert!(!err.refused);
        assert_eq!(err.detail, stderr);
    }

    #[test]
    fn the_unshowable_approval_survives_the_trust_read_back_that_follows_it() {
        let stderr = "SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (-60007)\nCert Verify Result: CSSMERR_TP_NOT_TRUSTED";
        assert!(command_failed(stderr, NO_INSTALL_DETAIL).unshowable);
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
    // single elevation, and the `&&` is what keeps an untrusted certificate from
    // leaving the machine routing through a proxy it does not trust.
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
        assert!(script.contains("') && networksetup"), "the proxy write follows the trust");
    }

    // R8: the proxy write waits on the certificate being trusted, read back,
    // not on the trust write's own exit — which can report -60007 and still
    // have left the certificate trusted.
    #[test]
    fn macos_sets_the_proxy_once_the_certificate_reads_back_trusted() {
        let svcs = ["Wi-Fi".into()];
        let script = &set_up_commands("macos", &cert(), &svcs, "127.0.0.1", 7420).unwrap()[0][2];

        assert!(script.contains(
            "('security' 'add-trusted-cert' '-d' '-r' 'trustRoot' '-k' '/Library/Keychains/System.keychain' '/tmp/trawler-ca.pem' || 'security' 'verify-cert' '-c' '/tmp/trawler-ca.pem' '-p' 'basic' '-L' '-l' '-q') && networksetup"
        ));
    }

    // R10: the probe verifies each installed copy; a copy present and
    // untrusted is exactly what a -60007 leaves behind.
    #[test]
    fn the_trust_probe_verifies_the_copy_rather_than_finding_it() {
        assert_eq!(
            macos_verify_argv(&cert()),
            [
                "security",
                "verify-cert",
                "-c",
                "/tmp/trawler-ca.pem",
                "-p",
                "basic",
                "-L",
                "-l",
                "-q"
            ]
        );
    }

    #[test]
    fn a_keychain_dump_splits_into_one_certificate_per_copy() {
        let one = "-----BEGIN CERTIFICATE-----\nAAA\n-----END CERTIFICATE-----";
        let two = "-----BEGIN CERTIFICATE-----\nBBB\n-----END CERTIFICATE-----";

        assert_eq!(pem_blocks(&format!("{one}\n{two}\n")), [one, two]);
        assert_eq!(pem_blocks("").len(), 0);
        assert_eq!(pem_blocks("security: SecKeychainSearchCopyNext: not found").len(), 0);
        assert_eq!(pem_blocks("-----BEGIN CERTIFICATE-----\ntruncated").len(), 0);
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

    #[cfg(unix)]
    #[test]
    fn a_read_that_never_answers_is_stopped_and_reads_as_nothing() {
        let patience = std::time::Duration::from_millis(200);
        let started = std::time::Instant::now();
        assert!(ran_within("sleep", &["30"], patience).is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_keeps_its_whole_output_and_its_exit() {
        let patience = std::time::Duration::from_secs(10);
        let big = ran_within("sh", &["-c", "head -c 300000 /dev/zero | tr '\\0' a"], patience)
            .expect("an answer");
        assert!(big.ok);
        assert_eq!(big.stdout.len(), 300_000);
        let failed =
            ran_within("sh", &["-c", "echo partial; exit 3"], patience).expect("an answer");
        assert!(!failed.ok);
        assert_eq!(failed.stdout, "partial\n");
    }

    fn trawler(host: &str, port: u16) -> bool {
        host == "127.0.0.1" && port == 7420
    }

    const MAC_WORK: &str = "Enabled: Yes\nServer: proxy.corp\nPort: 8080\n";

    #[test]
    fn macos_finds_trawler_behind_a_foreign_proxy_on_an_earlier_service() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
            answered(if svc == "Wi-Fi" { MAC_WORK } else { MAC_ON })
        });
        assert_eq!(read, ProxyRead::Set("127.0.0.1".into(), 7420));
    }

    #[test]
    fn macos_reads_a_foreign_proxy_only_when_no_service_names_trawler() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
            answered(if svc == "Wi-Fi" { MAC_WORK } else { MAC_OFF })
        });
        assert_eq!(read, ProxyRead::Set("proxy.corp".into(), 8080));
    }

    #[test]
    fn macos_cannot_vouch_for_a_foreign_proxy_beside_a_service_that_failed_to_answer() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
            if svc == "Wi-Fi" {
                answered(MAC_WORK)
            } else {
                None
            }
        });
        assert_eq!(read, ProxyRead::Unreadable);
    }

    #[test]
    fn macos_reads_the_first_enabled_proxy_of_any_service() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |flag, svc| {
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
            macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, _| answered(MAC_OFF)),
            ProxyRead::Off
        );
    }

    #[test]
    fn macos_cannot_read_without_its_service_list() {
        let never = |_: &str, _: &str| -> Option<Ran> { unreachable!("no service to read") };
        assert_eq!(macos_proxy(None, trawler, never), ProxyRead::Unreadable);
        assert_eq!(
            macos_proxy(exited_non_zero("").as_ref(), trawler, never),
            ProxyRead::Unreadable
        );
        assert_eq!(macos_proxy(answered("").as_ref(), trawler, never), ProxyRead::Unreadable);
    }

    #[test]
    fn macos_cannot_vouch_for_off_when_a_service_failed_to_answer() {
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
            if svc == "Wi-Fi" {
                None
            } else {
                answered(MAC_OFF)
            }
        });
        assert_eq!(read, ProxyRead::Unreadable);
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
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
        let read = macos_proxy(answered(MAC_SERVICES).as_ref(), trawler, |_, svc| {
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
    fn windows_cannot_read_when_reg_cannot_run_or_the_server_names_no_address() {
        let never = || -> Option<Ran> { unreachable!("an unread switch names no server") };
        assert_eq!(windows_proxy(None, never), ProxyRead::Unreadable);
        assert_eq!(windows_proxy(answered(WIN_ENABLED).as_ref(), || None), ProxyRead::Unreadable);
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), || exited_non_zero("ERROR")),
            ProxyRead::Unreadable
        );
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server("7420")),
            ProxyRead::Unreadable
        );
        assert_eq!(
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server("https=7420")),
            ProxyRead::Unreadable
        );
    }

    // D4: a per-scheme list sends HTTPS wherever its own entry says.
    #[test]
    fn windows_reads_the_https_entry_of_a_per_scheme_list() {
        let read = |server: &'static str| {
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server(server))
        };

        assert_eq!(
            read("http=127.0.0.1:7420;https=127.0.0.1:7420"),
            ProxyRead::Set("127.0.0.1".into(), 7420)
        );
        assert_eq!(
            read("ftp=10.0.0.1:21;HTTPS=10.0.0.9:3128"),
            ProxyRead::Set("10.0.0.9".into(), 3128)
        );
        assert_eq!(read("https=http://127.0.0.1:7420"), ProxyRead::Set("127.0.0.1".into(), 7420));
    }

    #[test]
    fn windows_reads_the_https_entry_of_a_whitespace_separated_list() {
        let read = |server: &'static str| {
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server(server))
        };

        assert_eq!(
            read("https=127.0.0.1:7420 http=10.0.0.1:80"),
            ProxyRead::Set("127.0.0.1".into(), 7420)
        );
        assert_eq!(
            read("http=10.0.0.1:80 https=10.0.0.9:3128 ftp=10.0.0.2:21"),
            ProxyRead::Set("10.0.0.9".into(), 3128)
        );
        assert_eq!(read("http=10.0.0.1:80 ftp=10.0.0.2:21"), ProxyRead::Off);
    }

    #[test]
    fn a_per_scheme_list_with_no_https_entry_proxies_no_https() {
        let read =
            windows_proxy(answered(WIN_ENABLED).as_ref(), || win_server("http=127.0.0.1:7420"));
        assert_eq!(read, ProxyRead::Off);
    }

    #[test]
    fn a_host_with_no_reader_cannot_read_its_proxy() {
        assert_eq!(active_system_proxy("linux", trawler), ProxyRead::Unreadable);
        assert_eq!(active_system_proxy("freebsd", trawler), ProxyRead::Unreadable);
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
        assert_eq!(classify(&ours, "127.0.0.1", 7420, &[]), ProxyStatus::Ours);
        let other_port = ProxyRead::Set("127.0.0.1".into(), 8080);
        assert_eq!(classify(&other_port, "127.0.0.1", 7420, &[]), ProxyStatus::Foreign);
        let other_host = ProxyRead::Set("10.0.0.1".into(), 7420);
        assert_eq!(classify(&other_host, "127.0.0.1", 7420, &[]), ProxyStatus::Foreign);
        assert_eq!(classify(&ProxyRead::Off, "127.0.0.1", 7420, &[]), ProxyStatus::None);
        assert_eq!(classify(&ProxyRead::Unreadable, "127.0.0.1", 7420, &[]), ProxyStatus::Unknown);
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
        assert_eq!(NoticeKey::try_from("unread-notice"), Ok(NoticeKey::UnreadNotice));
        mark_notice_dismissed(&dir, NoticeKey::UnreadNotice).unwrap();
        assert!(dir.join("unread-notice-dismissed").exists());
        assert!(NoticeKey::try_from("other").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notes_start_hidden_and_remember_each_choice() {
        let dir =
            std::env::temp_dir().join(format!("trawler-test-notes-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!notes_are_shown(&dir));
        remember_notes_shown(&dir, false).unwrap();
        assert!(!notes_are_shown(&dir));
        remember_notes_shown(&dir, true).unwrap();
        assert!(notes_are_shown(&dir));
        remember_notes_shown(&dir, true).unwrap();
        assert!(notes_are_shown(&dir));
        remember_notes_shown(&dir, false).unwrap();
        assert!(!notes_are_shown(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── The live certificate, read from each store (D1 · D2 · D4 · D8) ──────

    const LIVE: &str = "-----BEGIN CERTIFICATE-----\nMIIBLIVE\nAAAA\n-----END CERTIFICATE-----\n";
    const STALE: &str = "-----BEGIN CERTIFICATE-----\nMIIBSTALE\nBBBB\n-----END CERTIFICATE-----\n";

    fn live() -> String {
        live_body(LIVE).unwrap()
    }

    #[test]
    fn one_certificate_reads_as_one_body_however_its_lines_are_wrapped() {
        let rewrapped =
            "-----BEGIN CERTIFICATE-----\r\nMIIB\r\nLIVEAAAA\r\n-----END CERTIFICATE-----";
        assert_eq!(live_body(rewrapped), Some(live()));
        assert_ne!(live_body(STALE), Some(live()));
        assert_eq!(live_body("not a certificate"), None);
        assert_eq!(live_body("-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----"), None);
    }

    fn never_verified() -> Option<bool> {
        unreachable!("a certificate no keychain holds is not verified")
    }

    #[test]
    fn a_mac_holding_the_live_certificate_and_verifying_it_trusts_it() {
        let dump = format!("{STALE}\n{LIVE}");
        assert_eq!(macos_trust(&live(), || answered(&dump), || Some(true)), Some(Trust::Trusted));
    }

    // D2: a double-click that never chose Always Trust, or a -60007, leaves the
    // certificate in a keychain and untrusted.
    #[test]
    fn a_mac_holding_the_live_certificate_untrusted_reads_installed() {
        assert_eq!(macos_trust(&live(), || answered(LIVE), || Some(false)), Some(Trust::Installed));
    }

    // Recorded on the owner's Mac: the certificate deleted from the System
    // keychain, its Always Trust left behind — `verify-cert` passed the live
    // bytes while a browser, sent only the leaf, refused it.
    #[test]
    fn a_trust_setting_that_outlived_its_certificate_trusts_nothing_a_browser_is_sent() {
        let not_found = || answered("");
        assert_eq!(macos_trust(&live(), not_found, never_verified), Some(Trust::Absent));
    }

    // `find-certificate -a` exits 0 when nothing matches, so a non-zero exit is
    // a keychain search that failed, not an empty keychain.
    #[test]
    fn a_keychain_search_that_failed_cannot_say() {
        assert_eq!(macos_trust(&live(), || exited_non_zero(""), never_verified), None);
    }

    // D1: the old same-name certificate verifies, and the live one is nowhere.
    #[test]
    fn a_same_name_certificate_never_answers_for_the_live_one() {
        assert_eq!(macos_trust(&live(), || answered(STALE), never_verified), Some(Trust::Absent));
    }

    #[test]
    fn a_mac_whose_tools_did_not_run_cannot_say() {
        assert_eq!(macos_trust(&live(), || None, never_verified), None);
        assert_eq!(macos_trust(&live(), || answered(LIVE), || None), None);
    }

    const HASH: &str = "61b175631438ca67af33a923d85c823bbad8a0cc";
    const HASH_LISTED: &str = "SHA-1 hash: 61B175631438CA67AF33A923D85C823BBAD8A0CC\nkeychain: \"/Users/u/Library/Keychains/login.keychain-db\"\n";
    const ADMIN_TRUSTS_TRAWLER: &str = "Number of trusted certs = 1\nCert 0: Trawler Proxy CA\n";

    #[test]
    fn removal_reads_which_keychain_holds_the_live_certificate_by_its_hash() {
        let other = "SHA-1 hash: 0000000000000000000000000000000000000000\n";
        assert_eq!(
            macos_held(HASH, answered(HASH_LISTED).as_ref(), answered(other).as_ref(), None),
            Some(Held { login: true, system: false, admin_trust: false })
        );
        assert_eq!(
            macos_held(
                HASH,
                answered("").as_ref(),
                answered(HASH_LISTED).as_ref(),
                answered(ADMIN_TRUSTS_TRAWLER).as_ref()
            ),
            Some(Held { login: false, system: true, admin_trust: true })
        );
    }

    #[test]
    fn removal_that_could_not_search_a_keychain_cannot_say_what_to_remove() {
        assert_eq!(macos_held(HASH, None, answered("").as_ref(), None), None);
        assert_eq!(
            macos_held(HASH, answered("").as_ref(), exited_non_zero("").as_ref(), None),
            None
        );
    }

    #[test]
    fn removing_from_the_login_keychain_takes_its_trust_and_asks_no_administrator() {
        let held = Held { login: true, ..Held::default() };
        assert_eq!(
            remove_ca_commands(Path::new("/tmp/trawler-ca.pem"), HASH, &held),
            vec![argv(&[
                "security",
                "delete-certificate",
                "-t",
                "-Z",
                &HASH.to_ascii_uppercase(),
                "login.keychain"
            ])]
        );
    }

    #[test]
    fn removing_from_the_system_keychain_drops_its_admin_trust_under_one_prompt() {
        let held = Held { system: true, admin_trust: true, ..Held::default() };
        let commands = remove_ca_commands(Path::new("/tmp/trawler-ca.pem"), HASH, &held);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0][0], "osascript");
        assert!(commands[0][2].contains(&format!(
            "'security' 'remove-trusted-cert' '-d' '/tmp/trawler-ca.pem' ; 'security' 'delete-certificate' '-t' '-Z' '{}' '/Library/Keychains/System.keychain'",
            HASH.to_ascii_uppercase()
        )));
        assert!(commands[0][2].ends_with("with administrator privileges"));
    }

    // Recorded on the owner's Mac: a trust left behind by a deleted System copy
    // trusted the same bytes added back to the login keychain.
    #[test]
    fn an_admin_trust_the_system_keychain_outlived_is_removed_with_the_login_copy() {
        let held = Held { login: true, admin_trust: true, ..Held::default() };
        let commands = remove_ca_commands(Path::new("/tmp/trawler-ca.pem"), HASH, &held);
        assert_eq!(commands.len(), 2);
        assert!(commands[1][2]
            .contains("'security' 'remove-trusted-cert' '-d' '/tmp/trawler-ca.pem' ; true"));
    }

    #[test]
    fn nothing_held_removes_nothing() {
        assert_eq!(
            remove_ca_commands(Path::new("/tmp/trawler-ca.pem"), HASH, &Held::default()).len(),
            0
        );
    }

    #[test]
    fn the_thumbprint_is_the_sha1_of_the_certificates_der() {
        assert_eq!(thumbprint("YWJj").as_deref(), Some("a9993e364706816aba3e25717850c26c9cd0d89d"));
        assert_eq!(thumbprint("not base64!"), None);
    }

    const THUMB: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    #[test]
    fn a_store_listing_names_the_thumbprint_in_any_case_spacing_or_language() {
        for listing in [
            "Cert Hash(sha1): a9993e364706816aba3e25717850c26c9cd0d89d\r\n",
            "Cert Hash(sha1): A9 99 3E 36 47 06 81 6A BA 3E 25 71 78 50 C2 6C 9C D0 D8 9D\r\n",
            "Zertifikathash(sha1): a9993e364706816aba3e25717850c26c9cd0d89d\r\n",
        ] {
            assert!(lists_thumbprint(listing, THUMB), "{listing}");
        }
        assert!(!lists_thumbprint(
            "Cert Hash(sha1): 0000000000000000000000000000000000000000",
            THUMB
        ));
    }

    #[test]
    fn windows_trusts_a_root_in_the_machine_store_or_the_users() {
        let listing = answered(&format!("Cert Hash(sha1): {THUMB}"));
        let other = answered("Cert Hash(sha1): 0000000000000000000000000000000000000000");
        assert_eq!(windows_trust(THUMB, listing.as_ref(), other.as_ref()), Some(Trust::Trusted));
        assert_eq!(windows_trust(THUMB, other.as_ref(), listing.as_ref()), Some(Trust::Trusted));
        assert_eq!(windows_trust(THUMB, None, listing.as_ref()), Some(Trust::Trusted));
    }

    #[test]
    fn windows_reads_absent_only_once_both_stores_answered() {
        let other = answered("Cert Hash(sha1): 0000000000000000000000000000000000000000");
        assert_eq!(windows_trust(THUMB, other.as_ref(), other.as_ref()), Some(Trust::Absent));
        assert_eq!(windows_trust(THUMB, other.as_ref(), None), None);
        assert_eq!(windows_trust(THUMB, exited_non_zero("").as_ref(), other.as_ref()), None);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trawler-test-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    // D8: the first bundle this host has is the store its TLS libraries read.
    #[test]
    fn a_linux_bundle_holding_the_live_certificate_trusts_it() {
        let bundle = scratch("with-live.crt");
        std::fs::write(&bundle, format!("{STALE}{LIVE}")).unwrap();
        let missing = scratch("missing.crt");
        let paths = [missing.to_str().unwrap(), bundle.to_str().unwrap()];

        assert_eq!(bundle_trust(&live(), first_readable(&paths).as_deref()), Some(Trust::Trusted));
        std::fs::write(&bundle, STALE).unwrap();
        assert_eq!(bundle_trust(&live(), first_readable(&paths).as_deref()), Some(Trust::Absent));
        let _ = std::fs::remove_file(&bundle);
    }

    #[test]
    fn a_host_with_no_bundle_cannot_say() {
        let missing = scratch("none-here.crt");
        assert_eq!(first_readable(&[missing.to_str().unwrap()]), None);
        assert_eq!(bundle_trust(&live(), None), None);
    }

    #[test]
    fn what_is_not_a_certificate_reads_unknown_on_every_host() {
        for os in ["macos", "windows", "linux", "freebsd"] {
            assert_eq!(ca_trust(os, "BEGIN RSA PRIVATE KEY"), None, "{os}");
        }
        assert_eq!(ca_trust("freebsd", LIVE), None);
    }

    #[test]
    fn each_reading_has_its_own_word() {
        assert_eq!(trust_word(Some(Trust::Trusted)), "trusted");
        assert_eq!(trust_word(Some(Trust::Installed)), "installed");
        assert_eq!(trust_word(Some(Trust::Absent)), "absent");
        assert_eq!(trust_word(None), "unknown");
    }

    // ── Each running Simulator's own store (E-ONBRVP.W7 · Q1) ───────────────

    // CoreSimulator's own layout, as recorded on an iOS 26.5 runtime.
    fn device_plist(name: &str, state: u8) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n\
             \t<key>UDID</key>\n\t<string>F8B53282-E662-4C67-B106-81760F47AEA4</string>\n\
             \t<key>name</key>\n\t<string>{name}</string>\n\
             \t<key>state</key>\n\t<integer>{state}</integer>\n</dict>\n</plist>\n"
        )
    }

    fn simulators(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("trawler-test-simulators-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn simulator(devices: &Path, udid: &str, name: &str, state: u8, store: bool) {
        let dir = devices.join(udid);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("device.plist"), device_plist(name, state)).unwrap();
        if store {
            let store = dir.join(SIMULATOR_TRUST_STORE);
            std::fs::create_dir_all(store.parent().unwrap()).unwrap();
            std::fs::write(store, "").unwrap();
        }
    }

    fn never_queried(_: &Path, _: &str) -> Option<Ran> {
        unreachable!("a store that is not there is not queried")
    }

    #[test]
    fn a_device_plist_names_the_device_and_its_state() {
        let plist = device_plist("iPad Pro 11-inch (M5) &amp; more", 3);
        assert_eq!(plist_value(&plist, "name").as_deref(), Some("iPad Pro 11-inch (M5) & more"));
        assert_eq!(plist_value(&plist, "state").as_deref(), Some("3"));
        assert_eq!(plist_value(&plist, "runtime"), None);
    }

    #[test]
    fn the_store_is_asked_for_the_live_certificates_own_bytes() {
        assert_eq!(der_hex("MIIB").as_deref(), Some("308201"));
        let devices = simulators("asked");
        simulator(&devices, "A", "iPhone 17 Pro", 3, true);
        let asked = std::cell::RefCell::new(String::new());
        simulators_trust(&devices, Some("MIIB"), |_, sql| {
            asked.replace(sql.to_string());
            answered("1\n")
        });
        assert_eq!(asked.into_inner(), "select count(*) from tsettings where data = x'308201';");
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn only_a_running_simulator_is_read_and_each_is_named() {
        let devices = simulators("running");
        simulator(&devices, "A", "iPhone 17 Pro", 3, true);
        simulator(&devices, "B", "iPad Air 11-inch (M4)", 1, true);
        simulator(&devices, "C", "iPad mini (A17 Pro)", 3, true);
        let read = simulators_trust(&devices, Some("MIIB"), |store, _| {
            answered(if store.starts_with(devices.join("A")) { "1" } else { "0" })
        });
        assert_eq!(
            read,
            [
                SimulatorTrust::new("iPad mini (A17 Pro)".into(), Some(Trust::Absent)),
                SimulatorTrust::new("iPhone 17 Pro".into(), Some(Trust::Trusted)),
            ]
        );
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn a_store_not_where_this_runtime_keeps_it_or_that_does_not_answer_reads_unknown() {
        let devices = simulators("unread");
        simulator(&devices, "A", "iPhone 17 Pro", 3, false);
        assert_eq!(
            simulators_trust(&devices, Some("MIIB"), never_queried),
            [SimulatorTrust::new("iPhone 17 Pro".into(), None)]
        );
        simulator(&devices, "A", "iPhone 17 Pro", 3, true);
        for answer in [Some((false, "")), Some((true, "Error: no such table: tsettings")), None] {
            assert_eq!(
                simulators_trust(&devices, Some("MIIB"), |_, _| {
                    answer.map(|(ok, stdout)| Ran { ok, stdout: stdout.into() })
                }),
                [SimulatorTrust::new("iPhone 17 Pro".into(), None)]
            );
        }
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn a_certificate_that_does_not_read_leaves_each_running_simulator_unknown() {
        let devices = simulators("unreadable-cert");
        simulator(&devices, "A", "iPhone 17 Pro", 3, true);
        assert_eq!(
            simulators_trust(&devices, None, never_queried),
            [SimulatorTrust::new("iPhone 17 Pro".into(), None)]
        );
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn no_simulator_running_or_none_installed_reads_nothing() {
        let devices = simulators("none");
        simulator(&devices, "A", "iPhone 17 Pro", 1, true);
        assert_eq!(simulators_trust(&devices, Some("MIIB"), never_queried), []);
        assert_eq!(simulators_trust(&devices.join("absent"), Some("MIIB"), never_queried), []);
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn a_running_simulator_is_addressed_by_its_udid() {
        let devices = simulators("udid");
        simulator(&devices, "F8B53282-E662-4C67-B106-81760F47AEA4", "iPhone 17 Pro", 3, true);
        simulator(&devices, "13CE9116-3056-4FBB-BC1D-068189E9842A", "iPhone 17", 1, true);
        let running = running_simulators(&devices);
        assert_eq!(
            running.iter().map(|s| s.udid.as_str()).collect::<Vec<_>>(),
            ["F8B53282-E662-4C67-B106-81760F47AEA4"]
        );
        let _ = std::fs::remove_dir_all(&devices);
    }

    const XCODE_DEVELOPER_DIR: &str = "/Applications/Xcode.app/Contents/Developer";
    const SIMULATOR_APP: &str =
        "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app\n";

    #[test]
    fn simctl_runs_under_the_xcode_holding_the_simulator_only_where_xcode_select_points_elsewhere()
    {
        let never =
            || -> String { unreachable!("an Xcode already selected asks Spotlight nothing") };
        assert_eq!(simulator_developer_dir(&format!("{XCODE_DEVELOPER_DIR}\n"), never), None);
        assert_eq!(
            simulator_developer_dir("/Library/Developer/CommandLineTools\n", || SIMULATOR_APP
                .into())
            .as_deref(),
            Some(XCODE_DEVELOPER_DIR)
        );
        assert_eq!(simulator_developer_dir("", String::new), None);
        assert_eq!(simulator_developer_dir("", || "/Users/me/Simulator.app\n".into()), None);
    }

    #[test]
    fn simctl_is_xcrun_unless_a_developer_dir_is_named() {
        assert_eq!(simctl(None, &["list"]), argv(&["xcrun", "simctl", "list"]));
        assert_eq!(
            simctl(Some(XCODE_DEVELOPER_DIR), &["list"]),
            argv(&[
                "env",
                &format!("DEVELOPER_DIR={XCODE_DEVELOPER_DIR}"),
                "xcrun",
                "simctl",
                "list"
            ])
        );
    }

    #[test]
    fn the_certificate_is_trusted_on_every_running_simulator() {
        let sims = Simulators { developer_dir: None, running: vec!["A".into(), "B".into()] };
        assert_eq!(
            simulator_trust_commands(&sims, Path::new("/tmp/trawler-ca.pem")),
            [
                argv(&["xcrun", "simctl", "keychain", "A", "add-root-cert", "/tmp/trawler-ca.pem"]),
                argv(&["xcrun", "simctl", "keychain", "B", "add-root-cert", "/tmp/trawler-ca.pem"]),
            ]
        );
        assert_eq!(
            simulator_trust_commands(&Simulators::default(), Path::new("/tmp/x.pem")).len(),
            0
        );
    }

    // ── Each running emulator, through adb (E-ONBRVP.W8) ────────────────────

    const ADB_DEVICES: &str =
        "List of devices attached\nemulator-5554\tdevice\nemulator-5556\toffline\n\
        R58M12ABCDE\tdevice\nemulator-5558\tdevice\n\n";
    const WIFI_NETWORK: &str = "  NetworkAgentInfo{network{100}  handle{432902426637}  ni{WIFI CONNECTED extra: } lp{{InterfaceName: wlan0 LinkAddresses: [ 10.0.2.16/24 ] DnsAddresses: [ /10.0.2.3 ] MTU: 0 ServerAddress: /10.0.2.2 Routes: [ 0.0.0.0/0 -> 10.0.2.2 wlan0 mtu 0 ]}}";
    const MOBILE_NETWORK: &str = "  NetworkAgentInfo{network{101}  handle{437197393933}  ni{MOBILE[HSPA] CONNECTED extra: fast.t-mobile.com} lp{{InterfaceName: eth0 LinkAddresses: [ 10.0.2.15/24 ] MTU: 0}}";

    fn connectivity(default: &str, wifi_proxy: Option<&str>) -> String {
        let wifi = match wifi_proxy {
            Some(proxy) => WIFI_NETWORK.replace(
                " MTU: 0 ServerAddress",
                &format!(" MTU: 0 HttpProxy: {proxy} xl ServerAddress"),
            ),
            None => WIFI_NETWORK.into(),
        };
        format!("NetworkProviders for:\n  5: WifiNetworkFactory\n\nActive default network: {default}\n\nCurrent Networks:\n{wifi}\n    Nat464Xlat:\n{MOBILE_NETWORK}\n")
    }

    fn store_answer(certs: &[&[u8]]) -> Option<Ran> {
        use base64::Engine as _;
        let files: Vec<u8> = certs.concat();
        let encoded = base64::engine::general_purpose::STANDARD.encode(files);
        let wrapped: Vec<String> = encoded
            .as_bytes()
            .chunks(76)
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect();
        answered(&format!("{SU_ANSWERED}\n{}\n", wrapped.join("\n")))
    }

    #[test]
    fn only_an_emulator_adb_can_talk_to_is_read() {
        assert_eq!(running_emulators(ADB_DEVICES), ["emulator-5554", "emulator-5558"]);
        assert_eq!(running_emulators("List of devices attached\n\n"), Vec::<String>::new());
    }

    #[test]
    fn adb_is_looked_for_where_the_sdk_is_before_the_path() {
        let vars = |key: &str| match key {
            "HOME" => Some("/Users/me".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\me\AppData\Local".into()),
            "ANDROID_HOME" => Some("/opt/sdk".into()),
            _ => None,
        };
        assert_eq!(
            adb_candidates("macos", vars),
            [
                PathBuf::from("/opt/sdk/platform-tools/adb"),
                PathBuf::from("/Users/me/Library/Android/sdk/platform-tools/adb")
            ]
        );
        assert_eq!(
            adb_candidates("linux", |key| if key == "HOME" {
                Some("/home/me".into())
            } else {
                None
            }),
            [PathBuf::from("/home/me/Android/Sdk/platform-tools/adb")]
        );
        let windows =
            adb_candidates("windows", |key| if key == "LOCALAPPDATA" { vars(key) } else { None });
        assert_eq!(windows.len(), 1);
        assert!(
            windows[0].ends_with("adb.exe") && windows[0].to_string_lossy().contains("Android")
        );
        assert_eq!(adb_candidates("freebsd", |_| None).len(), 0);
    }

    #[test]
    fn the_user_store_reads_trusted_only_where_its_files_carry_the_live_der() {
        let live = [0x30, 0x82, 0x01, 0xa4, 0x01, 0x02];
        let other = [0x30, 0x82, 0x01, 0x99, 0x07, 0x08];
        assert_eq!(
            user_store_trust(store_answer(&[&other, &live]).as_ref(), &live),
            Some(Trust::Trusted)
        );
        assert_eq!(user_store_trust(store_answer(&[&other]).as_ref(), &live), Some(Trust::Absent));
        assert_eq!(user_store_trust(store_answer(&[]).as_ref(), &live), Some(Trust::Absent));
    }

    #[test]
    fn a_store_su_cannot_open_reads_unknown_never_absent() {
        let live = [0x30, 0x82, 0x01];
        assert_eq!(user_store_trust(answered("").as_ref(), &live), None);
        assert_eq!(user_store_trust(exited_non_zero("").as_ref(), &live), None);
        assert_eq!(user_store_trust(None, &live), None);
        assert_eq!(
            user_store_trust(answered(&format!("{SU_ANSWERED}\n!!not base64")).as_ref(), &live),
            None
        );
    }

    #[test]
    fn the_global_proxy_is_read_first_and_covers_every_network() {
        let wifi_elsewhere = connectivity("101", Some("[192.168.1.9] 8080"));
        for (global, read) in [
            ("10.0.2.2:7420\n", EmulatorProxy::Ours),
            ("10.0.2.2:8888\n", EmulatorProxy::Foreign),
            ("192.168.1.9:3128\n", EmulatorProxy::Foreign),
        ] {
            assert_eq!(
                emulator_proxy(answered(global).as_ref(), answered(&wifi_elsewhere).as_ref(), 7420),
                read
            );
        }
    }

    #[test]
    fn a_proxy_on_a_network_the_emulator_does_not_send_over_routes_nothing() {
        let unset = answered("null\n");
        let on_wifi = connectivity("101", Some("[10.0.2.2] 7420"));
        assert_eq!(
            emulator_proxy(unset.as_ref(), answered(&on_wifi).as_ref(), 7420),
            EmulatorProxy::WifiOnly
        );
        let wifi_default = connectivity("100", Some("[10.0.2.2] 7420"));
        assert_eq!(
            emulator_proxy(unset.as_ref(), answered(&wifi_default).as_ref(), 7420),
            EmulatorProxy::Ours
        );
        let foreign_default = connectivity("100", Some("[10.0.2.2] 3128"));
        assert_eq!(
            emulator_proxy(unset.as_ref(), answered(&foreign_default).as_ref(), 7420),
            EmulatorProxy::Foreign
        );
    }

    #[test]
    fn no_proxy_anywhere_reads_none_and_an_unread_answer_unknown() {
        let nothing = connectivity("101", None);
        for unset in ["null\n", ":0\n", "\n"] {
            assert_eq!(
                emulator_proxy(answered(unset).as_ref(), answered(&nothing).as_ref(), 7420),
                EmulatorProxy::None
            );
        }
        assert_eq!(emulator_proxy(None, answered(&nothing).as_ref(), 7420), EmulatorProxy::Unknown);
        assert_eq!(emulator_proxy(answered("null\n").as_ref(), None, 7420), EmulatorProxy::Unknown);
        assert_eq!(
            emulator_proxy(answered("null\n").as_ref(), exited_non_zero("").as_ref(), 7420),
            EmulatorProxy::Unknown
        );
    }

    #[test]
    fn each_running_emulator_is_named_by_its_avd_and_read_on_its_own_serial() {
        let live = "MIIBpAECAw==";
        let der = der_bytes(live).unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        let read = emulators_read(Some(live), 7420, |args| {
            asked.borrow_mut().push(args.join(" "));
            match args {
                ["devices"] => answered(ADB_DEVICES),
                ["-s", _, "shell", "getprop", "sys.boot_completed"] => answered("1\n"),
                ["-s", "emulator-5554", "shell", "getprop", _] => answered("Pixel_7_API_35\n"),
                ["-s", _, "shell", "getprop", _] => answered("\n"),
                ["-s", "emulator-5554", "shell", script] if script.starts_with("su 0") => {
                    store_answer(&[&der])
                }
                ["-s", _, "shell", script] if script.starts_with("su 0") => answered(""),
                ["-s", "emulator-5554", "shell", "settings", ..] => answered("10.0.2.2:7420\n"),
                ["-s", _, "shell", "settings", ..] => answered("null\n"),
                ["-s", _, "shell", "dumpsys", "connectivity"] => {
                    answered(&connectivity("101", Some("[10.0.2.2] 7420")))
                }
                _ => None,
            }
        })
        .expect("adb answered");
        assert_eq!(
            read.running,
            [
                EmulatorReading::new(
                    "Pixel 7 API 35".into(),
                    Some(Trust::Trusted),
                    &EmulatorProxy::Ours
                ),
                EmulatorReading::new("emulator-5558".into(), None, &EmulatorProxy::WifiOnly),
            ]
        );
        assert!(asked.borrow().iter().all(|a| a == "devices" || a.starts_with("-s emulator-")));
        assert!(asked.borrow().contains(&format!("-s emulator-5554 shell {}", user_ca_script())));
    }

    #[test]
    fn no_adb_is_no_reading_and_no_emulator_is_one() {
        assert_eq!(emulators_read(Some("MIIB"), 7420, |_| None), None);
        assert_eq!(emulators_read(Some("MIIB"), 7420, |_| exited_non_zero("")), None);
        assert_eq!(
            emulators_read(Some("MIIB"), 7420, |_| answered("List of devices attached\n\n")),
            Some(EmulatorsNow { running: vec![], starting: false })
        );
    }

    #[test]
    fn an_emulator_still_booting_is_starting_and_never_read_as_running() {
        let asked = std::cell::RefCell::new(Vec::new());
        let now = emulators_read(Some("MIIB"), 7420, |args| {
            asked.borrow_mut().push(args.join(" "));
            match args {
                ["devices"] => answered("List of devices attached\nemulator-5554\tdevice\n\n"),
                ["-s", _, "shell", "getprop", "sys.boot_completed"] => answered("\n"),
                _ => None,
            }
        });
        assert_eq!(now, Some(EmulatorsNow { running: vec![], starting: true }));
        assert_eq!(
            *asked.borrow(),
            ["devices", "-s emulator-5554 shell getprop sys.boot_completed"]
        );
        let offline = |args: &[&str]| match args {
            ["devices"] => answered("List of devices attached\nemulator-5554\toffline\n\n"),
            _ => None,
        };
        assert_eq!(
            emulators_read(Some("MIIB"), 7420, offline),
            Some(EmulatorsNow { running: vec![], starting: true })
        );
    }

    #[test]
    fn a_certificate_that_does_not_read_leaves_each_emulator_listed_and_its_store_unknown() {
        let read = emulators_read(None, 7420, |args| match args {
            ["devices"] => answered(ADB_DEVICES),
            ["-s", _, "shell", "getprop", "sys.boot_completed"] => answered("1\n"),
            ["-s", _, "shell", "getprop", _] => answered("\n"),
            _ => None,
        })
        .expect("adb answered");
        assert_eq!(read.running.len(), 2);
        assert!(read.starting);
        assert!(read.running.iter().all(|e| e.trust == trust_word(None)));
    }

    const ANDROID_CA: &str = "-----BEGIN CERTIFICATE-----
MIIBjTCCATSgAwIBAgIUEluwT45SZ7hQPwp2s/2SgtYX4IEwCgYIKoZIzj0EAwIw
LTEZMBcGA1UEAwwQVHJhd2xlciBQcm94eSBDQTEQMA4GA1UECgwHVHJhd2xlcjAe
Fw0yNjExMDEwNjA5NTVaFw0zNjEwMjkwNjA5NTVaMC0xGTAXBgNVBAMMEFRyYXds
ZXIgUHJveHkgQ0ExEDAOBgNVBAoMB1RyYXdsZXIwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAAQH6bjS3GrJ72fOHT2PtTt1sj9OHagtIoF5erErXdK04ckqSgXCxWNP
AG886uAwCrn33L+l2vQyCCnJrUIyCQGJozIwMDAdBgNVHQ4EFgQUijMZ8kLjBgTM
bcHkeIM1PHjeaiIwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQDAgNHADBEAiAs
k+Jq39rckTYg7cUtzO/mFYYtpuuo+0MW+mC0/NjqZQIgBvaqdde8xAjOw9nqsBrB
NecwLoyuObd5tf9Qwxdkolo=
-----END CERTIFICATE-----";

    #[test]
    fn a_user_ca_is_named_as_openssl_names_it_for_android() {
        let der = der_bytes(&live_body(ANDROID_CA).unwrap()).unwrap();
        assert_eq!(subject_hash_old(&der).as_deref(), Some("d082df7e"));
        assert_eq!(subject_hash_old(&[0x30, 0x03, 0x02, 0x01, 0x01]), None);
        assert_eq!(subject_hash_old(&[]), None);
    }

    fn two_emulators() -> Emulators {
        Emulators {
            adb: "/sdk/platform-tools/adb".into(),
            emulator: "/sdk/emulator/emulator".into(),
            running: vec!["emulator-5554".into(), "emulator-5556".into()],
            last_avd: None,
        }
    }

    #[test]
    fn the_install_pushes_the_der_and_writes_it_as_root_on_every_running_emulator() {
        let commands = emulator_install_commands(
            &two_emulators(),
            Path::new("/tmp/trawler-ca.der"),
            "d082df7e",
        );
        let script = user_ca_install_script("d082df7e");
        let on = |serial: &str| {
            [
                argv(&[
                    "/sdk/platform-tools/adb",
                    "-s",
                    serial,
                    "push",
                    "/tmp/trawler-ca.der",
                    STAGED_DER,
                ]),
                argv(&["/sdk/platform-tools/adb", "-s", serial, "shell", &script]),
            ]
        };
        assert_eq!(commands, [on("emulator-5554"), on("emulator-5556")].concat());
        for part in [
            "su 0 sh -c",
            "/data/misc/user/0/cacerts-added",
            "d082df7e.$i",
            "! cmp -s $t",
            "chown system:system",
            "chmod 644",
            "restorecon -R",
            "am broadcast -a android.security.action.TRUST_STORE_CHANGED",
        ] {
            assert!(script.contains(part), "{part}");
        }
        assert_eq!(emulator_install_commands(&Emulators::default(), Path::new("/t"), "h").len(), 0);
    }

    #[test]
    fn routing_sets_the_global_proxy_on_every_running_emulator_and_unrouting_clears_it() {
        let set = |serial: &str, value: &str| {
            argv(&[
                "/sdk/platform-tools/adb",
                "-s",
                serial,
                "shell",
                "settings",
                "put",
                "global",
                "http_proxy",
                value,
            ])
        };
        assert_eq!(
            emulator_route_commands(&two_emulators(), "10.0.2.2:7420"),
            [set("emulator-5554", "10.0.2.2:7420"), set("emulator-5556", "10.0.2.2:7420")]
        );
        assert_eq!(
            emulator_route_commands(&two_emulators(), ":0"),
            [set("emulator-5554", ":0"), set("emulator-5556", ":0")]
        );
    }

    #[test]
    fn no_emulator_running_is_refused_before_any_command_runs() {
        assert_eq!(running(Emulators::default()).unwrap_err(), setup_error(NO_EMULATOR_RUNNING));
        assert!(running(two_emulators()).is_ok());
    }

    #[test]
    fn the_sdk_is_spelled_as_the_users_own_shell_writes_it() {
        let vars = |key: &str| match key {
            "HOME" => Some("/Users/me".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\me\AppData\Local".into()),
            _ => None,
        };
        assert_eq!(
            spelled_sdk("macos", Path::new("/Users/me/Library/Android/sdk"), vars).as_deref(),
            Some("$HOME/Library/Android/sdk")
        );
        assert_eq!(
            spelled_sdk("windows", Path::new(r"C:\Users\me\AppData\Local\Android\Sdk"), vars)
                .as_deref(),
            Some(r"$env:LOCALAPPDATA\Android\Sdk")
        );
        assert_eq!(
            spelled_sdk("linux", Path::new("/opt/android-sdk"), vars).as_deref(),
            Some("/opt/android-sdk")
        );
        assert_eq!(
            spelled_sdk("macos", Path::new("/Users/meme/sdk"), vars).as_deref(),
            Some("/Users/meme/sdk")
        );
        assert_eq!(spelled_sdk("macos", Path::new("/Users/me/$dk"), vars), None);
        assert_eq!(spelled_sdk("linux", Path::new("/opt/\"sdk"), vars), None);
    }

    fn avds(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("trawler-test-avds-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn avd(home: &Path, id: &str, config: &str, launched_secs_ago: Option<u64>) {
        let dir = home.join(format!("{id}.avd"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(home.join(format!("{id}.ini")), format!("path={}\n", dir.display()))
            .unwrap();
        std::fs::write(dir.join("config.ini"), config).unwrap();
        if let Some(ago) = launched_secs_ago {
            let launched = std::fs::File::create(dir.join(AVD_LAUNCHED)).unwrap();
            launched
                .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(ago))
                .unwrap();
        }
    }

    #[test]
    fn the_avd_used_last_is_the_one_whose_launch_file_is_newest_by_its_display_name() {
        let home = avds("last");
        avd(&home, "Pixel_7_API_35", "avd.ini.displayname=Pixel 7 API 35\n", Some(3_600));
        avd(&home, "Pixel_Fold_API_34", "hw.lcd.width=1080\n", Some(60));
        assert_eq!(
            last_avd(&home),
            Some(Avd {
                id: "Pixel_Fold_API_34".into(),
                name: "Pixel Fold API 34".into(),
                play_store: false
            })
        );
        avd(&home, "Pixel_7_API_35", "avd.ini.displayname=Pixel 7 API 35\n", Some(1));
        assert_eq!(
            last_avd(&home),
            Some(Avd {
                id: "Pixel_7_API_35".into(),
                name: "Pixel 7 API 35".into(),
                play_store: false
            })
        );
        assert_eq!(last_avd(&avds("none")), None);
        assert_eq!(last_avd(&home.join("missing")), None);
    }

    #[test]
    fn a_google_play_image_is_told_by_its_config() {
        let home = avds("play");
        avd(
            &home,
            "Pixel_8_Play",
            "PlayStore.enabled=yes\ntag.id=google_apis_playstore\n",
            Some(1),
        );
        assert!(last_avd(&home).is_some_and(|avd| avd.play_store));
        avd(&home, "Pixel_8_Play", "PlayStore.enabled=true\n", Some(1));
        assert!(last_avd(&home).is_some_and(|avd| avd.play_store));
        avd(&home, "Pixel_8_Play", "PlayStore.enabled=no\ntag.id=google_apis\n", Some(1));
        assert!(last_avd(&home).is_some_and(|avd| !avd.play_store));
    }

    #[test]
    fn the_avd_folder_is_where_android_studio_points_it() {
        let vars = |key: &str| match key {
            "HOME" => Some("/Users/me".into()),
            "USERPROFILE" => Some(r"C:\Users\me".into()),
            _ => None,
        };
        assert_eq!(avd_home("macos", vars), Some(PathBuf::from("/Users/me/.android/avd")));
        assert_eq!(
            avd_home("windows", vars),
            Some(PathBuf::from(r"C:\Users\me").join(".android").join("avd"))
        );
        assert_eq!(
            avd_home("linux", |key| (key == "ANDROID_USER_HOME").then(|| "/u".into())),
            Some(PathBuf::from("/u/avd"))
        );
        assert_eq!(
            avd_home("linux", |key| (key != "HOME").then(|| "/a".into())),
            Some(PathBuf::from("/a"))
        );
    }

    // ── The proxy HTTPS goes through now (D3 · D5 · D8) ─────────────────────

    const SCUTIL_ON: &str = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n    1 : 169.254/16\n  }\n  FTPPassive : 1\n  HTTPEnable : 1\n  HTTPPort : 7420\n  HTTPProxy : 127.0.0.1\n  HTTPSEnable : 1\n  HTTPSPort : 7420\n  HTTPSProxy : 127.0.0.1\n}\n";
    const SCUTIL_OFF: &str = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n  }\n  FTPPassive : 1\n  HTTPEnable : 0\n  HTTPSEnable : 0\n}\n";
    const SCUTIL_HTTP_ONLY: &str = "<dictionary> {\n  HTTPEnable : 1\n  HTTPPort : 7420\n  HTTPProxy : 127.0.0.1\n  HTTPSEnable : 0\n}\n";
    const SCUTIL_SCOPED: &str = "<dictionary> {\n  HTTPSEnable : 0\n  __SCOPED__ : <dictionary> {\n    en1 : <dictionary> {\n      HTTPSEnable : 1\n      HTTPSPort : 7420\n      HTTPSProxy : 127.0.0.1\n    }\n  }\n}\n";

    #[test]
    fn macos_reads_the_https_proxy_it_applies_now() {
        assert_eq!(
            scutil_proxy(answered(SCUTIL_ON).as_ref()),
            ProxyRead::Set("127.0.0.1".into(), 7420)
        );
        assert_eq!(scutil_proxy(answered(SCUTIL_OFF).as_ref()), ProxyRead::Off);
    }

    // D3: HTTPS decides, and only what applies now counts.
    #[test]
    fn neither_an_http_proxy_nor_an_idle_services_proxy_routes_https() {
        assert_eq!(scutil_proxy(answered(SCUTIL_HTTP_ONLY).as_ref()), ProxyRead::Off);
        assert_eq!(scutil_proxy(answered(SCUTIL_SCOPED).as_ref()), ProxyRead::Off);
        let no_port = "<dictionary> {\n  HTTPSEnable : 1\n  HTTPSProxy : 127.0.0.1\n}\n";
        assert_eq!(scutil_proxy(answered(no_port).as_ref()), ProxyRead::Off);
    }

    #[test]
    fn macos_cannot_say_when_scutil_did_not_answer() {
        assert_eq!(scutil_proxy(None), ProxyRead::Unreadable);
        assert_eq!(scutil_proxy(exited_non_zero(SCUTIL_ON).as_ref()), ProxyRead::Unreadable);
        assert_eq!(scutil_proxy(answered("").as_ref()), ProxyRead::Unreadable);
    }

    fn gsettings(
        values: &'static [(&'static str, &'static str)],
    ) -> impl FnMut(&str, &str) -> Option<Ran> {
        move |schema, key| {
            let asked = format!("{schema} {key}");
            values
                .iter()
                .find(|(k, _)| *k == asked)
                .map(|(_, v)| Ran { ok: true, stdout: format!("{v}\n") })
        }
    }

    #[test]
    fn gnome_reads_its_manual_https_proxy() {
        let read = gnome_proxy(
            Some("ubuntu:GNOME"),
            gsettings(&[
                ("org.gnome.system.proxy mode", "'manual'"),
                ("org.gnome.system.proxy.https host", "'127.0.0.1'"),
                ("org.gnome.system.proxy.https port", "7420"),
            ]),
        );
        assert_eq!(read, ProxyRead::Set("127.0.0.1".into(), 7420));
    }

    #[test]
    fn gnome_with_no_https_proxy_is_off() {
        assert_eq!(
            gnome_proxy(Some("GNOME"), gsettings(&[("org.gnome.system.proxy mode", "'none'")])),
            ProxyRead::Off
        );
        let manual_without_https = gsettings(&[
            ("org.gnome.system.proxy mode", "'manual'"),
            ("org.gnome.system.proxy.https host", "''"),
            ("org.gnome.system.proxy.https port", "0"),
        ]);
        assert_eq!(gnome_proxy(Some("GNOME"), manual_without_https), ProxyRead::Off);
    }

    #[test]
    fn gnome_cannot_say_for_an_automatic_proxy_a_failed_read_or_another_desktop() {
        assert_eq!(
            gnome_proxy(Some("GNOME"), gsettings(&[("org.gnome.system.proxy mode", "'auto'")])),
            ProxyRead::Unreadable
        );
        assert_eq!(gnome_proxy(Some("GNOME"), gsettings(&[])), ProxyRead::Unreadable);
        let never = |_: &str, _: &str| -> Option<Ran> {
            unreachable!("another desktop keeps its proxy elsewhere")
        };
        assert_eq!(gnome_proxy(Some("KDE"), never), ProxyRead::Unreadable);
        assert_eq!(gnome_proxy(None, never), ProxyRead::Unreadable);
    }

    #[test]
    fn a_host_with_no_reader_cannot_say_which_proxy_is_in_use() {
        assert_eq!(proxy_in_use_now("freebsd"), ProxyRead::Unreadable);
    }

    // D5: localhost resolves to the address Trawler listens on.
    #[test]
    fn localhost_on_trawlers_port_is_trawler() {
        for set in ["localhost", "LOCALHOST", "127.0.0.1"] {
            let read = ProxyRead::Set(set.into(), 7420);
            assert_eq!(classify(&read, "127.0.0.1", 7420, &[]), ProxyStatus::Ours, "{set}");
        }
        assert_eq!(
            classify(&ProxyRead::Set("localhost".into(), 8080), "127.0.0.1", 7420, &[]),
            ProxyStatus::Foreign
        );
        assert_eq!(
            classify(&ProxyRead::Set("localhost.evil".into(), 7420), "127.0.0.1", 7420, &[]),
            ProxyStatus::Foreign
        );
        assert_eq!(
            classify(&ProxyRead::Set("192.168.1.24".into(), 7420), "127.0.0.1", 7420, &[]),
            ProxyStatus::Foreign
        );
    }

    #[test]
    fn another_loopback_address_reaches_nothing_trawler_listens_on() {
        for set in ["127.0.0.2", "::1", "[::1]"] {
            let read = ProxyRead::Set(set.into(), 7420);
            assert_eq!(classify(&read, "127.0.0.1", 7420, &[]), ProxyStatus::Foreign, "{set}");
        }
    }

    // W11's D3: this machine's own address at Trawler's port reaches the same
    // listener, so a quit reads it as Trawler's; another machine's never is.
    #[test]
    fn this_machines_own_address_on_trawlers_port_is_trawler() {
        let own = ["192.168.1.24".to_string(), "fe80::1".to_string()];
        for set in ["192.168.1.24", "[fe80::1]", "FE80::1"] {
            let read = ProxyRead::Set(set.into(), 7420);
            assert_eq!(classify(&read, "127.0.0.1", 7420, &own), ProxyStatus::Ours, "{set}");
        }
        assert_eq!(
            classify(&ProxyRead::Set("192.168.1.24".into(), 8080), "127.0.0.1", 7420, &own),
            ProxyStatus::Foreign
        );
        assert_eq!(
            classify(&ProxyRead::Set("192.168.1.25".into(), 7420), "127.0.0.1", 7420, &own),
            ProxyStatus::Foreign
        );
        assert_eq!(
            read_proxy_among(&ProxyRead::Set("192.168.1.24".into(), 7420), "127.0.0.1", 7420, &own),
            ProxyReading { status: "ours", address: Some("192.168.1.24:7420".into()) }
        );
    }

    #[test]
    fn quit_reads_gnomes_proxy_on_linux_as_setup_does() {
        assert_eq!(active_system_proxy("linux", trawler), proxy_in_use_now("linux"));
    }
}
