// Shared by build.rs (via #[path]) and the lib's tests, so it reaches nothing
// but std.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Everything under `ui/` whose change changes the bundle `pnpm build` emits.
pub const BUNDLE_INPUTS: &[&str] = &[
    "src",
    "index.html",
    "package.json",
    "pnpm-lock.yaml",
    "vite.config.ts",
    "tailwind.config.js",
    "postcss.config.js",
    "tsconfig.json",
];

/// The file whose age stands for the whole bundle: Vite writes it last.
pub const BUNDLE_MARKER: &str = "dist/index.html";

/// Where `vite.config.ts` records which flavour the bundle was built as.
pub const BUNDLE_FLAVOUR: &str = "dist/bundle-flavour";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavour {
    Release,
    E2e,
}

impl Flavour {
    pub fn wanted(e2e_feature: bool) -> Self {
        if e2e_feature {
            Self::E2e
        } else {
            Self::Release
        }
    }

    pub fn parse(recorded: &str) -> Option<Self> {
        match recorded.trim() {
            "release" => Some(Self::Release),
            "e2e" => Some(Self::E2e),
            _ => None,
        }
    }

    fn read(ui: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(ui.join(BUNDLE_FLAVOUR)).ok()?)
    }
}

pub fn newest_modified(paths: &[PathBuf]) -> Option<SystemTime> {
    paths.iter().filter_map(|path| newest_under(path)).max()
}

fn newest_under(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_dir() {
        return meta.modified().ok();
    }
    std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| newest_under(&entry.path()))
        .max()
}

pub fn is_stale(
    bundle_built: Option<SystemTime>,
    inputs_changed: Option<SystemTime>,
    built_as: Option<Flavour>,
    wanted: Flavour,
) -> bool {
    if built_as != Some(wanted) {
        return true;
    }
    match (bundle_built, inputs_changed) {
        (None, _) => true,
        (Some(built), Some(changed)) => changed > built,
        (Some(_), None) => false,
    }
}

pub fn missing_dependencies_message(ui: &Path) -> String {
    format!(
        "the app embeds the front-end bundle, and building it needs Node and pnpm with the UI's \
         packages installed: run `pnpm install` in {}, then build again \
         (the headless core alone builds without them: `cargo build -p trawler`)",
        ui.display()
    )
}

pub fn build_command(ui: &Path, flavour: Flavour) -> Command {
    let mut command = Command::new("pnpm");
    command.arg("build").current_dir(ui);
    match flavour {
        Flavour::E2e => command.env("VITE_E2E", "1"),
        Flavour::Release => command.env_remove("VITE_E2E"),
    };
    command
}

/// Rebuild `ui/dist` when an input is newer than it or it was built as another
/// flavour. Returns why the build cannot go on, or `None` when the bundle is fresh.
pub fn ensure_fresh(ui: &Path, wanted: Flavour) -> Option<String> {
    let inputs: Vec<PathBuf> = BUNDLE_INPUTS.iter().map(|input| ui.join(input)).collect();
    let built = std::fs::metadata(ui.join(BUNDLE_MARKER)).and_then(|m| m.modified()).ok();
    if !is_stale(built, newest_modified(&inputs), Flavour::read(ui), wanted) {
        return None;
    }
    if !ui.join("node_modules").is_dir() {
        return Some(missing_dependencies_message(ui));
    }
    match build_command(ui, wanted).status() {
        Ok(status) if status.success() => None,
        Ok(status) => Some(format!("`pnpm build` in {} failed ({status})", ui.display())),
        Err(_) => Some(missing_dependencies_message(ui)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::time::Duration;

    const EPOCH: SystemTime = SystemTime::UNIX_EPOCH;

    fn at(secs: u64) -> SystemTime {
        EPOCH + Duration::from_secs(secs)
    }

    fn touch(path: &Path, modified: SystemTime) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().set_modified(modified).unwrap();
    }

    const RELEASE: Option<Flavour> = Some(Flavour::Release);

    fn bundle(ui: &Path, built: SystemTime, flavour: &str) {
        touch(&ui.join(BUNDLE_MARKER), built);
        fs::write(ui.join(BUNDLE_FLAVOUR), flavour).unwrap();
    }

    #[test]
    fn no_bundle_is_stale() {
        assert!(is_stale(None, Some(at(1)), RELEASE, Flavour::Release));
        assert!(is_stale(None, None, RELEASE, Flavour::Release));
    }

    #[test]
    fn an_input_newer_than_the_bundle_is_stale() {
        assert!(is_stale(Some(at(10)), Some(at(11)), RELEASE, Flavour::Release));
    }

    #[test]
    fn a_bundle_at_least_as_new_as_every_input_is_fresh() {
        assert!(!is_stale(Some(at(10)), Some(at(10)), RELEASE, Flavour::Release));
        assert!(!is_stale(Some(at(10)), Some(at(9)), RELEASE, Flavour::Release));
        assert!(!is_stale(Some(at(10)), None, RELEASE, Flavour::Release));
    }

    #[test]
    fn a_bundle_of_the_other_flavour_is_stale_however_new() {
        assert!(is_stale(Some(at(10)), Some(at(9)), Some(Flavour::E2e), Flavour::Release));
        assert!(is_stale(Some(at(10)), Some(at(9)), RELEASE, Flavour::E2e));
    }

    #[test]
    fn a_bundle_that_records_no_flavour_is_stale() {
        assert!(is_stale(Some(at(10)), Some(at(9)), None, Flavour::Release));
    }

    #[test]
    fn the_e2e_feature_wants_the_e2e_flavour_and_everything_else_release() {
        assert_eq!(Flavour::wanted(true), Flavour::E2e);
        assert_eq!(Flavour::wanted(false), Flavour::Release);
    }

    #[test]
    fn a_recorded_flavour_parses_and_anything_else_is_none() {
        assert_eq!(Flavour::parse("release"), RELEASE);
        assert_eq!(Flavour::parse("e2e\n"), Some(Flavour::E2e));
        assert_eq!(Flavour::parse("debug"), None);
        assert_eq!(Flavour::parse(""), None);
    }

    fn vite_e2e(command: &Command) -> String {
        match command.get_envs().find(|(key, _)| *key == "VITE_E2E") {
            None => "inherited".to_string(),
            Some((_, None)) => "removed".to_string(),
            Some((_, Some(value))) => value.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn the_rebuild_sets_vite_e2e_only_for_the_e2e_flavour() {
        let ui = Path::new("ui");
        assert_eq!(vite_e2e(&build_command(ui, Flavour::E2e)), "1");
        assert_eq!(vite_e2e(&build_command(ui, Flavour::Release)), "removed");
    }

    #[test]
    fn the_newest_input_is_found_at_any_depth() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("index.html"), at(100));
        touch(&ui.path().join("src/components/deep/Row.tsx"), at(300));
        touch(&ui.path().join("src/App.tsx"), at(200));
        let inputs = [ui.path().join("src"), ui.path().join("index.html")];
        assert_eq!(newest_modified(&inputs), Some(at(300)));
    }

    #[test]
    fn a_missing_input_counts_as_unchanged() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("index.html"), at(100));
        let inputs = [ui.path().join("index.html"), ui.path().join("no-such-config.js")];
        assert_eq!(newest_modified(&inputs), Some(at(100)));
    }

    #[test]
    fn a_fresh_bundle_runs_no_build() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("src/App.tsx"), at(100));
        bundle(ui.path(), at(200), "release");
        assert_eq!(ensure_fresh(ui.path(), Flavour::Release), None);
    }

    #[test]
    fn a_new_bundle_of_the_other_flavour_goes_to_the_build() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("src/App.tsx"), at(100));
        bundle(ui.path(), at(200), "e2e");
        let problem = ensure_fresh(ui.path(), Flavour::Release).expect("an e2e bundle is rebuilt");
        assert!(problem.contains("`pnpm install`"), "{problem}");
    }

    #[test]
    fn a_stale_bundle_without_installed_packages_names_pnpm_install() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("src/App.tsx"), at(300));
        bundle(ui.path(), at(200), "release");
        let problem = ensure_fresh(ui.path(), Flavour::Release)
            .expect("a stale bundle with nothing to build it");
        assert!(problem.contains("`pnpm install`"), "{problem}");
        assert!(problem.contains(&ui.path().display().to_string()), "{problem}");
        assert!(problem.contains("cargo build -p trawler"), "{problem}");
    }
}
