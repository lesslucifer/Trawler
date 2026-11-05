use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    BuildAndBundle,
    BuildOnly,
    BundleOnly,
    SignOnly,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub universal: bool,
    pub profile: Option<String>,
    pub stage: Stage,
}

pub fn parse(args: &[&str]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match *arg {
            "--universal" if !options.universal => options.universal = true,
            "--build-only" if options.stage == Stage::BuildAndBundle => {
                options.stage = Stage::BuildOnly;
            }
            "--bundle-only" if options.stage == Stage::BuildAndBundle => {
                options.stage = Stage::BundleOnly;
            }
            "--sign-only" if options.stage == Stage::BuildAndBundle => {
                options.stage = Stage::SignOnly;
            }
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
pub const SIGNING_KEY_VAR: &str = "TAURI_SIGNING_PRIVATE_KEY";

#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: &'static str,
    pub args: Vec<String>,
    pub bundle_dir: PathBuf,
}

pub fn plan(os: Os, options: &Options, installed: &[&str]) -> Result<Plan, String> {
    let mut args: Vec<String> = match options.stage {
        Stage::BuildAndBundle => vec!["tauri", "build", "--bundles", os.bundles()],
        Stage::BuildOnly => vec!["tauri", "build", "--no-bundle"],
        Stage::BundleOnly => vec!["tauri", "bundle", "--bundles", os.bundles()],
        Stage::SignOnly => vec!["tauri", "signer", "sign"],
    }
    .into_iter()
    .map(String::from)
    .collect();
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
        if options.stage != Stage::SignOnly {
            args.extend(["--target".to_string(), UNIVERSAL.to_string()]);
        }
        bundle_dir.push(UNIVERSAL);
    }
    if let Some(profile) = &options.profile {
        if matches!(options.stage, Stage::BundleOnly | Stage::SignOnly) {
            return Err("--bundle-only and --sign-only take the release profile; drop --profile"
                .to_string());
        }
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
    if options.stage == Stage::SignOnly {
        return sign(&plan, &ui, &repo.join("target").join(&plan.bundle_dir), repo);
    }
    if options.stage != Stage::BundleOnly {
        pnpm(&ui, os.pnpm(), &["install".to_string(), "--frozen-lockfile".to_string()])?;
    }
    pnpm(&ui, plan.program, &plan.args)?;
    if options.stage == Stage::BuildOnly {
        return Ok((0, "build complete; cargo xtask package --bundle-only bundles it".to_string()));
    }
    let bundle_dir = repo.join("target").join(&plan.bundle_dir);
    let built = artifacts(&bundle_dir)?;
    if os == Os::MacOs {
        for app in built.iter().filter(|path| path.extension().is_some_and(|e| e == "app")) {
            verify_signature(app)?;
            archive_for_the_updater(app)?;
        }
    }
    let listed: Vec<String> =
        artifacts(&bundle_dir)?.iter().map(|path| format!("  {}", path.display())).collect();
    Ok((
        0,
        format!(
            "package complete (no updater signatures; --sign-only adds them):\n{}",
            listed.join("\n")
        ),
    ))
}

/// The updater archive the bundler would write beside the app, made here so
/// the bundle runs without the signing key.
fn archive_for_the_updater(app: &Path) -> Result<(), String> {
    let parent = app.parent().ok_or(format!("{} has no parent", app.display()))?;
    let name = app.file_name().ok_or(format!("{} has no name", app.display()))?;
    let mut archive = app.as_os_str().to_owned();
    archive.push(".tar.gz");
    let status = Command::new("tar")
        .arg("--no-mac-metadata")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(parent)
        .arg(name)
        .status()
        .map_err(|e| format!("failed to launch tar: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tar of {} failed", app.display()))
    }
}

/// Every file the updater installs from: the installers the release manifest maps.
pub fn updater_targets(artifacts: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut targets = Vec::new();
    for path in artifacts.iter().filter(|path| path.is_file()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if crate::release_manifest::keys_of(name)?.is_some() {
            targets.push(path.clone());
        }
    }
    Ok(targets)
}

fn sign(plan: &Plan, ui: &Path, bundle_dir: &Path, repo: &Path) -> Result<(i32, String), String> {
    if std::env::var_os(SIGNING_KEY_VAR).is_none_or(|key| key.is_empty()) {
        return Err(format!("--sign-only signs with {SIGNING_KEY_VAR}, which is not set"));
    }
    let version = shipped_version(&workspace_members(repo)?)?.to_string();
    let targets = updater_targets(&artifacts(bundle_dir)?)?;
    if targets.is_empty() {
        return Err(format!("{} holds no updater target to sign", bundle_dir.display()));
    }
    for target in &targets {
        let mut args = plan.args.clone();
        args.extend(["--app-version".to_string(), version.clone()]);
        args.push(target.to_string_lossy().into_owned());
        pnpm(ui, plan.program, &args)?;
    }
    let listed: Vec<String> =
        targets.iter().map(|path| format!("  {}.sig", path.display())).collect();
    Ok((0, format!("signed for the updater as {version}:\n{}", listed.join("\n"))))
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
    [".app", ".dmg", ".msi", "-setup.exe", ".deb", ".rpm", ".AppImage", ".app.tar.gz", ".sig"]
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

/// The crate whose version Tauri stamps on every artifact.
pub const SHIPPED: &str = "trawler-ui";

#[derive(Debug, Deserialize)]
struct Members {
    packages: Vec<Member>,
}

#[derive(Debug, Deserialize)]
pub struct Member {
    pub name: String,
    pub version: String,
}

fn workspace_members(repo: &Path) -> Result<Vec<Member>, String> {
    Ok(crate::target::cargo_metadata::<Members>(repo)?.packages)
}

/// The version the app ships, which every other version source must equal.
pub fn shipped_version(members: &[Member]) -> Result<&str, String> {
    members
        .iter()
        .find(|member| member.name == SHIPPED)
        .map(|member| member.version.as_str())
        .ok_or_else(|| format!("the workspace has no {SHIPPED} crate"))
}

/// Every version source that disagrees with the shipped one, one line each.
pub fn drift_of(
    members: &[Member],
    tauri_conf: &serde_json::Value,
    ui_package: &serde_json::Value,
) -> Result<Vec<String>, String> {
    let shipped = shipped_version(members)?;
    let mut drift: Vec<String> = members
        .iter()
        .filter(|member| member.version != shipped)
        .map(|member| format!("{} is {}, the app ships {shipped}", member.name, member.version))
        .collect();
    if tauri_conf.get("version").is_some() {
        drift.push(
            "ui/src-tauri/tauri.conf.json: carries a version; Tauri takes the crate's".to_string(),
        );
    }
    if !msi_takes(shipped) {
        drift.push(format!(
            "{SHIPPED} {shipped}: an MSI takes <=255.<=255.<=65535 with at most a numeric pre-release <=65535 (0.1.0-1, never 0.1.0-rc.1)"
        ));
    }
    let ui_version = ui_package["version"].as_str();
    if ui_version != Some(shipped) {
        drift.push(format!("ui/package.json: version {ui_version:?}, the app ships {shipped}"));
    }
    Ok(drift)
}

fn msi_takes(version: &str) -> bool {
    let (core, pre) = version.split_once('-').unwrap_or((version, "0"));
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    let parts: Vec<&str> = core.split('.').collect();
    numeric(pre)
        && pre.parse::<u16>().is_ok()
        && parts.len() == 3
        && parts.iter().all(|part| numeric(part))
        && parts[0].parse::<u8>().is_ok()
        && parts[1].parse::<u8>().is_ok()
        && parts[2].parse::<u16>().is_ok()
}

pub fn version_drift(repo: &Path) -> Result<Vec<String>, String> {
    let json = |path: &str| -> Result<serde_json::Value, String> {
        let text =
            std::fs::read_to_string(repo.join(path)).map_err(|e| format!("read {path}: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
    };
    drift_of(
        &workspace_members(repo)?,
        &json("ui/src-tauri/tauri.conf.json")?,
        &json("ui/package.json")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(universal: bool, profile: Option<&str>) -> Options {
        Options { universal, profile: profile.map(str::to_string), stage: Stage::BuildAndBundle }
    }

    fn staged(stage: Stage, universal: bool) -> Options {
        Options { universal, profile: None, stage }
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
    fn build_and_bundle_never_ask_tauri_to_sign_and_the_sign_stage_only_signs() {
        let build = plan(Os::Windows, &staged(Stage::BuildOnly, false), &[]).unwrap();
        assert_eq!(argv(&build), "pnpm.cmd tauri build --no-bundle");
        let bundle = plan(Os::Windows, &staged(Stage::BundleOnly, false), &[]).unwrap();
        assert_eq!(argv(&bundle), "pnpm.cmd tauri bundle --bundles msi,nsis");
        assert_eq!(bundle.bundle_dir, Path::new("release/bundle"));
        for os in [Os::MacOs, Os::Windows, Os::Linux] {
            for stage in [Stage::BuildAndBundle, Stage::BuildOnly, Stage::BundleOnly] {
                let argv = argv(&plan(os, &staged(stage, false), &[]).unwrap());
                assert!(!argv.contains("createUpdaterArtifacts"), "{argv}");
            }
        }
        let sign = plan(Os::MacOs, &staged(Stage::SignOnly, true), &UNIVERSAL_PARTS).unwrap();
        assert_eq!(argv(&sign), "pnpm tauri signer sign");
        assert_eq!(sign.bundle_dir, Path::new("universal-apple-darwin/release/bundle"));
        let universal =
            plan(Os::MacOs, &staged(Stage::BundleOnly, true), &UNIVERSAL_PARTS).unwrap();
        assert!(argv(&universal).ends_with("--target universal-apple-darwin"));
        assert_eq!(universal.bundle_dir, Path::new("universal-apple-darwin/release/bundle"));
        let universal = plan(Os::MacOs, &staged(Stage::BuildOnly, true), &UNIVERSAL_PARTS).unwrap();
        assert_eq!(
            argv(&universal),
            "pnpm tauri build --no-bundle --target universal-apple-darwin"
        );
    }

    #[test]
    fn a_bundle_only_or_sign_only_run_refuses_a_profile() {
        for stage in [Stage::BundleOnly, Stage::SignOnly] {
            let options = Options { profile: Some("x".into()), ..staged(stage, false) };
            assert!(plan(Os::Linux, &options, &[]).unwrap_err().contains("drop --profile"));
        }
    }

    #[test]
    fn the_signed_set_is_every_installer_the_release_manifest_maps() {
        let dir = tempfile::tempdir().unwrap();
        let names = [
            "Trawler.app.tar.gz",
            "Trawler.app.tar.gz.sig",
            "Trawler_0.1.0_universal.dmg",
            "Trawler_0.1.0_x64_en-US.msi",
            "Trawler_0.1.0_x64-setup.exe",
            "Trawler_0.1.0_amd64.AppImage",
            "Trawler_0.1.0_amd64.deb",
            "Trawler-0.1.0-1.x86_64.rpm",
        ];
        let mut built: Vec<PathBuf> = names.iter().map(|name| dir.path().join(name)).collect();
        for path in &built {
            std::fs::write(path, "").unwrap();
        }
        std::fs::create_dir(dir.path().join("Trawler.app")).unwrap();
        built.push(dir.path().join("Trawler.app"));
        let signed: Vec<String> = updater_targets(&built)
            .unwrap()
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            signed,
            [
                "Trawler.app.tar.gz",
                "Trawler_0.1.0_x64_en-US.msi",
                "Trawler_0.1.0_x64-setup.exe",
                "Trawler_0.1.0_amd64.AppImage",
                "Trawler_0.1.0_amd64.deb",
                "Trawler-0.1.0-1.x86_64.rpm",
            ]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_app_archive_holds_the_app_at_its_root_without_apple_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Trawler.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/MacOS/trawler-ui"), "bin").unwrap();
        archive_for_the_updater(&app).unwrap();
        let listed = Command::new("tar")
            .arg("-tzf")
            .arg(dir.path().join("Trawler.app.tar.gz"))
            .output()
            .unwrap();
        let listed = String::from_utf8(listed.stdout).unwrap();
        assert!(listed.lines().any(|l| l == "Trawler.app/Contents/MacOS/trawler-ui"), "{listed}");
        assert!(listed.lines().all(|l| l.starts_with("Trawler.app") && !l.contains("._")));
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
        assert_eq!(parse(&["--build-only"]).unwrap().stage, Stage::BuildOnly);
        assert_eq!(parse(&["--sign-only"]).unwrap().stage, Stage::SignOnly);
        assert_eq!(
            parse(&["--universal", "--bundle-only"]).unwrap(),
            staged(Stage::BundleOnly, true)
        );
        for bad in [
            &["--sign"][..],
            &["--build-only", "--bundle-only"],
            &["--bundle-only", "--bundle-only"],
            &["--bundle-only", "--sign-only"],
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
        std::fs::write(bundle.join("macos/Trawler.app.tar.gz"), "").unwrap();
        std::fs::write(bundle.join("macos/Trawler.app.tar.gz.sig"), "").unwrap();
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
                "macos/Trawler.app.tar.gz",
                "macos/Trawler.app.tar.gz.sig",
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

    fn member(name: &str, version: &str) -> Member {
        Member { name: name.to_string(), version: version.to_string() }
    }

    fn one_version() -> Vec<Member> {
        ["trawler", "trawler-ui", "xtask"].map(|name| member(name, "1.2.3-1")).into()
    }

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn the_shipped_version_is_the_app_crate_s() {
        let mut members = one_version();
        members[1].version = "2.0.0".to_string();
        assert_eq!(shipped_version(&members), Ok("2.0.0"));
        assert!(shipped_version(&members[..1]).unwrap_err().contains("no trawler-ui crate"));
    }

    #[test]
    fn one_version_everywhere_is_no_drift() {
        let drift = drift_of(&one_version(), &json("{}"), &json(r#"{"version":"1.2.3-1"}"#));
        assert_eq!(drift.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn each_source_that_disagrees_is_named() {
        let mut members = one_version();
        members[2].version = "0.1.0".to_string();
        let drift =
            drift_of(&members, &json(r#"{"version":"1.2.3-1"}"#), &json(r#"{"version":"1.2.3"}"#))
                .unwrap();
        assert_eq!(
            drift,
            [
                "xtask is 0.1.0, the app ships 1.2.3-1",
                "ui/src-tauri/tauri.conf.json: carries a version; Tauri takes the crate's",
                "ui/package.json: version Some(\"1.2.3\"), the app ships 1.2.3-1",
            ]
        );
    }

    #[test]
    fn only_a_version_an_msi_can_carry_ships() {
        for good in ["0.1.0", "0.1.0-1", "255.255.65535-65535"] {
            assert!(msi_takes(good), "{good}");
        }
        for bad in
            ["0.1.0-rc.1", "0.1.0-65536", "0.1.0-+1", "0.1.0-1.2", "0.1.0+5", "256.0.0", "0.1"]
        {
            assert!(!msi_takes(bad), "{bad}");
        }
        let mut members = one_version();
        for member in &mut members {
            member.version = "0.1.0-rc.1".to_string();
        }
        let drift = drift_of(&members, &json("{}"), &json(r#"{"version":"0.1.0-rc.1"}"#)).unwrap();
        assert_eq!(drift.len(), 1, "{drift:?}");
        assert!(drift[0].contains("never 0.1.0-rc.1"), "{drift:?}");
    }

    #[test]
    fn a_package_json_without_a_version_is_drift() {
        let drift = drift_of(&one_version(), &json("{}"), &json("{}")).unwrap();
        assert_eq!(drift, ["ui/package.json: version None, the app ships 1.2.3-1"]);
    }

    #[test]
    fn the_tree_carries_one_version() {
        let repo = crate::repo_root().expect("repo root");
        assert_eq!(version_drift(&repo).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn package_is_documented() {
        assert!(crate::USAGE.contains(
            "cargo xtask package [--universal] [--profile <name>] [--build-only|--bundle-only|--sign-only]"
        ));
    }
}
