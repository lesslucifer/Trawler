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

pub fn is_stale(bundle_built: Option<SystemTime>, inputs_changed: Option<SystemTime>) -> bool {
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

/// Rebuild `ui/dist` when an input is newer than it. Returns why the build
/// cannot go on, or `None` when the bundle is fresh.
pub fn ensure_fresh(ui: &Path) -> Option<String> {
    let inputs: Vec<PathBuf> = BUNDLE_INPUTS.iter().map(|input| ui.join(input)).collect();
    let built = std::fs::metadata(ui.join(BUNDLE_MARKER)).and_then(|m| m.modified()).ok();
    if !is_stale(built, newest_modified(&inputs)) {
        return None;
    }
    if !ui.join("node_modules").is_dir() {
        return Some(missing_dependencies_message(ui));
    }
    match Command::new("pnpm").arg("build").current_dir(ui).status() {
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

    #[test]
    fn the_build_script_checks_the_ui_of_the_crate_it_runs_for_not_the_one_it_was_compiled_in() {
        // Worktrees sharing one target dir reuse a build script compiled in another worktree,
        // so a crate dir baked in at compile time checks that worktree's bundle instead.
        let build_script = include_str!("../build.rs");
        assert!(!build_script.contains("env!(\"CARGO_MANIFEST_DIR\")"), "{build_script}");
        assert!(build_script.contains("var_os(\"CARGO_MANIFEST_DIR\")"), "{build_script}");
    }

    #[test]
    fn no_bundle_is_stale() {
        assert!(is_stale(None, Some(at(1))));
        assert!(is_stale(None, None));
    }

    #[test]
    fn an_input_newer_than_the_bundle_is_stale() {
        assert!(is_stale(Some(at(10)), Some(at(11))));
    }

    #[test]
    fn a_bundle_at_least_as_new_as_every_input_is_fresh() {
        assert!(!is_stale(Some(at(10)), Some(at(10))));
        assert!(!is_stale(Some(at(10)), Some(at(9))));
        assert!(!is_stale(Some(at(10)), None));
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
        touch(&ui.path().join(BUNDLE_MARKER), at(200));
        assert_eq!(ensure_fresh(ui.path()), None);
    }

    #[test]
    fn a_stale_bundle_without_installed_packages_names_pnpm_install() {
        let ui = tempfile::tempdir().unwrap();
        touch(&ui.path().join("src/App.tsx"), at(300));
        touch(&ui.path().join(BUNDLE_MARKER), at(200));
        let problem = ensure_fresh(ui.path()).expect("a stale bundle with nothing to build it");
        assert!(problem.contains("`pnpm install`"), "{problem}");
        assert!(problem.contains(&ui.path().display().to_string()), "{problem}");
        assert!(problem.contains("cargo build -p trawler"), "{problem}");
    }
}
