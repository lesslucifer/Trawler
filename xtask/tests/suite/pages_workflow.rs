use serde_yaml::Value;

const SITE: &str = "target/site";

fn text_of(file: &str) -> String {
    let repo = xtask::repo_root().unwrap();
    std::fs::read_to_string(repo.join(".github/workflows").join(file))
        .unwrap_or_else(|e| panic!("read {file}: {e}"))
}

fn workflow() -> Value {
    serde_yaml::from_str(&text_of("pages.yml")).expect("the Pages workflow is YAML")
}

fn yaml(text: &str) -> Value {
    serde_yaml::from_str(text).unwrap()
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

fn every_step() -> Vec<(String, Value)> {
    keys(&workflow()["jobs"])
        .into_iter()
        .flat_map(|name| steps(&job(&name)).into_iter().map(move |step| (name.clone(), step)))
        .collect()
}

fn uses_of(step: &Value) -> &str {
    step["uses"].as_str().unwrap_or_default()
}

fn run_of(step: &Value) -> String {
    step["run"].as_str().unwrap_or_default().to_string()
}

fn step_using(job: &Value, action: &str) -> Value {
    let found: Vec<Value> =
        steps(job).into_iter().filter(|s| uses_of(s).starts_with(&format!("{action}@"))).collect();
    assert_eq!(found.len(), 1, "one {action} step");
    found[0].clone()
}

fn pinned_lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.trim_start().trim_start_matches("- ").strip_prefix("uses:"))
        .map(|rest| {
            let (uses, comment) = rest.split_once(" # ").unwrap_or((rest, ""));
            (uses.trim().to_string(), comment.trim().to_string())
        })
        .collect()
}

#[test]
fn the_site_deploys_on_a_push_to_main_a_published_release_and_by_hand_only() {
    let on = &workflow()["on"];
    let mut triggers = keys(on);
    triggers.sort();
    assert_eq!(triggers, ["push", "release", "workflow_dispatch"]);
    assert_eq!(keys(&on["push"]), ["branches"], "no paths or tags filter on the push");
    assert_eq!(on["push"]["branches"], yaml("[main]"));
    assert_eq!(keys(&on["release"]), ["types"]);
    assert_eq!(on["release"]["types"], yaml("[published]"));
}

#[test]
fn a_published_pre_release_skips_the_build_and_so_the_deploy() {
    assert_eq!(
        job("build")["if"].as_str(),
        Some("github.event_name != 'release' || !github.event.release.prerelease")
    );
    assert_eq!(job("deploy")["needs"].as_str(), Some("build"));
    assert!(job("deploy").get("if").is_none(), "the deploy follows the build alone");
}

#[test]
fn only_the_deploy_job_can_write_pages_and_mint_an_id_token() {
    let workflow = workflow();
    assert_eq!(workflow["permissions"], yaml("{contents: read}"));
    let mut jobs = keys(&workflow["jobs"]);
    jobs.sort();
    assert_eq!(jobs, ["build", "deploy"]);
    assert!(job("build")["permissions"].is_null(), "the build keeps the read-only default");
    assert_eq!(job("deploy")["permissions"], yaml("{pages: write, id-token: write}"));
}

#[test]
fn every_action_is_pinned_to_a_full_sha_with_its_version_beside_it() {
    let pages = pinned_lines(&text_of("pages.yml"));
    let used: Vec<String> = every_step()
        .iter()
        .map(|(_, s)| uses_of(s).to_string())
        .filter(|u| !u.is_empty())
        .collect();
    assert_eq!(pages.iter().map(|(uses, _)| uses.clone()).collect::<Vec<_>>(), used);
    for (uses, comment) in &pages {
        let sha = uses.split_once('@').map(|(_, sha)| sha).unwrap_or_default();
        assert!(
            sha.len() == 40
                && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{uses} is not pinned to a commit"
        );
        assert!(!comment.is_empty(), "{uses} carries no version comment");
    }
}

#[test]
fn the_actions_release_yml_also_uses_carry_its_exact_pin() {
    let release = pinned_lines(&text_of("release.yml"));
    let action = |uses: &str| uses.split('@').next().unwrap_or_default().to_string();
    let pages = pinned_lines(&text_of("pages.yml"));
    let shared: Vec<String> = pages
        .iter()
        .filter(|(uses, _)| release.iter().any(|(r, _)| action(r) == action(uses)))
        .map(|(uses, _)| action(uses))
        .collect();
    for wanted in ["actions/checkout", "dtolnay/rust-toolchain"] {
        assert!(shared.iter().any(|a| a == wanted), "{wanted} is not shared: {shared:?}");
    }
    for pin in pages.iter().filter(|(uses, _)| shared.contains(&action(uses))) {
        for theirs in release.iter().filter(|(r, _)| action(r) == action(&pin.0)) {
            assert_eq!(pin, theirs, "pages.yml and release.yml pin {} differently", action(&pin.0));
        }
    }
}

#[test]
fn the_toolchain_is_the_pinned_one() {
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
    let setup = step_using(&job("build"), "dtolnay/rust-toolchain");
    assert_eq!(setup["with"]["toolchain"].as_str(), Some(channel.as_str()));
}

#[test]
fn no_checkout_leaves_the_token_in_git() {
    let checkouts: Vec<Value> = every_step()
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| uses_of(s).starts_with("actions/checkout@"))
        .collect();
    assert_eq!(checkouts.len(), 1);
    assert_eq!(checkouts[0]["with"]["persist-credentials"], Value::Bool(false));
}

#[test]
fn the_build_reads_the_latest_release_never_the_event_payload() {
    let build = job("build");
    let read: Vec<Value> =
        steps(&build).into_iter().filter(|s| run_of(s).contains("gh api")).collect();
    assert_eq!(read.len(), 1, "one release read");
    let run = run_of(&read[0]);
    assert!(run.contains("gh api \"repos/$GITHUB_REPOSITORY/releases/latest\""), "{run}");
    assert_eq!(read[0]["env"]["GH_TOKEN"].as_str(), Some("${{ github.token }}"));
    for (job, step) in every_step() {
        let squeezed = serde_yaml::to_string(&step).unwrap();
        assert!(!squeezed.contains("github.event"), "{job}: {step:?} reads the event");
    }
}

#[test]
fn the_build_writes_the_site_into_the_one_dir_the_artifact_uploads() {
    let build = job("build");
    let runs: Vec<String> = steps(&build).iter().map(run_of).collect();
    let read = runs.iter().position(|r| r.contains("releases/latest")).unwrap();
    let site = runs
        .iter()
        .position(|r| r.contains("cargo xtask site build"))
        .expect("a step builds the site");
    assert!(read < site, "the release JSON exists before the build reads it");
    assert!(runs[site].contains(&format!("--out {SITE}")), "{}", runs[site]);
    let release = runs[read].rsplit("> ").next().unwrap().trim().to_string();
    assert!(runs[site].contains(&format!("--release {release}")), "{}", runs[site]);
    let upload = step_using(&build, "actions/upload-pages-artifact");
    assert_eq!(keys(&upload["with"]), ["path"]);
    assert_eq!(upload["with"]["path"].as_str(), Some(SITE));
    let upload_at = steps(&build).iter().position(|s| *s == upload).unwrap();
    assert!(site < upload_at, "the upload follows the build");
}

#[test]
fn the_deploy_publishes_the_artifact_to_the_github_pages_environment() {
    let deploy = job("deploy");
    let step = step_using(&deploy, "actions/deploy-pages");
    assert_eq!(steps(&deploy).len(), 1, "the deploy job checks nothing out and runs nothing");
    let id = step["id"].as_str().expect("the deploy step has an id");
    assert_eq!(deploy["environment"]["name"].as_str(), Some("github-pages"));
    assert_eq!(
        deploy["environment"]["url"].as_str(),
        Some(format!("${{{{ steps.{id}.outputs.page_url }}}}").as_str())
    );
}

#[test]
fn the_base_url_is_the_xtask_constant_so_configure_pages_is_absent() {
    for (job, step) in every_step() {
        assert!(!uses_of(&step).contains("configure-pages"), "{job}: {step:?}");
    }
}

#[test]
fn no_secret_reaches_the_workflow() {
    let squeezed: String = text_of("pages.yml").split_whitespace().collect();
    assert!(!squeezed.contains("secrets"), "the workflow names a secret");
}

#[test]
fn one_deploy_runs_at_a_time_and_none_is_cancelled_half_way() {
    let concurrency = &workflow()["concurrency"];
    assert_eq!(concurrency["group"].as_str(), Some("pages"));
    assert_eq!(concurrency["cancel-in-progress"], Value::Bool(false));
}

#[test]
fn no_step_can_fail_without_turning_the_run_red() {
    for name in keys(&workflow()["jobs"]) {
        let job = job(&name);
        assert!(job.get("continue-on-error").is_none(), "{name}");
        assert!(job["timeout-minutes"].as_u64().is_some_and(|m| m <= 30), "{name}: a timeout");
        for step in steps(&job) {
            assert!(step.get("continue-on-error").is_none(), "{name}: {step:?}");
            assert!(step.get("if").is_none(), "{name}: {step:?} can be skipped");
        }
    }
}

#[test]
fn a_pinned_line_reads_its_action_and_its_comment() {
    let lines = pinned_lines(
        "      - uses: a/b@0123 # v1.2.3\n        uses: c/d@v4\n      - run: echo uses: x\n",
    );
    assert_eq!(
        lines,
        [("a/b@0123".to_string(), "v1.2.3".to_string()), ("c/d@v4".to_string(), String::new())]
    );
}
