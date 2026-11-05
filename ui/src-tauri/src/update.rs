use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::utils::config::BundleType;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const LAST_CHECK_FILE: &str = "last-update-check";
const INSTALL: &str = "Install";
const LATER: &str = "Later";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    AtLaunch,
    ByUser,
}

/// Whether a launch should ask the channel: never asked, asked at least
/// `CHECK_EVERY` ago, or a record from the future (a clock set back).
pub fn due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.and_then(|at| now.duration_since(at).ok()).map_or(true, |since| since >= CHECK_EVERY)
}

pub fn last_check(record: &Path) -> Option<SystemTime> {
    let secs: u64 = std::fs::read_to_string(record).ok()?.trim().parse().ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

pub fn record_check(record: &Path, at: SystemTime) -> std::io::Result<()> {
    if let Some(dir) = record.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let secs = at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    std::fs::write(record, secs.to_string())
}

/// Only an app installed from one of its bundles can replace itself; a binary
/// run from `target/` has no bundle to swap.
pub fn installed(bundle: Option<BundleType>) -> bool {
    bundle.is_some()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Found {
    Newer { current: String, next: String },
    UpToDate { current: String },
    Failed(String),
    NotInstalled,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Offer,
    Info,
    Error,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: &'static str,
    pub text: String,
    pub kind: NoticeKind,
}

/// What the user is told about a check. A launch check speaks only to offer
/// an update; a check the user asked for always answers.
pub fn notice(asked: Asked, found: &Found) -> Option<Notice> {
    let notice = match found {
        Found::Newer { current, next } => Notice {
            title: "Update available",
            text: format!(
                "Trawler {next} is available. You have {current}.\n\n\
                 Installing quits Trawler, which clears the captured list, then opens {next}. \
                 Your certificate and settings stay."
            ),
            kind: NoticeKind::Offer,
        },
        _ if asked == Asked::AtLaunch => return None,
        Found::UpToDate { current } => Notice {
            title: "No update",
            text: format!("Trawler {current} is up to date."),
            kind: NoticeKind::Info,
        },
        Found::Failed(why) => Notice {
            title: "Couldn't check for updates",
            text: format!("Trawler couldn't check for updates: {why}"),
            kind: NoticeKind::Error,
        },
        Found::NotInstalled => Notice {
            title: "No update",
            text: "Updates come to an installed Trawler. This one runs from a build folder."
                .to_string(),
            kind: NoticeKind::Info,
        },
    };
    Some(notice)
}

pub fn not_installed(current: &str, why: &str) -> Notice {
    Notice {
        title: "Update not installed",
        text: format!("The update was not installed: {why}\n\nTrawler {current} is unchanged."),
        kind: NoticeKind::Error,
    }
}

/// The update the user said Install to, verified and waiting for the quit it
/// asked for. A cancelled quit drops it, so nothing installs on a later quit.
pub type Staged = (Update, Vec<u8>);

pub struct Pending<T = Staged>(Mutex<Option<T>>);

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

impl<T> Pending<T> {
    pub fn stage(&self, update: T) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(update);
    }

    pub fn take(&self) -> Option<T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

#[tauri::command]
pub fn quit_cancelled(pending: tauri::State<'_, Pending>) {
    pending.take();
}

fn record_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join(LAST_CHECK_FILE))
}

#[allow(clippy::print_stderr)] // the shell has no logger; stderr is its log
pub fn check_at_launch<R: Runtime>(app: &AppHandle<R>) {
    if !installed(tauri::utils::platform::bundle_type()) {
        return;
    }
    let Some(record) = record_path(app) else { return };
    let now = SystemTime::now();
    if !due(last_check(&record), now) {
        return;
    }
    if let Err(err) = record_check(&record, now) {
        eprintln!("recording the update check failed: {err}");
    }
    check(app, Asked::AtLaunch);
}

pub fn check<R: Runtime>(app: &AppHandle<R>, asked: Asked) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let current = app.package_info().version.to_string();
        let (found, update) = if installed(tauri::utils::platform::bundle_type()) {
            match find(&app).await {
                Ok(Some(update)) => {
                    let next = update.version.clone();
                    (Found::Newer { current, next }, Some(update))
                }
                Ok(None) => (Found::UpToDate { current }, None),
                Err(why) => (Found::Failed(why), None),
            }
        } else {
            (Found::NotInstalled, None)
        };
        if let Some(notice) = notice(asked, &found) {
            present(&app, notice, update);
        }
    });
}

async fn find<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Update>, String> {
    app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

fn present<R: Runtime>(app: &AppHandle<R>, notice: Notice, update: Option<Update>) {
    let dialog = app.dialog().message(notice.text).title(notice.title);
    match (notice.kind, update) {
        (NoticeKind::Offer, Some(update)) => {
            let app = app.clone();
            dialog
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancelCustom(INSTALL.into(), LATER.into()))
                .show(move |install| {
                    if install {
                        tauri::async_runtime::spawn(install_on_quit(app, update));
                    }
                });
        }
        (NoticeKind::Error, _) => dialog.kind(MessageDialogKind::Error).show(|_| {}),
        _ => dialog.kind(MessageDialogKind::Info).show(|_| {}),
    }
}

/// Downloading verifies the signature against the configured public key, so
/// only a verified update is ever staged. The quit is ⌘Q's request, so a
/// windowless app opens its window to run DEC-64's check too.
async fn install_on_quit<R: Runtime>(app: AppHandle<R>, update: Update) {
    match update.download(|_, _| {}, || {}).await {
        Ok(bytes) => drop(stage_and_quit(&app, (update, bytes))),
        Err(err) => {
            let current = app.package_info().version.to_string();
            present(&app, not_installed(&current, &err.to_string()), None);
        }
    }
}

pub fn stage_and_quit<R: Runtime>(app: &AppHandle<R>, staged: Staged) -> JoinHandle<()> {
    app.state::<Pending>().stage(staged);
    crate::lifecycle::request_quit(app)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
    Relaunch,
    TellThenExit(Notice),
}

/// How a quit that carried an update ends: as the new version, or, when the
/// install failed, with the user told why before the app goes.
pub fn ending(current: &str, installed: Result<(), String>) -> Ending {
    match installed {
        Ok(()) => Ending::Relaunch,
        Err(why) => Ending::TellThenExit(not_installed(current, &why)),
    }
}

/// Set while a failed install is being told: the last window is already gone,
/// and the app must not end under the dialog that says why.
#[derive(Default)]
pub struct Telling(AtomicBool);

/// An exit the user's last window asked for waits while a failed install is
/// told; an exit the app asked for itself (the dialog's close) goes through.
pub fn holds_exit(asked_by_app: bool, telling: bool) -> bool {
    telling && !asked_by_app
}

pub fn telling<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<Telling>().is_some_and(|t| t.0.load(Ordering::SeqCst))
}

/// The app ends, by its last window or by a quit. With an update staged, the
/// core stops, the update installs and the app opens again as the new version.
pub fn end<R: Runtime>(app: &AppHandle<R>) {
    let Some((update, bytes)) = app.state::<Pending>().take() else {
        app.exit(0);
        return;
    };
    if let Some(core) = app.try_state::<crate::core::Core>() {
        core.stop();
    }
    let current = app.package_info().version.to_string();
    match ending(&current, update.install(bytes).map_err(|e| e.to_string())) {
        Ending::Relaunch => app.restart(),
        Ending::TellThenExit(notice) => {
            if let Some(flag) = app.try_state::<Telling>() {
                flag.0.store(true, Ordering::SeqCst);
            }
            let ended = app.clone();
            app.dialog()
                .message(notice.text)
                .title(notice.title)
                .kind(MessageDialogKind::Error)
                .show(move |_| ended.exit(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    const DAY: u64 = 24 * 60 * 60;

    #[test]
    fn a_launch_asks_the_channel_at_most_once_a_day() {
        assert!(due(None, at(10)));
        assert!(!due(Some(at(1_000)), at(1_000 + DAY - 1)));
        assert!(due(Some(at(1_000)), at(1_000 + DAY)));
    }

    #[test]
    fn a_record_from_the_future_does_not_silence_the_check() {
        assert!(due(Some(at(5_000)), at(1_000)));
    }

    #[test]
    fn the_record_round_trips_and_an_unreadable_one_reads_as_never() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("nested/last-update-check");
        assert_eq!(last_check(&record), None);
        record_check(&record, at(1_234)).unwrap();
        assert_eq!(last_check(&record), Some(at(1_234)));
        std::fs::write(&record, "yesterday").unwrap();
        assert_eq!(last_check(&record), None);
    }

    #[test]
    fn only_a_bundled_app_updates_itself() {
        assert!(!installed(None));
        for bundle in [BundleType::App, BundleType::Nsis, BundleType::Deb, BundleType::AppImage] {
            assert!(installed(Some(bundle)));
        }
    }

    fn newer() -> Found {
        Found::Newer { current: "0.1.0".into(), next: "0.2.0".into() }
    }

    #[test]
    fn an_offer_names_both_versions_the_cleared_list_and_what_stays() {
        let offer = notice(Asked::AtLaunch, &newer()).expect("a launch check offers an update");
        assert_eq!(offer.kind, NoticeKind::Offer);
        for part in
            ["0.2.0 is available", "You have 0.1.0", "clears the captured list", "opens 0.2.0"]
        {
            assert!(offer.text.contains(part), "{part:?} missing from {:?}", offer.text);
        }
        assert!(offer.text.contains("certificate and settings stay"));
        assert_eq!(notice(Asked::ByUser, &newer()), Some(offer));
    }

    #[test]
    fn a_launch_check_says_nothing_unless_it_offers() {
        let quiet = [
            Found::UpToDate { current: "0.1.0".into() },
            Found::Failed("offline".into()),
            Found::NotInstalled,
        ];
        for found in quiet {
            assert_eq!(notice(Asked::AtLaunch, &found), None, "{found:?}");
        }
    }

    #[test]
    fn a_check_the_user_asked_for_always_answers() {
        let up = notice(Asked::ByUser, &Found::UpToDate { current: "0.1.0".into() }).unwrap();
        assert_eq!(up.text, "Trawler 0.1.0 is up to date.");
        let failed = notice(Asked::ByUser, &Found::Failed("the channel is unreachable".into()));
        assert_eq!(
            failed.unwrap().text,
            "Trawler couldn't check for updates: the channel is unreachable"
        );
        let dev = notice(Asked::ByUser, &Found::NotInstalled).unwrap();
        assert!(dev.text.contains("an installed Trawler"), "{}", dev.text);
    }

    #[test]
    fn a_refused_update_says_why_and_that_nothing_changed() {
        let refused = not_installed("0.1.0", "the signature does not match");
        assert_eq!(refused.kind, NoticeKind::Error);
        assert_eq!(
            refused.text,
            "The update was not installed: the signature does not match\n\nTrawler 0.1.0 is unchanged."
        );
    }

    // The channel is the project's public release, read only through the
    // owner's key, and a signature must name the version it was made for.
    #[test]
    fn the_shipped_config_trusts_only_the_owner_key_on_the_public_channel() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid json");
        let updater = &config["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            serde_json::json!([
                "https://github.com/lesslucifer/Trawler/releases/latest/download/latest.json"
            ])
        );
        assert_eq!(updater["requireSignedVersion"], true);
        assert!(updater.get("dangerousInsecureTransportProtocol").is_none());
        let pubkey = updater["pubkey"].as_str().expect("a public key");
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, pubkey)
            .expect("a base64 public key");
        assert!(String::from_utf8(decoded)
            .unwrap()
            .starts_with("untrusted comment: minisign public key: "));
    }

    #[test]
    fn an_installed_update_relaunches_and_a_failed_one_is_told_before_the_app_ends() {
        assert_eq!(ending("0.1.0", Ok(())), Ending::Relaunch);
        assert_eq!(
            ending("0.1.0", Err("the app folder is read-only".into())),
            Ending::TellThenExit(not_installed("0.1.0", "the app folder is read-only"))
        );
    }

    #[test]
    fn the_app_outlives_its_last_window_only_while_a_failed_install_is_told() {
        assert!(holds_exit(false, true), "the last window closing waits for the notice");
        assert!(!holds_exit(true, true), "the notice's own exit goes through");
        assert!(!holds_exit(false, false), "with nothing to tell, a close ends the app");
    }

    #[test]
    fn a_staged_update_is_taken_once_and_a_cancel_drops_it() {
        let pending: Pending<&str> = Pending::default();
        pending.stage("0.2.0");
        assert_eq!(pending.take(), Some("0.2.0"));
        assert_eq!(pending.take(), None, "taken once");
        pending.stage("0.2.0");
        pending.take();
        assert_eq!(pending.take(), None, "a cancelled quit leaves nothing to install later");
    }
}
