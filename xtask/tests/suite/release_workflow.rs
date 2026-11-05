use serde_yaml::Value;

fn workflow() -> Value {
    let repo = xtask::repo_root().unwrap();
    let text = std::fs::read_to_string(repo.join(".github/workflows/release.yml"))
        .expect("the release workflow exists");
    serde_yaml::from_str(&text).expect("the release workflow is YAML")
}

fn keys(map: &Value) -> Vec<String> {
    map.as_mapping()
        .map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn job(name: &str) -> Value {
    workflow()["jobs"][name].clone()
}

fn steps(job: &Value) -> Vec<Value> {
    job["steps"].as_sequence().expect("steps").clone()
}

fn run_of(step: &Value) -> String {
    step["run"].as_str().unwrap_or_default().to_string()
}

fn position(job: &Value, wanted: impl Fn(&Value) -> bool, what: &str) -> usize {
    steps(job).iter().position(wanted).unwrap_or_else(|| panic!("no step for {what}"))
}

fn builds(step: &Value) -> bool {
    run_of(step).contains("cargo xtask package")
}

fn smokes(step: &Value) -> bool {
    run_of(step).contains("cargo xtask smoke")
}

fn uploads(step: &Value) -> bool {
    step["uses"].as_str().is_some_and(|u| u.starts_with("actions/upload-artifact"))
}

#[test]
fn a_release_runs_on_a_v_tag_and_nothing_else() {
    let on = &workflow()["on"];
    assert_eq!(keys(on), ["push"]);
    assert_eq!(keys(&on["push"]), ["tags"]);
    assert_eq!(on["push"]["tags"], serde_yaml::from_str::<Value>("[\"v*\"]").unwrap());
}

#[test]
fn only_the_release_job_can_write_the_repository() {
    let workflow = workflow();
    assert_eq!(workflow["permissions"], serde_yaml::from_str::<Value>("{contents: read}").unwrap());
    for name in keys(&workflow["jobs"]) {
        let permissions = &workflow["jobs"][name.as_str()]["permissions"];
        if name == "release" {
            assert_eq!(*permissions, serde_yaml::from_str::<Value>("{contents: write}").unwrap());
        } else {
            assert!(permissions.is_null(), "{name} sets its own permissions");
        }
    }
}

#[test]
fn the_tag_is_checked_against_the_workspace_version_before_any_build() {
    let version = job("version");
    let check =
        steps(&version).iter().map(run_of).find(|run| run.contains("cargo metadata")).unwrap();
    assert!(check.contains("trawler-ui") && check.contains("exit 1"), "{check}");
    assert!(check.contains("v$version"), "the tag keeps its v: {check}");
    assert_eq!(job("build")["needs"].as_str(), Some("version"));
}

#[test]
fn every_target_is_built_on_its_runner() {
    let matrix = job("build")["strategy"]["matrix"]["include"].clone();
    let rows: Vec<(String, String)> = matrix
        .as_sequence()
        .unwrap()
        .iter()
        .map(|row| (row["name"].as_str().unwrap().into(), row["os"].as_str().unwrap().into()))
        .collect();
    let expected = [
        ("macos-universal", "macos-latest"),
        ("windows-x64", "windows-latest"),
        ("windows-arm64", "windows-11-arm"),
        ("linux-x64", "ubuntu-22.04"),
        ("linux-arm64", "ubuntu-22.04-arm"),
    ];
    assert_eq!(rows, expected.map(|(n, o)| (n.to_string(), o.to_string())));
    assert_eq!(job("build")["strategy"]["fail-fast"], Value::Bool(true));
    let mac = &matrix[0];
    assert_eq!(mac["package-args"].as_str(), Some("--universal"));
    assert!(mac["targets"].as_str().unwrap().contains("aarch64-apple-darwin"));
    assert!(mac["targets"].as_str().unwrap().contains("x86_64-apple-darwin"));
}

#[test]
fn each_build_packages_then_smokes_then_uploads() {
    let build = job("build");
    let package = position(&build, builds, "cargo xtask package");
    let smoke = position(&build, smokes, "cargo xtask smoke");
    let upload = position(&build, uploads, "upload-artifact");
    assert!(package < smoke && smoke < upload, "package {package}, smoke {smoke}, upload {upload}");
    assert_eq!(steps(&build).iter().filter(|s| uploads(s)).count(), 1);
}

#[test]
fn the_smoke_checks_the_bundle_the_package_step_built() {
    let build = job("build");
    let package = run_of(&steps(&build)[position(&build, builds, "package")]);
    let smoke = run_of(&steps(&build)[position(&build, smokes, "smoke")]);
    assert!(package.contains("matrix.package-args") && smoke.contains("matrix.package-args"));
}

#[test]
fn an_upload_without_files_is_red_and_carries_every_installer_kind() {
    let build = job("build");
    let upload = steps(&build)[position(&build, uploads, "upload")].clone();
    assert_eq!(upload["with"]["if-no-files-found"].as_str(), Some("error"));
    let path = upload["with"]["path"].as_str().unwrap();
    for kind in [".dmg", ".app.tar.gz", ".msi", "-setup.exe", ".deb", ".rpm", ".AppImage", ".sig"] {
        assert!(path.contains(kind), "the upload misses {kind}");
    }
}

#[test]
fn the_signing_key_is_read_by_secret_name_and_only_where_the_package_is_built() {
    let text =
        std::fs::read_to_string(xtask::repo_root().unwrap().join(".github/workflows/release.yml"))
            .unwrap();
    for secret in ["TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] {
        assert!(text.contains(&format!("secrets.{secret}")), "{secret}");
    }
    let build = job("build");
    let package = steps(&build)[position(&build, builds, "package")].clone();
    assert!(keys(&package["env"]).contains(&"TAURI_SIGNING_PRIVATE_KEY".to_string()));
    let elsewhere = text.matches("secrets.TAURI_SIGNING_PRIVATE_KEY ").count();
    assert_eq!(elsewhere, 1, "the key appears in one step only");
}

#[test]
fn the_release_waits_for_every_build_and_is_only_ever_a_draft() {
    let release = job("release");
    assert_eq!(release["needs"].as_str(), Some("build"));
    let runs: Vec<String> = steps(&release).iter().map(run_of).collect();
    let manifest = runs.iter().position(|r| r.contains("cargo xtask release-manifest")).unwrap();
    let create = runs.iter().position(|r| r.contains("gh release create")).unwrap();
    assert!(manifest < create, "latest.json exists before the release is created");
    assert!(runs[create].contains("--draft"), "{}", runs[create]);
    assert!(!runs.iter().any(|r| r.contains("prerelease") || r.contains("gh release edit")));
    let download = steps(&release)
        .into_iter()
        .find(|s| s["uses"].as_str().is_some_and(|u| u.starts_with("actions/download-artifact")))
        .unwrap();
    assert_eq!(download["with"]["merge-multiple"], Value::Bool(true));
}

#[test]
fn the_toolchain_is_the_pinned_one_in_every_job() {
    let repo = xtask::repo_root().unwrap();
    let pinned = std::fs::read_to_string(repo.join("rust-toolchain.toml")).unwrap();
    let channel = pinned
        .lines()
        .find_map(|l| l.trim().strip_prefix("channel"))
        .unwrap()
        .trim_start_matches([' ', '='])
        .trim()
        .trim_matches('"')
        .to_string();
    for name in ["version", "build", "release"] {
        let setups: Vec<Value> = steps(&job(name))
            .into_iter()
            .filter(|s| s["uses"].as_str().is_some_and(|u| u.starts_with("dtolnay/rust-toolchain")))
            .collect();
        assert_eq!(setups.len(), 1, "{name}: one toolchain step");
        assert_eq!(setups[0]["with"]["toolchain"].as_str(), Some(channel.as_str()), "{name}");
    }
}

#[test]
fn no_step_can_fail_without_turning_the_run_red() {
    for name in ["version", "build", "release"] {
        let job = job(name);
        assert!(job.get("continue-on-error").is_none(), "{name}");
        assert!(job["timeout-minutes"].as_u64().is_some_and(|m| m <= 120), "{name}: a timeout");
        for step in steps(&job) {
            assert!(step.get("continue-on-error").is_none(), "{name}: {step:?}");
        }
    }
}
