use serde_yaml::Value;

fn text() -> String {
    let repo = xtask::repo_root().unwrap();
    std::fs::read_to_string(repo.join(".github/workflows/release.yml"))
        .expect("the release workflow exists")
}

fn workflow() -> Value {
    serde_yaml::from_str(&text()).expect("the release workflow is YAML")
}

fn every_step() -> Vec<(String, Value)> {
    keys(&workflow()["jobs"])
        .into_iter()
        .flat_map(|name| steps(&job(&name)).into_iter().map(move |step| (name.clone(), step)))
        .collect()
}

fn uses_of(step: &Value) -> &str {
    step["uses"].as_str().unwrap_or_default()
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
    run_of(step).contains("cargo xtask package") && run_of(step).contains("--build-only")
}

fn bundles(step: &Value) -> bool {
    run_of(step).contains("cargo xtask package") && run_of(step).contains("--bundle-only")
}

fn signs(step: &Value) -> bool {
    run_of(step).contains("cargo xtask package") && run_of(step).contains("--sign-only")
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
    let mac = &matrix[0];
    assert_eq!(mac["package-args"].as_str(), Some("--universal"));
    assert!(mac["targets"].as_str().unwrap().contains("aarch64-apple-darwin"));
    assert!(mac["targets"].as_str().unwrap().contains("x86_64-apple-darwin"));
}

#[test]
fn a_red_row_lets_the_others_finish_but_the_release_still_needs_every_build() {
    assert_eq!(job("build")["strategy"]["fail-fast"], Value::Bool(false));
    assert_eq!(job("release")["needs"].as_str(), Some("build"));
}

#[test]
fn each_build_compiles_then_bundles_then_smokes_then_signs_then_uploads() {
    let build = job("build");
    let compile = position(&build, builds, "cargo xtask package --build-only");
    let bundle = position(&build, bundles, "cargo xtask package --bundle-only");
    let smoke = position(&build, smokes, "cargo xtask smoke");
    let sign = position(&build, signs, "cargo xtask package --sign-only");
    let upload = position(&build, uploads, "upload-artifact");
    assert!(
        compile < bundle && bundle < smoke && smoke < sign && sign < upload,
        "compile {compile}, bundle {bundle}, smoke {smoke}, sign {sign}, upload {upload}"
    );
    let packaging =
        steps(&build).iter().filter(|s| run_of(s).contains("cargo xtask package")).count();
    assert_eq!(packaging, 3, "one compile, one bundle, one sign");
    assert_eq!(steps(&build).iter().filter(|s| uploads(s)).count(), 1);
}

#[test]
fn the_smoke_checks_the_bundle_the_package_steps_built() {
    let build = job("build");
    for (wanted, what) in [
        (builds as fn(&Value) -> bool, "compile"),
        (bundles, "bundle"),
        (smokes, "smoke"),
        (signs, "sign"),
    ] {
        let run = run_of(&steps(&build)[position(&build, wanted, what)]);
        assert!(run.contains("matrix.package-args"), "{what}: {run}");
    }
}

#[test]
fn every_build_proves_its_rustc_host_is_the_row_s_architecture() {
    let build = job("build");
    let rows = build["strategy"]["matrix"]["include"].as_sequence().unwrap().clone();
    let hosts: Vec<(&str, &str)> =
        rows.iter().map(|r| (r["name"].as_str().unwrap(), r["host"].as_str().unwrap())).collect();
    assert_eq!(
        hosts,
        [
            ("macos-universal", "aarch64-apple-darwin"),
            ("windows-x64", "x86_64-pc-windows-msvc"),
            ("windows-arm64", "aarch64-pc-windows-msvc"),
            ("linux-x64", "x86_64-unknown-linux-gnu"),
            ("linux-arm64", "aarch64-unknown-linux-gnu"),
        ]
    );
    let check = position(
        &build,
        |s| run_of(s).contains("host=$(rustc -vV | sed -n 's/^host: //p')"),
        "a rustc host check",
    );
    let run = run_of(&steps(&build)[check]);
    assert!(run.contains("if [ \"$host\" != \"$HOST\" ]") && run.contains("exit 1"), "{run}");
    assert_eq!(steps(&build)[check]["env"]["HOST"].as_str(), Some("${{ matrix.host }}"));
    assert!(check < position(&build, builds, "compile"));
    assert_eq!(build["defaults"]["run"]["shell"].as_str(), Some("bash"), "sed runs on Windows too");
}

fn closes_its_pipe_early(line: &str) -> bool {
    line.replace("||", "").split('|').skip(1).any(|stage| {
        let reader = stage.trim_start();
        ["grep -q", "grep -l", "grep -m", "head"].iter().any(|early| reader.starts_with(early))
    })
}

#[test]
fn no_pipe_feeds_a_reader_that_stops_early_under_pipefail() {
    assert!(closes_its_pipe_early("rustc -vV | grep -qx \"host: $HOST\""));
    assert!(closes_its_pipe_early("ls |head -1"));
    assert!(!closes_its_pipe_early("rustc -vV | sed -n 's/^host: //p'"));
    assert!(!closes_its_pipe_early("a || grep -q b"));
    for (job, step) in every_step() {
        for line in run_of(&step).lines() {
            assert!(!closes_its_pipe_early(line), "{job}: SIGPIPE under pipefail fails {line}");
        }
    }
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
fn the_signing_key_reaches_the_sign_step_alone() {
    let squeezed: String = text().split_whitespace().collect();
    assert!(
        !squeezed.contains("toJSON(secrets") && !squeezed.contains("secrets["),
        "secrets by name"
    );
    assert_eq!(squeezed.matches("secrets.").count(), 3, "the presence check and the sign's two");
    assert_eq!(squeezed.matches("secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD").count(), 1);
    let carrying: Vec<(String, Value)> = every_step()
        .into_iter()
        .filter(|(_, step)| serde_yaml::to_string(step).unwrap().contains("secrets."))
        .collect();
    let build = job("build");
    let sign = steps(&build)[position(&build, signs, "sign")].clone();
    let presence = steps(&build)[0].clone();
    assert_eq!(
        carrying,
        [("build".to_string(), presence.clone()), ("build".to_string(), sign.clone())]
    );
    assert_eq!(
        keys(&sign["env"]),
        ["TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"]
    );
    assert!(sign.get("uses").is_none(), "the key goes to the pinned tauri signer alone");
    assert_eq!(
        sign["env"]["TAURI_SIGNING_PRIVATE_KEY"].as_str(),
        Some("${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}")
    );
    assert_eq!(
        sign["env"]["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"].as_str(),
        Some("${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}")
    );
    assert_eq!(
        presence["env"]["KEY_PRESENT"].as_str(),
        Some("${{ secrets.TAURI_SIGNING_PRIVATE_KEY != '' }}"),
        "the first build step sees whether the key is set, never its value"
    );
    assert!(run_of(&presence).contains("exit 1"), "{presence:?}");
}

#[test]
fn every_action_is_pinned_to_a_full_commit_sha() {
    let pinned = |uses: &str| {
        uses.split_once('@').is_some_and(|(_, sha)| {
            sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    };
    assert!(!pinned("actions/checkout@v4") && !pinned("dtolnay/rust-toolchain@stable"));
    let used: Vec<String> = every_step()
        .iter()
        .map(|(_, s)| uses_of(s).to_string())
        .filter(|u| !u.is_empty())
        .collect();
    assert!(used.len() >= 9, "{used:?}");
    for uses in used {
        assert!(pinned(&uses), "{uses} is not pinned to a commit");
    }
}

#[test]
fn a_release_restores_no_cache() {
    for (job, step) in every_step() {
        assert!(!uses_of(&step).contains("cache"), "{job}: {step:?}");
        assert!(step["with"].get("cache").is_none(), "{job}: {step:?}");
    }
}

#[test]
fn no_checkout_leaves_the_token_in_git() {
    let checkouts: Vec<Value> = every_step()
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| uses_of(s).starts_with("actions/checkout@"))
        .collect();
    assert_eq!(checkouts.len(), 3);
    for checkout in checkouts {
        assert_eq!(checkout["with"]["persist-credentials"], Value::Bool(false));
    }
}

#[test]
fn one_release_per_tag_runs_at_a_time_and_none_is_cancelled() {
    let concurrency = &workflow()["concurrency"];
    assert_eq!(concurrency["group"].as_str(), Some("release-${{ github.ref }}"));
    assert_eq!(concurrency["cancel-in-progress"], Value::Bool(false));
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
}

#[test]
fn a_rerun_replaces_only_a_draft_of_its_tag() {
    let runs: Vec<String> = steps(&job("release")).iter().map(run_of).collect();
    let create = runs.iter().find(|r| r.contains("gh release create")).unwrap();
    let replace = create.find("gh release delete \"$TAG\"").expect("a stale draft is deleted");
    let guard = create.find("--json isDraft").expect("only when it is a draft");
    assert!(guard < replace && replace < create.find("gh release create").unwrap(), "{create}");
    assert!(create.contains("= true ]"), "{create}");
    assert!(!create.contains("--cleanup-tag"), "the tag stays");
}

#[test]
fn the_downloaded_installers_are_flattened_into_dist_before_the_manifest_reads_them() {
    let runs: Vec<String> = steps(&job("release")).iter().map(run_of).collect();
    let flatten = runs
        .iter()
        .position(|r| r.contains("find dist") && r.contains("-mindepth 2") && r.contains("mv -n"))
        .expect("a step moves every downloaded file up into dist, never over another");
    assert!(
        runs[flatten].contains("find dist -mindepth 1 -type d)")
            && runs[flatten].contains("exit 1"),
        "a name two artifacts share leaves a directory behind and turns the run red"
    );
    let manifest = runs.iter().position(|r| r.contains("cargo xtask release-manifest")).unwrap();
    assert!(flatten < manifest, "flatten {flatten}, manifest {manifest}");
    let download = steps(&job("release"))
        .into_iter()
        .find(|s| uses_of(s).starts_with("actions/download-artifact"))
        .unwrap();
    assert!(download["with"].get("merge-multiple").is_none(), "a merge would overwrite silently");
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
    for name in keys(&workflow()["jobs"]) {
        let setups: Vec<Value> = steps(&job(&name))
            .into_iter()
            .filter(|s| uses_of(s).starts_with("dtolnay/rust-toolchain@"))
            .collect();
        assert_eq!(setups.len(), 1, "{name}: one toolchain step");
        assert_eq!(setups[0]["with"]["toolchain"].as_str(), Some(channel.as_str()), "{name}");
    }
}

#[test]
fn no_step_can_fail_without_turning_the_run_red() {
    for name in keys(&workflow()["jobs"]) {
        let job = job(&name);
        assert!(job.get("continue-on-error").is_none(), "{name}");
        assert!(job["timeout-minutes"].as_u64().is_some_and(|m| m <= 120), "{name}: a timeout");
        for step in steps(&job) {
            assert!(step.get("continue-on-error").is_none(), "{name}: {step:?}");
        }
    }
}

#[test]
fn each_row_uploads_from_the_bundle_dir_its_package_args_build() {
    use xtask::package::{self, Os};
    let rows = job("build")["strategy"]["matrix"]["include"].as_sequence().unwrap().clone();
    for row in rows {
        let os = match row["os"].as_str().unwrap() {
            runner if runner.starts_with("macos") => Os::MacOs,
            runner if runner.starts_with("windows") => Os::Windows,
            _ => Os::Linux,
        };
        let args: Vec<&str> = row["package-args"].as_str().unwrap().split_whitespace().collect();
        let options = package::parse(&args).unwrap();
        let plan = package::plan(os, &options, &package::UNIVERSAL_PARTS, "0.1.0").unwrap();
        let built: Vec<String> =
            plan.bundle_dir.iter().map(|part| part.to_string_lossy().into_owned()).collect();
        assert_eq!(
            row["bundle"].as_str(),
            Some(format!("target/{}", built.join("/")).as_str()),
            "{row:?}"
        );
    }
}

#[test]
fn the_checksums_are_rechecked_by_another_tool_and_ship_with_the_draft() {
    let runs: Vec<String> = steps(&job("release")).iter().map(run_of).collect();
    let manifest = runs.iter().position(|r| r.contains("cargo xtask release-manifest")).unwrap();
    let check = runs
        .iter()
        .position(|r| r.contains("sha256sum --strict -c SHA256SUMS"))
        .expect("a step re-hashes every installer SHA256SUMS names");
    let create = runs.iter().position(|r| r.contains("gh release create")).unwrap();
    assert!(manifest < check && check <= create, "manifest {manifest}, check {check}");
    assert!(runs[create].contains("dist/*"), "SHA256SUMS is attached from dist");
}
