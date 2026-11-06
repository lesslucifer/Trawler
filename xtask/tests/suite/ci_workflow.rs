use serde_yaml::Value;

const WORKFLOW: &str = ".github/workflows/ci.yml";

fn workflow() -> Value {
    let repo = xtask::repo_root().unwrap();
    let text = std::fs::read_to_string(repo.join(WORKFLOW)).expect("the CI workflow exists");
    serde_yaml::from_str(&text).expect("the CI workflow is YAML")
}

fn keys(map: &Value) -> Vec<String> {
    map.as_mapping()
        .map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

const OSES: [&str; 3] = ["macos-latest", "ubuntu-22.04", "windows-latest"];

fn jobs() -> Vec<(String, Value)> {
    let workflow = workflow();
    let jobs = &workflow["jobs"];
    keys(jobs).into_iter().map(|key| (key.clone(), jobs[key.as_str()].clone())).collect()
}

fn job_on(os: &str) -> Value {
    let on_os: Vec<Value> = jobs()
        .into_iter()
        .map(|(_, job)| job)
        .filter(|job| job["runs-on"].as_str() == Some(os))
        .collect();
    assert_eq!(on_os.len(), 1, "exactly one job runs on {os}");
    on_os[0].clone()
}

fn steps(job: &Value) -> &Vec<Value> {
    job["steps"].as_sequence().expect("steps")
}

fn runs(job: &Value) -> Vec<(String, String)> {
    steps(job)
        .iter()
        .filter_map(|step| {
            let run = step["run"].as_str()?.trim().to_string();
            let dir = step["working-directory"].as_str().unwrap_or(".").to_string();
            Some((dir, run))
        })
        .collect()
}

fn position(runs: &[(String, String)], wanted: &str) -> usize {
    runs.iter()
        .position(|(_, run)| run == wanted)
        .unwrap_or_else(|| panic!("no step runs {wanted:?}: {runs:?}"))
}

#[test]
fn ci_runs_on_a_push_to_main_and_by_hand_and_on_nothing_else() {
    let workflow = workflow();
    let on = &workflow["on"];
    let mut triggers = keys(on);
    triggers.sort();
    assert_eq!(triggers, ["push", "workflow_dispatch"]);
    assert_eq!(on["push"]["branches"], serde_yaml::from_str::<Value>("[main]").unwrap());
    assert_eq!(keys(&on["push"]), ["branches"], "no paths or tags filter on the push");
}

#[test]
fn ci_reads_the_repository_and_writes_nothing() {
    assert_eq!(
        workflow()["permissions"],
        serde_yaml::from_str::<Value>("{contents: read}").unwrap()
    );
}

#[test]
fn there_is_one_job_per_platform_and_no_other() {
    let mut on: Vec<String> =
        jobs().iter().map(|(_, job)| job["runs-on"].as_str().unwrap_or("").to_string()).collect();
    on.sort();
    assert_eq!(on, OSES);
}

#[test]
fn a_hung_run_is_cut_off_well_before_the_runners_six_hours() {
    for os in OSES {
        let minutes = job_on(os)["timeout-minutes"].as_u64().expect("the job sets timeout-minutes");
        assert!((15..=60).contains(&minutes), "{os}: timeout-minutes is {minutes}");
    }
}

#[test]
fn no_step_can_fail_without_turning_the_run_red() {
    for os in OSES {
        let job = job_on(os);
        assert!(job.get("continue-on-error").is_none(), "{os}: the job continues on error");
        assert!(job.get("if").is_none(), "{os}: the job can be skipped");
        for step in steps(&job) {
            assert!(step.get("continue-on-error").is_none(), "{os}: {step:?} continues on error");
            assert!(step.get("if").is_none(), "{os}: {step:?} can be skipped");
        }
    }
}

#[test]
fn the_toolchains_are_set_up_before_anything_runs() {
    for os in OSES {
        let job = job_on(os);
        let uses: Vec<&str> = steps(&job)
            .iter()
            .filter_map(|step| step["uses"].as_str())
            .map(|action| action.split('@').next().unwrap_or(action))
            .collect();
        for action in ["dtolnay/rust-toolchain", "pnpm/action-setup", "actions/setup-node"] {
            assert!(uses.contains(&action), "{os}: {action} is not set up: {uses:?}");
        }
        let order: Vec<&&str> = uses
            .iter()
            .filter(|a| ["pnpm/action-setup", "actions/setup-node"].contains(a))
            .collect();
        assert_eq!(
            order,
            [&"pnpm/action-setup", &"actions/setup-node"],
            "{os}: setup-node's pnpm cache needs pnpm first"
        );
    }
}

#[test]
fn every_job_installs_the_ui_from_its_lockfile_then_lints_and_tests_the_core_and_the_app() {
    for os in OSES {
        let runs = runs(&job_on(os));
        let install = position(&runs, "pnpm install --frozen-lockfile");
        assert_eq!(runs[install].0, "ui", "{os}: the UI's packages install in ui/");
        let clippy =
            position(&runs, "cargo clippy --workspace --all-targets --all-features -- -D warnings");
        let tests_command = if os == "ubuntu-22.04" {
            "xvfb-run cargo test -p trawler -p trawler-ui -p trawler-login-service"
        } else {
            "cargo test -p trawler -p trawler-ui -p trawler-login-service"
        };
        let tests = position(&runs, tests_command);
        assert!(
            install < clippy && install < tests,
            "{os}: the app's build script needs ui/node_modules"
        );
        assert!(clippy < tests, "{os}: a lint failure is reported before the slow suite runs");
    }
}

#[test]
fn ubuntu_installs_the_tauri_libraries_and_xvfb_before_any_cargo_step() {
    let runs = runs(&job_on("ubuntu-22.04"));
    let apt = runs
        .iter()
        .position(|(_, run)| run.contains("apt-get install"))
        .expect("an apt-get install step");
    let first_cargo = runs.iter().position(|(_, run)| run.contains("cargo ")).expect("cargo");
    assert!(apt < first_cargo, "the system libraries precede the first cargo step");
    for package in ["libwebkit2gtk-4.1-dev", "libgtk-3-dev", "libayatana-appindicator3-dev", "xvfb"]
    {
        assert!(runs[apt].1.contains(package), "apt does not install {package}");
    }
}

fn pinned_channel() -> String {
    let repo = xtask::repo_root().unwrap();
    let text = std::fs::read_to_string(repo.join("rust-toolchain.toml"))
        .expect("rust-toolchain.toml exists");
    let line = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel"))
        .expect("rust-toolchain.toml names a channel");
    line.trim_start_matches([' ', '=']).trim().trim_matches('"').to_string()
}

#[test]
fn the_rust_toolchain_is_one_exact_version_everywhere() {
    let channel = pinned_channel();
    assert!(
        channel.split('.').count() == 3 && channel.split('.').all(|n| n.parse::<u32>().is_ok()),
        "rust-toolchain.toml pins an exact version, not {channel:?}"
    );
    for os in OSES {
        let job = job_on(os);
        let setups: Vec<&Value> = steps(&job)
            .iter()
            .filter(|step| {
                step["uses"].as_str().is_some_and(|u| u.starts_with("dtolnay/rust-toolchain"))
            })
            .collect();
        assert_eq!(setups.len(), 1, "{os}: one toolchain step");
        assert_eq!(
            setups[0]["with"]["toolchain"].as_str(),
            Some(channel.as_str()),
            "{os}: the toolchain step disagrees with rust-toolchain.toml"
        );
    }
}

#[test]
fn the_pinned_toolchain_carries_clippy_and_rustfmt() {
    let repo = xtask::repo_root().unwrap();
    let text = std::fs::read_to_string(repo.join("rust-toolchain.toml")).unwrap();
    assert!(text.contains("\"clippy\"") && text.contains("\"rustfmt\""), "{text}");
}

#[test]
fn the_lockfile_links_one_webview2_binding_so_the_windows_build_has_one_set_of_types() {
    let repo = xtask::repo_root().unwrap();
    let lock = std::fs::read_to_string(repo.join("Cargo.lock")).expect("Cargo.lock exists");
    let versions = |name: &str| -> Vec<String> {
        lock.split("[[package]]")
            .filter(|block| block.lines().any(|l| l == format!("name = \"{name}\"")))
            .filter_map(|block| block.lines().find_map(|l| l.strip_prefix("version = ")))
            .map(str::to_string)
            .collect()
    };
    for name in ["webview2-com", "windows"] {
        let found = versions(name);
        assert_eq!(found.len(), 1, "{name} resolves to more than one version: {found:?}");
    }
}

#[test]
fn the_checkout_keeps_lf_line_endings_so_windows_sees_the_bytes_the_tests_expect() {
    let repo = xtask::repo_root().unwrap();
    let attributes = std::fs::read_to_string(repo.join(".gitattributes")).unwrap_or_default();
    assert!(attributes.lines().any(|l| l.trim() == "* text=auto eol=lf"), "{attributes}");
}
