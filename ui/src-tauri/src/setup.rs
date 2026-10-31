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
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err(setup_error("not a PEM certificate"));
    }
    let cert = std::env::temp_dir().join("trawler-ca.pem");
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
    let verified =
        Command::new(&argv[0]).args(&argv[1..]).output().ok().map(|out| out.status.success());
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

// The running devices, by the name the Simulator's own window shows, each with
// the store it trusts from.
fn running_simulators(devices: &Path) -> Vec<(String, std::path::PathBuf)> {
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
            Some((plist_value(&plist, "name")?, entry.path().join(SIMULATOR_TRUST_STORE)))
        })
        .collect();
    running.sort();
    running
}

fn der_hex(live: &str) -> Option<String> {
    use base64::Engine as _;
    let der = base64::engine::general_purpose::STANDARD.decode(live).ok()?;
    Some(der.iter().map(|byte| format!("{byte:02X}")).collect())
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

fn simulators_trust(
    devices: &Path,
    live: &str,
    query: impl Fn(&Path, &str) -> Option<Ran>,
) -> Vec<SimulatorTrust> {
    let der_hex = der_hex(live);
    running_simulators(devices)
        .into_iter()
        .map(|(name, store)| {
            let reading =
                der_hex.as_deref().and_then(|hex| simulator_store_trust(&store, hex, &query));
            SimulatorTrust::new(name, reading)
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
    match (std::env::consts::OS, home, live_body(&pem)) {
        ("macos", Some(home), Some(live)) => {
            simulators_trust(&Path::new(&home).join(SIMULATOR_DEVICES), &live, sqlite_read)
        }
        _ => Vec::new(),
    }
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
fn adb_candidates(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let exe = if os == "windows" { "adb.exe" } else { "adb" };
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
        .map(|sdk| sdk.join("platform-tools").join(exe))
        .collect()
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

fn emulators_read(
    live: &str,
    port: u16,
    adb: impl Fn(&[&str]) -> Option<Ran>,
) -> Vec<EmulatorReading> {
    let Some(devices) = adb(&["devices"]).filter(|out| out.ok) else {
        return Vec::new();
    };
    let der = der_bytes(live);
    let store = user_ca_script();
    running_emulators(&devices.stdout)
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
        .collect()
}

// Read-only probe: which emulators run, does each one's own store hold the
// certificate the core serves now, and where does each send its traffic? None
// running is the screen's unread, and its tasks fall back to its marked request
// (DEC-63). The window asks only while the emulator is the platform chosen:
// adb starts a server of its own where none runs.
#[cfg_attr(not(feature = "scenario"), tauri::command(async))]
pub fn emulator_readings(pem: String, port: u16) -> Vec<EmulatorReading> {
    let Some(live) = live_body(&pem) else {
        return Vec::new();
    };
    let os = std::env::consts::OS;
    let adb = adb_candidates(os, |key| std::env::var_os(key))
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from("adb"));
    let adb = adb.to_string_lossy();
    emulators_read(&live, port, |args| ran(&adb, args))
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

fn classify(read: &ProxyRead, host: &str, port: u16, own: &[String]) -> ProxyStatus {
    match read {
        ProxyRead::Set(s, p) if *p == port && names_trawler(s, host, own) => ProxyStatus::Ours,
        ProxyRead::Set(..) => ProxyStatus::Foreign,
        ProxyRead::Off => ProxyStatus::None,
        ProxyRead::Unreadable => ProxyStatus::Unknown,
    }
}

// Quit's reader, and deliberately wider than setup's: any service, HTTPS then
// HTTP. A service not in use now that still points at Trawler strands the
// machine the day it becomes the one in use, so quit may not miss it. A read
// that failed means the services that did answer cannot vouch for a clean
// system.
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
        "windows" => active_system_proxy(os),
        "linux" => gnome_proxy_now(),
        _ => ProxyRead::Unreadable,
    }
}

fn gnome_proxy_now() -> ProxyRead {
    gnome_proxy(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(), |schema, key| {
        ran("gsettings", &["get", schema, key])
    })
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
    read_proxy_among(&active_system_proxy(std::env::consts::OS), &host, port, &own)
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
        assert!(pem_blocks("").is_empty());
        assert!(pem_blocks("security: SecKeychainSearchCopyNext: not found").is_empty());
        assert!(pem_blocks("-----BEGIN CERTIFICATE-----\ntruncated").is_empty());
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
        simulators_trust(&devices, "MIIB", |_, sql| {
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
        let read = simulators_trust(&devices, "MIIB", |store, _| {
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
            simulators_trust(&devices, "MIIB", never_queried),
            [SimulatorTrust::new("iPhone 17 Pro".into(), None)]
        );
        simulator(&devices, "A", "iPhone 17 Pro", 3, true);
        for answer in [Some((false, "")), Some((true, "Error: no such table: tsettings")), None] {
            assert_eq!(
                simulators_trust(&devices, "MIIB", |_, _| {
                    answer.map(|(ok, stdout)| Ran { ok, stdout: stdout.into() })
                }),
                [SimulatorTrust::new("iPhone 17 Pro".into(), None)]
            );
        }
        let _ = std::fs::remove_dir_all(&devices);
    }

    #[test]
    fn no_simulator_running_or_none_installed_reads_nothing() {
        let devices = simulators("none");
        simulator(&devices, "A", "iPhone 17 Pro", 1, true);
        assert_eq!(simulators_trust(&devices, "MIIB", never_queried), []);
        assert_eq!(simulators_trust(&devices.join("absent"), "MIIB", never_queried), []);
        let _ = std::fs::remove_dir_all(&devices);
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
        assert!(adb_candidates("freebsd", |_| None).is_empty());
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
        let read = emulators_read(live, 7420, |args| {
            asked.borrow_mut().push(args.join(" "));
            match args {
                ["devices"] => answered(ADB_DEVICES),
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
        });
        assert_eq!(
            read,
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
    fn no_adb_or_no_emulator_reads_nothing() {
        assert_eq!(emulators_read("MIIB", 7420, |_| None), []);
        assert_eq!(emulators_read("MIIB", 7420, |_| exited_non_zero("")), []);
        assert_eq!(emulators_read("MIIB", 7420, |_| answered("List of devices attached\n\n")), []);
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
        assert_eq!(active_system_proxy("linux"), proxy_in_use_now("linux"));
    }
}
