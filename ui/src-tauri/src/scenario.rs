// The walkthrough build (E-ONBREV.W1): every command that reads or changes this
// machine answers from a script the harness sets, so one Mac can film the setup
// screen as every host and every probe reading would show it. The answers go
// through the shell's own classifiers — the trust word, the proxy reader, the
// stderr reading of an elevated tool — so a fix to any of those shows in the
// recording without this module changing.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Deserialize;

use crate::setup::{
    command_failed, read_proxy, read_proxy_among, setup_error, trust_word, Avd, EmulatorProxy,
    EmulatorRead, EmulatorReading, ProxyRead, ProxyReading, SetupError, SimulatorTrust, Trust,
    NO_EMULATOR_INSTALL_DETAIL, NO_EMULATOR_ROUTE_DETAIL, NO_EMULATOR_RUNNING,
    NO_EMULATOR_UNROUTE_DETAIL, NO_INSTALL_DETAIL, NO_REMOVE_DETAIL, NO_SET_PROXY_DETAIL,
    NO_SIMULATOR_RUNNING, NO_SIMULATOR_TRUST_DETAIL, NO_UNSET_PROXY_DETAIL,
};

const HOST_VAR: &str = "TRAWLER_SCENARIO_HOST";
const SCRIPT_VAR: &str = "TRAWLER_SCENARIO_SCRIPT";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustOs {
    Trusted,
    Installed,
    #[default]
    Absent,
    Unreadable,
}

impl TrustOs {
    fn reading(self) -> Option<Trust> {
        match self {
            TrustOs::Trusted => Some(Trust::Trusted),
            TrustOs::Installed => Some(Trust::Installed),
            TrustOs::Absent => Some(Trust::Absent),
            TrustOs::Unreadable => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SimulatorOs {
    name: String,
    trust: TrustOs,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EmulatorProxyOs {
    Ours,
    WifiOnly,
    Foreign,
    None,
    Unreadable,
}

impl EmulatorProxyOs {
    fn reading(self) -> EmulatorProxy {
        match self {
            EmulatorProxyOs::Ours => EmulatorProxy::Ours,
            EmulatorProxyOs::WifiOnly => EmulatorProxy::WifiOnly,
            EmulatorProxyOs::Foreign => EmulatorProxy::Foreign,
            EmulatorProxyOs::None => EmulatorProxy::None,
            EmulatorProxyOs::Unreadable => EmulatorProxy::Unknown,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct EmulatorOs {
    name: String,
    trust: TrustOs,
    proxy: EmulatorProxyOs,
}

// The SDK as this host finds it: whether adb answers, the SDK its commands name,
// and the AVD used last.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AndroidOs {
    adb: bool,
    sdk: Option<String>,
    avd: Option<AvdOs>,
    #[serde(default)]
    starting: bool,
}

impl Default for AndroidOs {
    fn default() -> Self {
        Self { adb: true, sdk: None, avd: None, starting: false }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AvdOs {
    id: String,
    name: String,
    #[serde(default, rename = "playStore")]
    play_store: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyOs {
    #[default]
    Unset,
    Unreadable,
    Set {
        host: String,
        port: u16,
    },
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    #[default]
    Ok,
    Fails {
        stderr: String,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scenario {
    trust: TrustOs,
    simulators: Vec<SimulatorOs>,
    emulators: Vec<EmulatorOs>,
    android: AndroidOs,
    proxy: ProxyOs,
    set_up: Outcome,
    set_proxy: Outcome,
    unset_proxy: Outcome,
    remove_ca: Outcome,
    trust_simulators: Outcome,
    install_emulators: Outcome,
    route_emulators: Outcome,
    unroute_emulators: Outcome,
    allow_reach: bool,
    allow_replace: bool,
    interface_names: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    trust: Option<TrustOs>,
    simulators: Option<Vec<SimulatorOs>>,
    emulators: Option<Vec<EmulatorOs>>,
    android: Option<AndroidOs>,
    proxy: Option<ProxyOs>,
    set_up: Option<Outcome>,
    set_proxy: Option<Outcome>,
    unset_proxy: Option<Outcome>,
    remove_ca: Option<Outcome>,
    trust_simulators: Option<Outcome>,
    install_emulators: Option<Outcome>,
    route_emulators: Option<Outcome>,
    unroute_emulators: Option<Outcome>,
    allow_reach: Option<bool>,
    allow_replace: Option<bool>,
    interface_names: Option<HashMap<String, String>>,
}

impl Scenario {
    fn apply(&mut self, patch: Patch) {
        let Patch {
            trust,
            simulators,
            emulators,
            android,
            proxy,
            set_up,
            set_proxy,
            unset_proxy,
            remove_ca,
            trust_simulators,
            install_emulators,
            route_emulators,
            unroute_emulators,
            allow_reach,
            allow_replace,
            interface_names,
        } = patch;
        if let Some(v) = trust {
            self.trust = v;
        }
        if let Some(v) = simulators {
            self.simulators = v;
        }
        if let Some(v) = emulators {
            self.emulators = v;
        }
        if let Some(v) = android {
            self.android = v;
        }
        if let Some(v) = proxy {
            self.proxy = v;
        }
        if let Some(v) = set_up {
            self.set_up = v;
        }
        if let Some(v) = set_proxy {
            self.set_proxy = v;
        }
        if let Some(v) = unset_proxy {
            self.unset_proxy = v;
        }
        if let Some(v) = remove_ca {
            self.remove_ca = v;
        }
        if let Some(v) = trust_simulators {
            self.trust_simulators = v;
        }
        if let Some(v) = install_emulators {
            self.install_emulators = v;
        }
        if let Some(v) = route_emulators {
            self.route_emulators = v;
        }
        if let Some(v) = unroute_emulators {
            self.unroute_emulators = v;
        }
        if let Some(v) = allow_reach {
            self.allow_reach = v;
        }
        if let Some(v) = allow_replace {
            self.allow_replace = v;
        }
        if let Some(v) = interface_names {
            self.interface_names = v;
        }
    }

    fn trust_reading(&self) -> &'static str {
        trust_word(self.trust.reading())
    }

    fn simulators_reading(&self) -> Vec<SimulatorTrust> {
        self.simulators
            .iter()
            .map(|s| SimulatorTrust::new(s.name.clone(), s.trust.reading()))
            .collect()
    }

    fn emulators_reading(&self) -> Vec<EmulatorReading> {
        self.emulators
            .iter()
            .map(|e| EmulatorReading::new(e.name.clone(), e.trust.reading(), &e.proxy.reading()))
            .collect()
    }

    fn emulator_read(&self) -> EmulatorRead {
        let AndroidOs { adb, sdk, avd, starting } = &self.android;
        EmulatorRead {
            adb: *adb,
            sdk: sdk.clone(),
            avd: avd.as_ref().map(|a| Avd {
                id: a.id.clone(),
                name: a.name.clone(),
                play_store: a.play_store,
            }),
            emulators: if *adb { self.emulators_reading() } else { Vec::new() },
            starting: *adb && *starting,
        }
    }

    fn emulators_running(&self) -> Result<(), SetupError> {
        if self.android.adb && !self.emulators.is_empty() {
            Ok(())
        } else {
            Err(setup_error(NO_EMULATOR_RUNNING))
        }
    }

    // An image with no su refuses the write, as the real one does.
    fn install_emulators(&mut self) -> Result<(), SetupError> {
        self.emulators_running()?;
        run(&self.install_emulators, NO_EMULATOR_INSTALL_DETAIL)?;
        if self.emulators.iter().any(|e| e.trust == TrustOs::Unreadable) {
            return Err(command_failed(
                "/system/bin/sh: su: inaccessible or not found",
                NO_EMULATOR_INSTALL_DETAIL,
            ));
        }
        for emulator in &mut self.emulators {
            emulator.trust = TrustOs::Trusted;
        }
        Ok(())
    }

    fn route_emulators(
        &mut self,
        proxy: EmulatorProxyOs,
        outcome: &Outcome,
        no_detail: &str,
    ) -> Result<(), SetupError> {
        self.emulators_running()?;
        run(outcome, no_detail)?;
        for emulator in &mut self.emulators {
            emulator.proxy = proxy;
        }
        Ok(())
    }

    fn proxy_read(&self) -> ProxyRead {
        match &self.proxy {
            ProxyOs::Set { host, port } => ProxyRead::Set(host.clone(), *port),
            ProxyOs::Unset => ProxyRead::Off,
            ProxyOs::Unreadable => ProxyRead::Unreadable,
        }
    }

    fn proxy_reading(&self, host: &str, port: u16) -> ProxyReading {
        read_proxy(&self.proxy_read(), host, port)
    }

    fn quit_reading(&self, host: &str, port: u16, own: &[String]) -> ProxyReading {
        read_proxy_among(&self.proxy_read(), host, port, own)
    }

    fn set_up(&mut self, host: String, port: u16) -> Result<(), SetupError> {
        run(&self.set_up, NO_INSTALL_DETAIL)?;
        self.trust = TrustOs::Trusted;
        self.proxy = ProxyOs::Set { host, port };
        Ok(())
    }

    fn set_proxy(&mut self, host: String, port: u16) -> Result<(), SetupError> {
        run(&self.set_proxy, NO_SET_PROXY_DETAIL)?;
        self.proxy = ProxyOs::Set { host, port };
        Ok(())
    }

    fn unset_proxy(&mut self) -> Result<(), SetupError> {
        run(&self.unset_proxy, NO_UNSET_PROXY_DETAIL)?;
        self.proxy = ProxyOs::Unset;
        Ok(())
    }

    fn trust_simulators(&mut self) -> Result<(), SetupError> {
        if self.simulators.is_empty() {
            return Err(crate::setup::setup_error(NO_SIMULATOR_RUNNING));
        }
        run(&self.trust_simulators, NO_SIMULATOR_TRUST_DETAIL)?;
        for simulator in &mut self.simulators {
            simulator.trust = TrustOs::Trusted;
        }
        Ok(())
    }

    fn remove_ca(&mut self) -> Result<(), SetupError> {
        run(&self.remove_ca, NO_REMOVE_DETAIL)?;
        self.trust = TrustOs::Absent;
        Ok(())
    }
}

fn run(outcome: &Outcome, no_detail: &str) -> Result<(), SetupError> {
    match outcome {
        Outcome::Ok => Ok(()),
        Outcome::Fails { stderr } => Err(command_failed(stderr, no_detail)),
    }
}

type Shared<'a> = tauri::State<'a, Mutex<Scenario>>;

fn with<T>(state: &Shared<'_>, f: impl FnOnce(&mut Scenario) -> T) -> T {
    f(&mut state.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

#[tauri::command]
pub fn scenario_script(state: Shared<'_>, patch: Patch) {
    with(&state, |s| s.apply(patch));
}

#[tauri::command]
pub fn ca_trusted(state: Shared<'_>) -> String {
    with(&state, |s| s.trust_reading().into())
}

#[tauri::command]
pub fn simulator_trust(state: Shared<'_>) -> Vec<SimulatorTrust> {
    with(&state, |s| s.simulators_reading())
}

#[tauri::command]
pub fn emulator_readings(state: Shared<'_>) -> EmulatorRead {
    with(&state, |s| s.emulator_read())
}

#[tauri::command]
pub fn install_on_emulators(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, Scenario::install_emulators)
}

#[tauri::command]
pub fn route_emulators(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, |s| {
        let outcome = s.route_emulators.clone();
        s.route_emulators(EmulatorProxyOs::Ours, &outcome, NO_EMULATOR_ROUTE_DETAIL)
    })
}

#[tauri::command]
pub fn unroute_emulators(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, |s| {
        let outcome = s.unroute_emulators.clone();
        s.route_emulators(EmulatorProxyOs::None, &outcome, NO_EMULATOR_UNROUTE_DETAIL)
    })
}

#[tauri::command]
pub fn proxy_status(state: Shared<'_>, host: String, port: u16, own: Vec<String>) -> ProxyReading {
    with(&state, |s| s.quit_reading(&host, port, &own))
}

// One scripted proxy answers both questions: the walkthrough films a machine
// whose only service is the one in use.
#[tauri::command]
pub fn proxy_in_use(state: Shared<'_>, host: String, port: u16) -> ProxyReading {
    with(&state, |s| s.proxy_reading(&host, port))
}

#[tauri::command]
pub fn set_up_this_machine(state: Shared<'_>, host: String, port: u16) -> Result<(), SetupError> {
    with(&state, |s| s.set_up(host, port))
}

#[tauri::command]
pub fn set_proxy(state: Shared<'_>, host: String, port: u16) -> Result<(), SetupError> {
    with(&state, |s| s.set_proxy(host, port))
}

#[tauri::command]
pub fn unset_proxy(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, Scenario::unset_proxy)
}

#[tauri::command]
pub fn trust_simulators(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, Scenario::trust_simulators)
}

#[tauri::command]
pub fn remove_ca(state: Shared<'_>) -> Result<(), SetupError> {
    with(&state, Scenario::remove_ca)
}

// A filmed route opens no real screen: what it would open is the shell's own
// unit tests' to show.
#[tauri::command]
pub fn open_route(route: crate::route::Route) -> Result<(), SetupError> {
    let _ = route;
    Ok(())
}

#[tauri::command]
pub fn interface_names(state: Shared<'_>) -> HashMap<String, String> {
    with(&state, |s| s.interface_names.clone())
}

#[tauri::command]
pub fn confirm_reach(state: Shared<'_>) -> bool {
    with(&state, |s| s.allow_reach)
}

#[tauri::command]
pub fn confirm_replace_proxy(state: Shared<'_>) -> bool {
    with(&state, |s| s.allow_replace)
}

fn user_agent(host: &str) -> Result<&'static str, String> {
    match host {
        "macos" => {
            Ok("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)")
        }
        "windows" => {
            Ok("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)")
        }
        "linux" => Ok("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)"),
        other => Err(format!("{HOST_VAR}={other:?} names no host the walkthrough films")),
    }
}

fn host_script(host: Option<&str>) -> Result<Option<String>, String> {
    let Some(host) = host.map(str::trim).filter(|h| !h.is_empty()) else {
        return Ok(None);
    };
    let agent = serde_json::Value::from(user_agent(host)?);
    Ok(Some(format!(
        "Object.defineProperty(navigator, \"userAgent\", {{ get: () => {agent}, configurable: true }});"
    )))
}

fn host_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    let builder = tauri::plugin::Builder::new("trawler-scenario-host");
    match host_script(std::env::var(HOST_VAR).ok().as_deref()).unwrap_or_else(|e| panic!("{e}")) {
        Some(script) => builder.js_init_script(script),
        None => builder,
    }
    .build()
}

// The window reads the interface names and takes its first probes as it mounts,
// before a harness can reach it, so a launch's opening readings travel in the
// environment like its host does.
fn opening(script: Option<&str>) -> Result<Scenario, String> {
    let mut scenario = Scenario::default();
    if let Some(script) = script.map(str::trim).filter(|s| !s.is_empty()) {
        let patch = serde_json::from_str(script).map_err(|e| format!("{SCRIPT_VAR}: {e}"))?;
        scenario.apply(patch);
    }
    Ok(scenario)
}

pub fn install(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let scenario =
        opening(std::env::var(SCRIPT_VAR).ok().as_deref()).unwrap_or_else(|e| panic!("{e}"));
    builder.manage(Mutex::new(scenario)).plugin(host_plugin()).invoke_handler(
        tauri::generate_handler![
            scenario_script,
            ca_trusted,
            simulator_trust,
            emulator_readings,
            set_proxy,
            set_up_this_machine,
            unset_proxy,
            remove_ca,
            trust_simulators,
            install_on_emulators,
            route_emulators,
            unroute_emulators,
            proxy_status,
            proxy_in_use,
            interface_names,
            open_route,
            crate::setup::notice_dismissed,
            crate::setup::dismiss_notice,
            crate::setup::notes_shown,
            crate::setup::show_notes,
            crate::layout::layout_read,
            crate::layout::layout_write,
            confirm_reach,
            confirm_replace_proxy,
            crate::core::core_status,
            crate::core::core_log,
            crate::core::restart_core,
            crate::settings::open_settings
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patched(json: &str) -> Scenario {
        let mut s = Scenario::default();
        s.apply(serde_json::from_str(json).expect("a patch"));
        s
    }

    #[test]
    fn a_fresh_scenario_is_a_machine_with_no_certificate_and_no_proxy_that_refuses_every_confirm() {
        let s = Scenario::default();
        assert_eq!(s.trust_reading(), "absent");
        assert_eq!(
            s.proxy_reading("127.0.0.1", 7420),
            read_proxy(&ProxyRead::Off, "127.0.0.1", 7420)
        );
        assert!(!s.allow_reach && !s.allow_replace);
        assert_eq!(s.simulators_reading(), []);
        assert_eq!(s.emulators_reading(), []);
    }

    #[test]
    fn trusting_the_simulators_trusts_every_one_running_and_fails_as_scripted() {
        let mut s = patched(
            r#"{"simulators":[{"name":"iPhone 17 Pro","trust":"absent"},{"name":"iPad mini","trust":"unreadable"}]}"#,
        );
        s.trust_simulators().expect("trusted");
        assert!(s.simulators.iter().all(|sim| sim.trust == TrustOs::Trusted));
        let mut failing = patched(
            r#"{"simulators":[{"name":"iPhone 17 Pro","trust":"absent"}],"trust_simulators":{"fails":{"stderr":"simctl: error"}}}"#,
        );
        assert!(failing.trust_simulators().is_err());
        assert_eq!(failing.simulators[0].trust, TrustOs::Absent);
        assert!(Scenario::default().trust_simulators().is_err());
    }

    #[test]
    fn the_running_simulators_are_scripted_by_name_through_the_shells_own_trust_words() {
        let s = patched(
            r#"{"simulators":[{"name":"iPhone 17 Pro","trust":"trusted"},{"name":"iPad mini","trust":"unreadable"}]}"#,
        );
        assert_eq!(
            s.simulators_reading(),
            [
                SimulatorTrust::new("iPhone 17 Pro".into(), Some(Trust::Trusted)),
                SimulatorTrust::new("iPad mini".into(), None),
            ]
        );
    }

    #[test]
    fn the_running_emulators_are_scripted_by_name_through_the_shells_own_words() {
        let s = patched(
            r#"{"emulators":[{"name":"Pixel 7 API 35","trust":"trusted","proxy":"wifi-only"},{"name":"Pixel Fold","trust":"unreadable","proxy":"unreadable"}]}"#,
        );
        assert_eq!(
            s.emulators_reading(),
            [
                EmulatorReading::new(
                    "Pixel 7 API 35".into(),
                    Some(Trust::Trusted),
                    &EmulatorProxy::WifiOnly
                ),
                EmulatorReading::new("Pixel Fold".into(), None, &EmulatorProxy::Unknown),
            ]
        );
    }

    #[test]
    fn the_emulator_read_carries_the_scripted_sdk_and_no_adb_reads_no_emulator() {
        let s = patched(
            r#"{"android":{"adb":true,"sdk":"$HOME/Library/Android/sdk","avd":{"id":"Pixel_7_API_35","name":"Pixel 7 API 35"},"starting":true},"emulators":[{"name":"Pixel 7 API 35","trust":"absent","proxy":"none"}]}"#,
        );
        let read = s.emulator_read();
        assert!(read.adb);
        assert!(read.starting);
        assert_eq!(read.sdk.as_deref(), Some("$HOME/Library/Android/sdk"));
        assert_eq!(
            read.avd,
            Some(Avd {
                id: "Pixel_7_API_35".into(),
                name: "Pixel 7 API 35".into(),
                play_store: false
            })
        );
        assert_eq!(read.emulators.len(), 1);
        let play = patched(
            r#"{"android":{"adb":true,"sdk":null,"avd":{"id":"Pixel_8","name":"Pixel 8","playStore":true}}}"#,
        );
        assert!(play.emulator_read().avd.is_some_and(|avd| avd.play_store));
        assert!(!play.emulator_read().starting);
        let no_adb = patched(
            r#"{"android":{"adb":false,"sdk":null,"avd":null},"emulators":[{"name":"Pixel 7 API 35","trust":"absent","proxy":"none"}]}"#,
        );
        assert!(!no_adb.emulator_read().adb);
        assert!(no_adb.emulator_read().emulators.is_empty());
        assert!(Scenario::default().emulator_read().adb);
    }

    #[test]
    fn installing_trusts_every_running_emulator_and_an_image_with_no_su_refuses() {
        let mut s = patched(
            r#"{"emulators":[{"name":"Pixel 7 API 35","trust":"absent","proxy":"none"},{"name":"Pixel Fold","trust":"absent","proxy":"none"}]}"#,
        );
        s.install_emulators().expect("installed");
        assert!(s.emulators.iter().all(|e| e.trust == TrustOs::Trusted));
        let mut no_su = patched(
            r#"{"emulators":[{"name":"Pixel 7 API 35","trust":"unreadable","proxy":"none"}]}"#,
        );
        assert!(no_su.install_emulators().is_err());
        assert_eq!(no_su.emulators[0].trust, TrustOs::Unreadable);
        let mut failing = patched(
            r#"{"emulators":[{"name":"Pixel 7 API 35","trust":"absent","proxy":"none"}],"install_emulators":{"fails":{"stderr":"adb: device offline"}}}"#,
        );
        assert_eq!(
            failing.install_emulators(),
            Err(command_failed("adb: device offline", NO_EMULATOR_INSTALL_DETAIL))
        );
        assert_eq!(Scenario::default().install_emulators(), Err(setup_error(NO_EMULATOR_RUNNING)));
    }

    #[test]
    fn routing_and_unrouting_move_every_running_emulators_proxy_and_fail_as_scripted() {
        let mut s =
            patched(r#"{"emulators":[{"name":"Pixel 7 API 35","trust":"absent","proxy":"none"}]}"#);
        s.route_emulators(EmulatorProxyOs::Ours, &Outcome::Ok, NO_EMULATOR_ROUTE_DETAIL).unwrap();
        assert_eq!(s.emulators[0].proxy, EmulatorProxyOs::Ours);
        let refused = Outcome::Fails { stderr: "adb: device offline".into() };
        assert!(s
            .route_emulators(EmulatorProxyOs::None, &refused, NO_EMULATOR_UNROUTE_DETAIL)
            .is_err());
        assert_eq!(s.emulators[0].proxy, EmulatorProxyOs::Ours);
        assert!(Scenario::default()
            .route_emulators(EmulatorProxyOs::Ours, &Outcome::Ok, NO_EMULATOR_ROUTE_DETAIL)
            .is_err());
    }

    #[test]
    fn a_patch_moves_only_the_fields_it_names() {
        let mut s = patched(r#"{"trust":"trusted","allow_reach":true}"#);
        s.apply(
            serde_json::from_str(r#"{"proxy":{"set":{"host":"10.0.0.9","port":3128}}}"#).unwrap(),
        );
        assert_eq!(s.trust_reading(), "trusted");
        assert!(s.allow_reach);
        assert_eq!(s.proxy, ProxyOs::Set { host: "10.0.0.9".into(), port: 3128 });
    }

    #[test]
    fn a_patch_naming_an_unknown_field_is_refused() {
        assert!(serde_json::from_str::<Patch>(r#"{"trusted":true}"#).is_err());
    }

    #[test]
    fn each_trust_reading_is_worded_by_the_shells_own_trust_word() {
        for (word, reading) in [
            ("trusted", Some(Trust::Trusted)),
            ("installed", Some(Trust::Installed)),
            ("absent", Some(Trust::Absent)),
            ("unreadable", None),
        ] {
            let s = patched(&format!(r#"{{"trust":"{word}"}}"#));
            assert_eq!(s.trust_reading(), trust_word(reading), "{word}");
        }
    }

    #[test]
    fn each_proxy_reading_goes_through_the_shells_classifier() {
        let ours = patched(r#"{"proxy":{"set":{"host":"127.0.0.1","port":7420}}}"#);
        assert_eq!(
            ours.proxy_reading("127.0.0.1", 7420),
            read_proxy(&ProxyRead::Set("127.0.0.1".into(), 7420), "127.0.0.1", 7420)
        );
        let foreign = patched(r#"{"proxy":{"set":{"host":"10.0.0.9","port":3128}}}"#);
        assert_eq!(
            foreign.proxy_reading("127.0.0.1", 7420),
            read_proxy(&ProxyRead::Set("10.0.0.9".into(), 3128), "127.0.0.1", 7420)
        );
        let unreadable = patched(r#"{"proxy":"unreadable"}"#);
        assert_eq!(
            unreadable.proxy_reading("127.0.0.1", 7420),
            read_proxy(&ProxyRead::Unreadable, "127.0.0.1", 7420)
        );
        let lan = patched(r#"{"proxy":{"set":{"host":"192.168.1.24","port":7420}}}"#);
        let own = ["192.168.1.24".to_string()];
        assert_eq!(
            lan.quit_reading("127.0.0.1", 7420, &own),
            read_proxy_among(&ProxyRead::Set("192.168.1.24".into(), 7420), "127.0.0.1", 7420, &own)
        );
        let unset = patched(r#"{"proxy":"unset"}"#);
        assert_eq!(
            unset.proxy_reading("127.0.0.1", 7420),
            read_proxy(&ProxyRead::Off, "127.0.0.1", 7420)
        );
    }

    #[test]
    fn a_set_up_that_succeeds_trusts_the_certificate_and_routes_through_the_address_asked_for() {
        let mut s = Scenario::default();
        assert_eq!(s.set_up("127.0.0.1".into(), 7420), Ok(()));
        assert_eq!(s.trust_reading(), "trusted");
        assert_eq!(s.proxy, ProxyOs::Set { host: "127.0.0.1".into(), port: 7420 });
    }

    #[test]
    fn a_cancelled_prompt_reads_as_refused_through_the_shells_stderr_reading() {
        let mut s = patched(r#"{"set_up":{"fails":{"stderr":"User cancelled. (-128)"}}}"#);
        assert_eq!(
            s.set_up("127.0.0.1".into(), 7420),
            Err(command_failed("User cancelled. (-128)", NO_INSTALL_DETAIL))
        );
        assert_eq!(s.trust_reading(), "absent", "a refused set-up changed nothing");
    }

    #[test]
    fn a_removed_certificate_reads_absent_and_a_refused_removal_leaves_it_trusted() {
        let mut s = patched(
            r#"{"trust":"trusted","remove_ca":{"fails":{"stderr":"User cancelled. (-128)"}}}"#,
        );
        assert_eq!(s.remove_ca(), Err(command_failed("User cancelled. (-128)", NO_REMOVE_DETAIL)));
        assert_eq!(s.trust_reading(), "trusted");
        s.apply(serde_json::from_str(r#"{"remove_ca":"ok"}"#).unwrap());
        assert_eq!(s.remove_ca(), Ok(()));
        assert_eq!(s.trust_reading(), "absent");
    }

    #[test]
    fn a_failed_tool_keeps_its_own_words_and_changes_nothing() {
        let mut s =
            patched(r#"{"set_proxy":{"fails":{"stderr":"networksetup: no such service"}}}"#);
        assert_eq!(
            s.set_proxy("127.0.0.1".into(), 7420),
            Err(command_failed("networksetup: no such service", NO_SET_PROXY_DETAIL))
        );
        assert_eq!(s.proxy, ProxyOs::Unset);
    }

    #[test]
    fn a_silent_failure_takes_the_real_commands_no_detail_sentence() {
        let mut s = patched(
            r#"{"proxy":{"set":{"host":"127.0.0.1","port":7420}},"unset_proxy":{"fails":{"stderr":""}}}"#,
        );
        assert_eq!(s.unset_proxy(), Err(command_failed("", NO_UNSET_PROXY_DETAIL)));
        assert_ne!(s.proxy, ProxyOs::Unset);
    }

    #[test]
    fn an_unset_that_succeeds_turns_the_proxy_off() {
        let mut s = patched(r#"{"proxy":{"set":{"host":"127.0.0.1","port":7420}}}"#);
        assert_eq!(s.unset_proxy(), Ok(()));
        assert_eq!(s.proxy, ProxyOs::Unset);
    }

    #[test]
    fn each_host_becomes_a_user_agent_the_frontend_reads_as_that_host() {
        for (host, marker) in [("macos", "Macintosh"), ("windows", "Windows"), ("linux", "Linux")] {
            let script = host_script(Some(host)).unwrap().unwrap();
            assert!(script.contains(marker), "{host}: {script}");
            assert!(script.starts_with("Object.defineProperty(navigator, \"userAgent\""));
        }
        assert!(!host_script(Some("linux")).unwrap().unwrap().contains("Windows"));
    }

    #[test]
    fn a_launch_opens_on_the_readings_its_environment_names() {
        let s =
            opening(Some(r#"{"trust":"trusted","interface_names":{"en0":"Ethernet"}}"#)).unwrap();
        assert_eq!(s.trust_reading(), "trusted");
        assert_eq!(s.interface_names.get("en0").map(String::as_str), Some("Ethernet"));
        assert_eq!(opening(None), Ok(Scenario::default()));
        assert!(opening(Some("{\"trusted\":1}")).unwrap_err().starts_with(SCRIPT_VAR));
    }

    #[test]
    fn no_host_keeps_the_real_user_agent_and_an_unknown_one_is_refused() {
        assert_eq!(host_script(None), Ok(None));
        assert_eq!(host_script(Some("  ")), Ok(None));
        assert!(host_script(Some("beos")).unwrap_err().contains("beos"));
    }
}
