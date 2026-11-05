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

fn job() -> Value {
    let workflow = workflow();
    let jobs = &workflow["jobs"];
    assert_eq!(keys(jobs).len(), 1, "one job until E-PKG2.W2 adds the other platforms");
    jobs[keys(jobs)[0].as_str()].clone()
}

fn runs(job: &Value) -> Vec<(String, String)> {
    job["steps"]
        .as_sequence()
        .expect("steps")
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

// The workflow quotes `on`: a YAML 1.1 reader takes the bare word for true.
#[test]
fn ci_runs_on_a_push_to_main_and_by_hand_and_on_nothing_else() {
    let workflow = workflow();
    let on = &workflow["on"];
    assert_eq!(keys(on), ["push", "workflow_dispatch"]);
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
fn the_one_job_runs_on_macos() {
    assert_eq!(job()["runs-on"].as_str(), Some("macos-latest"));
}

#[test]
fn the_job_installs_the_ui_from_its_lockfile_then_lints_and_tests_the_core_and_the_app() {
    let runs = runs(&job());
    let install = position(&runs, "pnpm install --frozen-lockfile");
    assert_eq!(runs[install].0, "ui", "the UI's packages install in ui/");
    let clippy =
        position(&runs, "cargo clippy --workspace --all-targets --all-features -- -D warnings");
    let tests = position(&runs, "cargo test -p trawler -p trawler-ui");
    assert!(install < clippy && install < tests, "the app's build script needs ui/node_modules");
}
