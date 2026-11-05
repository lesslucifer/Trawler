use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    pub fn named(name: &str) -> Result<Self, String> {
        match name {
            "macos" => Ok(Self::MacOs),
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            other => {
                Err(format!("cargo xtask package builds on macOS, Windows or Linux, not {other}"))
            }
        }
    }

    fn bundles(self) -> &'static str {
        match self {
            Self::MacOs => "app,dmg",
            Self::Windows => "msi,nsis",
            Self::Linux => "deb,rpm,appimage",
        }
    }

    fn pnpm(self) -> &'static str {
        match self {
            Self::Windows => "pnpm.cmd",
            Self::MacOs | Self::Linux => "pnpm",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub universal: bool,
    pub profile: Option<String>,
}

pub fn parse(args: &[&str]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match *arg {
            "--universal" if !options.universal => options.universal = true,
            "--profile" if options.profile.is_none() => match rest.next() {
                Some(name) if !name.starts_with('-') => options.profile = Some((*name).to_string()),
                _ => return Err(super::USAGE.to_string()),
            },
            _ => return Err(super::USAGE.to_string()),
        }
    }
    Ok(options)
}

pub const UNIVERSAL: &str = "universal-apple-darwin";
pub const UNIVERSAL_PARTS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
const RELEASE: &str = "release";

#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: &'static str,
    pub args: Vec<String>,
    pub bundle_dir: PathBuf,
}

pub fn plan(os: Os, options: &Options, installed: &[&str]) -> Result<Plan, String> {
    let mut args: Vec<String> =
        ["tauri", "build", "--bundles", os.bundles()].map(String::from).into();
    let mut bundle_dir = PathBuf::new();
    if options.universal {
        if os != Os::MacOs {
            return Err("--universal builds a macOS universal app; build on macOS".to_string());
        }
        let missing: Vec<&str> =
            UNIVERSAL_PARTS.into_iter().filter(|part| !installed.contains(part)).collect();
        if !missing.is_empty() {
            return Err(format!(
                "--universal compiles for both Mac architectures; install the missing ones first:\n  rustup target add {}",
                missing.join(" ")
            ));
        }
        args.extend(["--target".to_string(), UNIVERSAL.to_string()]);
        bundle_dir.push(UNIVERSAL);
    }
    if let Some(profile) = &options.profile {
        args.extend(["--".to_string(), "--profile".to_string(), profile.clone()]);
    }
    bundle_dir.push(options.profile.as_deref().unwrap_or(RELEASE));
    bundle_dir.push("bundle");
    Ok(Plan { program: os.pnpm(), args, bundle_dir })
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let options = parse(args)?;
    let os = Os::named(std::env::consts::OS)?;
    let installed = if options.universal { installed_targets()? } else { String::new() };
    let plan = plan(os, &options, &installed.lines().collect::<Vec<_>>())?;
    let ui = repo.join("ui");
    pnpm(&ui, os.pnpm(), &["install".to_string(), "--frozen-lockfile".to_string()])?;
    pnpm(&ui, plan.program, &plan.args)?;
    let bundle_dir = repo.join("target").join(&plan.bundle_dir);
    let artifacts = artifacts(&bundle_dir)?;
    if os == Os::MacOs {
        for app in artifacts.iter().filter(|path| path.extension().is_some_and(|e| e == "app")) {
            verify_signature(app)?;
        }
    }
    let listed: Vec<String> =
        artifacts.iter().map(|path| format!("  {}", path.display())).collect();
    Ok((0, format!("package complete:\n{}", listed.join("\n"))))
}

fn installed_targets() -> Result<String, String> {
    let out = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|e| format!("failed to launch rustup (is it installed?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "rustup target list --installed failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn pnpm(ui: &Path, program: &str, args: &[String]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(ui)
        .env_remove("VITE_E2E")
        .status()
        .map_err(|e| format!("failed to launch {program} (is it installed?): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed", args.join(" ")))
    }
}

/// Each `bundle/<kind>/<artifact>` the build left, the `.app` directory included.
pub fn artifacts(bundle_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let kinds = std::fs::read_dir(bundle_dir)
        .map_err(|e| format!("the build left no {}: {e}", bundle_dir.display()))?;
    let mut found = Vec::new();
    for kind in kinds.filter_map(Result::ok).filter(|entry| entry.path().is_dir()) {
        for entry in std::fs::read_dir(kind.path()).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            if is_artifact(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn is_artifact(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    [".app", ".dmg", ".msi", "-setup.exe", ".deb", ".rpm", ".AppImage"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

fn verify_signature(app: &Path) -> Result<(), String> {
    let out = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .map_err(|e| format!("failed to launch codesign: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} fails codesign --verify --deep --strict:\n{}",
            app.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    for (at, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..at],
            _ => {}
        }
    }
    line
}

/// The value of `key` inside `[section]` of a TOML file, quotes kept and
/// comments dropped; an array runs to its closing bracket, across lines.
fn toml_value(toml: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    let mut lines = toml.lines().map(without_comment).map(str::trim);
    while let Some(line) = lines.next() {
        if line.starts_with('[') {
            inside = line == section;
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| inside) else { continue };
        if k.trim() != key {
            continue;
        }
        let mut value = v.trim().to_string();
        if value.starts_with('[') {
            while !value.contains(']') {
                value.push('\n');
                value.push_str(lines.next()?);
            }
        }
        return Some(value);
    }
    None
}

fn unquoted(value: &str) -> Option<&str> {
    value.strip_prefix('"')?.strip_suffix('"')
}

pub fn workspace_version(root_manifest: &str) -> Option<String> {
    unquoted(&toml_value(root_manifest, "[workspace.package]", "version")?).map(str::to_string)
}

fn workspace_members(root_manifest: &str) -> Result<Vec<String>, String> {
    let mut members = vec![".".to_string()];
    let Some(list) = toml_value(root_manifest, "[workspace]", "members") else {
        return Ok(members);
    };
    for entry in list.trim_matches(['[', ']']).split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let member = unquoted(entry).ok_or_else(|| {
            format!("Cargo.toml: [workspace] members holds {entry}, not a quoted path")
        })?;
        members.push(member.to_string());
    }
    Ok(members)
}

fn inherits_version(manifest: &str) -> bool {
    toml_value(manifest, "[package]", "version.workspace").as_deref() == Some("true")
        || toml_value(manifest, "[package]", "version")
            .is_some_and(|value| value.replace(' ', "") == "{workspace=true}")
}

/// Every place the version could drift from `[workspace.package] version`,
/// one line per file that disagrees.
pub fn version_drift(repo: &Path) -> Result<Vec<String>, String> {
    let read = |path: &str| {
        std::fs::read_to_string(repo.join(path)).map_err(|e| format!("read {path}: {e}"))
    };
    let root = read("Cargo.toml")?;
    let Some(version) = workspace_version(&root) else {
        return Ok(vec!["Cargo.toml: no [workspace.package] version".to_string()]);
    };
    let members = match workspace_members(&root) {
        Ok(members) => members,
        Err(unreadable) => return Ok(vec![unreadable]),
    };
    let mut drift = Vec::new();
    for member in members {
        let manifest = Path::new(&member).join("Cargo.toml");
        let manifest = manifest.to_string_lossy();
        if !inherits_version(&read(&manifest)?) {
            drift.push(format!("{manifest}: [package] must say version.workspace = true"));
        }
    }
    let json = |path: &str| -> Result<serde_json::Value, String> {
        serde_json::from_str(&read(path)?).map_err(|e| format!("{path}: {e}"))
    };
    if json("ui/src-tauri/tauri.conf.json")?.get("version").is_some() {
        drift.push(
            "ui/src-tauri/tauri.conf.json: carries a version; Tauri takes the crate's".to_string(),
        );
    }
    let ui_version = json("ui/package.json")?["version"].as_str().map(str::to_string);
    if ui_version != Some(version.clone()) {
        drift.push(format!("ui/package.json: version {ui_version:?}, the workspace is {version}"));
    }
    Ok(drift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(universal: bool, profile: Option<&str>) -> Options {
        Options { universal, profile: profile.map(str::to_string) }
    }

    fn argv(plan: &Plan) -> String {
        format!("{} {}", plan.program, plan.args.join(" "))
    }

    #[test]
    fn each_host_builds_its_own_installers() {
        let built = |os| plan(os, &Options::default(), &[]).expect("a plan");
        assert_eq!(argv(&built(Os::MacOs)), "pnpm tauri build --bundles app,dmg");
        assert_eq!(argv(&built(Os::Windows)), "pnpm.cmd tauri build --bundles msi,nsis");
        assert_eq!(argv(&built(Os::Linux)), "pnpm tauri build --bundles deb,rpm,appimage");
        assert_eq!(built(Os::Linux).bundle_dir, Path::new("release/bundle"));
    }

    #[test]
    fn a_profile_goes_to_cargo_and_names_the_bundle_dir() {
        let built = plan(Os::MacOs, &options(false, Some("package-test")), &[]).expect("a plan");
        assert_eq!(argv(&built), "pnpm tauri build --bundles app,dmg -- --profile package-test");
        assert_eq!(built.bundle_dir, Path::new("package-test/bundle"));
    }

    #[test]
    fn universal_targets_both_mac_architectures() {
        let built = plan(Os::MacOs, &options(true, None), &UNIVERSAL_PARTS).expect("a plan");
        assert_eq!(
            argv(&built),
            "pnpm tauri build --bundles app,dmg --target universal-apple-darwin"
        );
        assert_eq!(built.bundle_dir, Path::new("universal-apple-darwin/release/bundle"));
    }

    #[test]
    fn universal_without_a_mac_target_names_the_rustup_line() {
        let refused = plan(Os::MacOs, &options(true, None), &["aarch64-apple-darwin"]);
        assert!(
            refused.unwrap_err().ends_with("rustup target add x86_64-apple-darwin"),
            "the refusal names only the missing target"
        );
        let refused = plan(Os::MacOs, &options(true, None), &[]).unwrap_err();
        assert!(refused.ends_with("rustup target add aarch64-apple-darwin x86_64-apple-darwin"));
    }

    #[test]
    fn universal_off_macos_is_refused() {
        for os in [Os::Windows, Os::Linux] {
            let refused = plan(os, &options(true, None), &UNIVERSAL_PARTS).unwrap_err();
            assert!(refused.contains("macOS"), "{refused}");
        }
    }

    #[test]
    fn the_flags_parse_and_anything_else_is_usage() {
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert_eq!(
            parse(&["--profile", "package-test", "--universal"]).unwrap(),
            options(true, Some("package-test"))
        );
        for bad in [
            &["--sign"][..],
            &["--profile"],
            &["--profile", "--universal"],
            &["--universal", "--universal"],
        ] {
            assert!(parse(bad).unwrap_err().contains("cargo xtask package"), "{bad:?}");
        }
    }

    #[test]
    fn an_unknown_host_is_refused_by_name() {
        assert_eq!(Os::named("macos"), Ok(Os::MacOs));
        assert!(Os::named("freebsd").unwrap_err().contains("not freebsd"));
    }

    #[test]
    fn artifacts_are_read_one_level_under_each_bundle_kind() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path();
        for path in ["macos/Trawler.app/Contents", "dmg/Trawler_0.1.0_aarch64.dmg", "nsis/x"] {
            std::fs::create_dir_all(bundle.join(path)).unwrap();
        }
        std::fs::write(bundle.join("dmg/bundle_dmg.sh"), "").unwrap();
        std::fs::write(bundle.join("nsis/Trawler_0.1.0_x64-setup.exe"), "").unwrap();
        let names: Vec<String> = artifacts(bundle)
            .unwrap()
            .iter()
            .map(|path| path.strip_prefix(bundle).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "dmg/Trawler_0.1.0_aarch64.dmg",
                "macos/Trawler.app",
                "nsis/Trawler_0.1.0_x64-setup.exe"
            ]
        );
    }

    #[test]
    fn a_missing_bundle_dir_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("release/bundle");
        assert!(artifacts(&missing).unwrap_err().contains("release/bundle"));
    }

    const ROOT: &str = "[workspace]\nmembers = [\"xtask\", \"ui/src-tauri\"]\n\n\
        [workspace.package]\nversion = \"1.2.3-rc.1\"\n\n[package]\nname = \"trawler\"\nversion.workspace = true\n";

    #[test]
    fn the_workspace_version_is_read_from_its_own_section() {
        assert_eq!(workspace_version(ROOT).as_deref(), Some("1.2.3-rc.1"));
        assert_eq!(workspace_version("[package]\nversion = \"9.9.9\"\n"), None);
    }

    #[test]
    fn the_members_are_the_root_and_the_listed_crates() {
        assert_eq!(workspace_members(ROOT).unwrap(), [".", "xtask", "ui/src-tauri"]);
    }

    const MULTILINE_ROOT: &str = "[workspace]\nmembers = [\n    \"xtask\",\n    \"ui/src-tauri\",\n]\n\n\
        [workspace.package]\nversion = \"1.2.3-rc.1\"\n\n[package]\nname = \"trawler\"\nversion.workspace = true\n";

    #[test]
    fn members_listed_over_several_lines_are_all_read() {
        assert_eq!(workspace_members(MULTILINE_ROOT).unwrap(), [".", "xtask", "ui/src-tauri"]);
    }

    const COMMENTED_ROOT: &str = "[workspace] # the crates\nmembers = [\n    \"xtask\", # tooling\n    # \"retired\", ] gone\n    \"ui/src-tauri\",\n]\n\n\
        [workspace.package]\nversion = \"1.2.3-rc.1\" # bumped by a release commit\n";

    #[test]
    fn comments_neither_drop_a_member_nor_end_the_list() {
        assert_eq!(workspace_members(COMMENTED_ROOT).unwrap(), [".", "xtask", "ui/src-tauri"]);
        assert_eq!(workspace_version(COMMENTED_ROOT).as_deref(), Some("1.2.3-rc.1"));
    }

    #[test]
    fn a_member_entry_that_is_not_a_quoted_path_is_named_not_skipped() {
        let root = "[workspace]\nmembers = [\"xtask\", crates/*]\n";
        assert!(workspace_members(root).unwrap_err().contains("crates/*"));
    }

    #[test]
    fn both_spellings_of_inheritance_inherit() {
        assert!(inherits_version("[package]\nversion.workspace = true # one source\n"));
        assert!(inherits_version("[package]\nversion = { workspace = true }\n"));
        assert!(!inherits_version("[package]\nversion = \"0.1.0\"\n"));
        assert!(!inherits_version("[dependencies]\nversion.workspace = true\n"));
    }

    #[test]
    fn a_member_typing_its_version_is_drift_however_the_members_are_laid_out() {
        let typed = "[package]\nname = \"m\"\nversion = \"1.2.3-rc.1\"\n";
        let dir = tree(MULTILINE_ROOT, typed, "{}", r#"{"version":"1.2.3-rc.1"}"#);
        let drift = version_drift(dir.path()).unwrap().join("\n");
        assert!(drift.contains("xtask/Cargo.toml"), "{drift}");
        assert!(drift.contains("ui/src-tauri/Cargo.toml"), "{drift}");
    }

    fn tree(root: &str, member: &str, tauri_conf: &str, package_json: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("Cargo.toml", root);
        write("xtask/Cargo.toml", member);
        write("ui/src-tauri/Cargo.toml", member);
        write("ui/src-tauri/tauri.conf.json", tauri_conf);
        write("ui/package.json", package_json);
        dir
    }

    const INHERITS: &str = "[package]\nname = \"m\"\nversion.workspace = true\n";

    #[test]
    fn a_tree_with_one_version_has_no_drift() {
        let dir = tree(ROOT, INHERITS, "{}", r#"{"version":"1.2.3-rc.1"}"#);
        assert_eq!(version_drift(dir.path()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn each_drift_names_its_file() {
        let typed = "[package]\nname = \"m\"\nversion = \"1.2.3-rc.1\"\n";
        let dir = tree(ROOT, typed, r#"{"version":"1.2.3-rc.1"}"#, r#"{"version":"1.2.3"}"#);
        let drift = version_drift(dir.path()).unwrap().join("\n");
        for file in [
            "xtask/Cargo.toml",
            "ui/src-tauri/Cargo.toml",
            "ui/src-tauri/tauri.conf.json",
            "ui/package.json",
        ] {
            assert!(drift.contains(file), "{file} missing from:\n{drift}");
        }
    }

    #[test]
    fn a_root_without_a_workspace_version_is_drift() {
        let dir = tree("[workspace]\nmembers = []\n", INHERITS, "{}", r#"{"version":"1"}"#);
        assert_eq!(
            version_drift(dir.path()).unwrap(),
            ["Cargo.toml: no [workspace.package] version"]
        );
    }

    #[test]
    fn the_tree_carries_one_version() {
        let repo = crate::repo_root().expect("repo root");
        assert_eq!(version_drift(&repo).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn package_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask package [--universal] [--profile <name>]"));
    }
}
