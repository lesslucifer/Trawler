use std::path::Path;
use std::process::Command;

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let dir = repo.join("ui");
    match args {
        ["build"] => {
            pnpm(&dir, &["install"], &[])?;
            // Tailwind drops an off-vocabulary class silently, so a bundle can
            // be built, typed and tested green while rendering unstyled
            // elements. The lint rule is the only alarm (E-DFLOW.W6, DEC-17).
            pnpm(&dir, &["lint"], &[])?;
            pnpm(&dir, &["build"], &[])?;
            Ok((0, "ui build complete - bundle in ui/dist/".to_string()))
        }
        ["dev"] => {
            pnpm(&dir, &["install"], &[])?;
            pnpm(&dir, DEV_ARGS, &[])?;
            Ok((0, String::new()))
        }
        ["bench"] => {
            pnpm(&dir, &["install"], &[])?;
            pnpm(&dir, &["bench"], &[])?;
            Ok((0, "ui bench complete".to_string()))
        }
        ["e2e"] => e2e(&dir),
        ["walkthrough"] => super::walkthrough::run(&dir, repo),
        ["specimens"] => specimens(repo, "build-specimens.mjs"),
        ["specimens", "--check"] => specimens(repo, "check-render.mjs"),
        _ => Err(super::USAGE.to_string()),
    }
}

// The app embeds ui/dist by default (`custom-protocol`); dev drops that
// feature so the window loads the Vite dev server and keeps HMR.
const DEV_ARGS: &[&str] = &["tauri", "dev", "--", "--no-default-features"];

// Design files render the real ui/src/components/ collection. On a file:// origin
// a page may not read local files and module scripts never execute, so the
// collection is compiled to one classic script at author time instead of load
// time. Node owns that (esbuild, headless Chrome); xtask is the entry point, the
// same way `ui build` fronts pnpm.
fn specimens(repo: &Path, script: &str) -> Result<(i32, String), String> {
    let script_path = repo.join("scripts").join(script);
    let status = Command::new("node")
        .arg(&script_path)
        .current_dir(repo)
        .status()
        .map_err(|e| format!("failed to launch node (is it installed?): {e}"))?;
    if status.success() {
        Ok((0, String::new()))
    } else {
        Err(format!("{script} failed"))
    }
}

// The epic gate: build the frontend + the Tauri app with the `e2e` feature (wdio
// plugins in), then drive the real window with WebdriverIO against an ephemeral
// core the wdio harness spawns. macOS-primary, embedded WebDriver provider.
fn e2e(dir: &Path) -> Result<(i32, String), String> {
    pnpm(dir, &["install"], &[("VITE_E2E", "1")])?;
    pnpm(dir, &["build"], &[("VITE_E2E", "1")])?;
    // tauri build (not raw cargo) merges the e2e config overlay — withGlobalTauri
    // + the wdio capability — so the frontend plugin bridge the service uses works.
    // --no-bundle stops at the binary; the beforeBuildCommand is nulled in the
    // overlay so it reuses the VITE_E2E dist above.
    let build_args = [
        "tauri",
        "build",
        "--no-bundle",
        "--config",
        "src-tauri/tauri.conf.e2e.json",
        "--features",
        "e2e",
    ];
    pnpm(dir, &build_args, &[])?;
    let e2e_dir = dir.join("e2e");
    pnpm(&e2e_dir, &["install"], &[])?;
    let verdicts: Vec<Result<String, String>> =
        SUITES.iter().map(|suite| run_suite(&e2e_dir, suite)).collect();
    judge_suites(&verdicts).map(|summary| (0, summary))
}

// Each suite is its own wdio run: the app's environment is fixed per launcher,
// and the owned suite's app must start with no attach address at all.
struct Suite {
    name: &'static str,
    script: &'static str,
    tally: &'static str,
}

const SUITES: [Suite; 2] = [
    Suite { name: "attached", script: "test", tally: ".run-tally.json" },
    Suite { name: "owned", script: "test:one-app", tally: ".run-tally-one-app.json" },
];

fn run_suite(e2e_dir: &Path, suite: &Suite) -> Result<String, String> {
    let tally = e2e_dir.join(suite.tally);
    let _ = std::fs::remove_file(&tally);
    let ran = pnpm(e2e_dir, &[suite.script], &[]);
    let verdict = judge_run_tally(suite.tally, std::fs::read_to_string(&tally).ok().as_deref());
    let _ = std::fs::remove_file(&tally);
    ran.and(verdict)
        .map(|passed| format!("{} suite: {passed}", suite.name))
        .map_err(|problem| format!("{} suite: {problem}", suite.name))
}

fn judge_suites(verdicts: &[Result<String, String>]) -> Result<String, String> {
    let problems: Vec<&str> =
        verdicts.iter().filter_map(|v| v.as_ref().err()).map(String::as_str).collect();
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let passed: Vec<&str> =
        verdicts.iter().filter_map(|v| v.as_ref().ok()).map(String::as_str).collect();
    Ok(format!("ui e2e complete - {}", passed.join("; ")))
}

#[derive(serde::Deserialize)]
struct RunTally {
    specs: u64,
    passed: u64,
    failed: u64,
}

fn judge_run_tally(file: &str, raw: Option<&str>) -> Result<String, String> {
    let raw =
        raw.ok_or(format!("the e2e run left no ui/e2e/{file}, so nothing says it ran a spec"))?;
    let tally: RunTally =
        serde_yaml::from_str(raw).map_err(|e| format!("ui/e2e/{file} is not a run tally: {e}"))?;
    if tally.specs == 0 {
        return Err("the e2e run is configured with no spec file".to_string());
    }
    if tally.failed > 0 {
        return Err(format!("{} of {} spec files failed", tally.failed, tally.specs));
    }
    if tally.passed < tally.specs {
        return Err(format!(
            "only {} of {} spec files passed, so the rest never ran",
            tally.passed, tally.specs
        ));
    }
    Ok(format!("{} of {} spec files passed", tally.passed, tally.specs))
}

fn pnpm(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<(), String> {
    let mut cmd = Command::new("pnpm");
    cmd.args(args).current_dir(dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status =
        cmd.status().map_err(|e| format!("failed to launch pnpm (is it installed?): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("pnpm {} failed", args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::{judge_run_tally, judge_suites, run, DEV_ARGS, SUITES};
    use std::collections::BTreeSet;
    use std::path::Path;

    fn tally(specs: u64, passed: u64, failed: u64) -> String {
        format!(
            r#"{{"specs":{specs},"finished":{},"passed":{passed},"failed":{failed}}}"#,
            passed + failed
        )
    }

    #[test]
    fn dev_hands_cargo_no_default_features_so_the_window_loads_the_dev_server() {
        let runner_args = DEV_ARGS.iter().skip_while(|arg| **arg != "--").skip(1);
        assert_eq!(runner_args.collect::<Vec<_>>(), [&"--no-default-features"]);
    }

    #[test]
    fn a_run_is_green_only_when_every_spec_file_passed() {
        assert_eq!(
            judge_run_tally(".run-tally.json", Some(&tally(4, 4, 0))).unwrap(),
            "4 of 4 spec files passed"
        );
    }

    #[test]
    fn a_run_that_left_no_tally_is_red() {
        assert!(judge_run_tally(".run-tally.json", None)
            .unwrap_err()
            .contains("no ui/e2e/.run-tally.json"));
    }

    #[test]
    fn a_tally_that_does_not_parse_is_red() {
        assert!(judge_run_tally(".run-tally.json", Some("{\"specs\":"))
            .unwrap_err()
            .contains("is not a run tally"));
    }

    #[test]
    fn a_run_configured_with_no_spec_is_red() {
        assert!(judge_run_tally(".run-tally.json", Some(&tally(0, 0, 0)))
            .unwrap_err()
            .contains("no spec file"));
    }

    #[test]
    fn a_failed_spec_file_is_red() {
        assert_eq!(
            judge_run_tally(".run-tally.json", Some(&tally(4, 3, 1))).unwrap_err(),
            "1 of 4 spec files failed"
        );
    }

    #[test]
    fn a_run_that_passed_fewer_spec_files_than_configured_is_red() {
        assert_eq!(
            judge_run_tally(".run-tally.json", Some(&tally(4, 0, 0))).unwrap_err(),
            "only 0 of 4 spec files passed, so the rest never ran"
        );
    }

    #[test]
    fn a_tally_problem_names_the_suite_file() {
        assert!(judge_run_tally(".run-tally-one-app.json", None)
            .unwrap_err()
            .contains("no ui/e2e/.run-tally-one-app.json"));
    }

    #[test]
    fn two_suites_are_green_only_when_both_are() {
        let green = Ok("attached suite: 4 of 4 spec files passed".to_string());
        assert_eq!(
            judge_suites(&[green.clone(), Ok("owned suite: 1 of 1 spec files passed".to_string())])
                .unwrap(),
            "ui e2e complete - attached suite: 4 of 4 spec files passed; owned suite: 1 of 1 spec files passed"
        );
        assert_eq!(
            judge_suites(&[green, Err("owned suite: 1 of 1 spec files failed".to_string())])
                .unwrap_err(),
            "owned suite: 1 of 1 spec files failed"
        );
    }

    #[test]
    fn every_suite_has_its_script_and_its_config_writes_its_tally() {
        let e2e = crate::repo_root().expect("repo root").join("ui/e2e");
        let package = std::fs::read_to_string(e2e.join("package.json")).expect("package.json");
        let scripts: serde_yaml::Value = serde_yaml::from_str(&package).expect("package json");
        for suite in &SUITES {
            let command = scripts["scripts"][suite.script].as_str().unwrap_or_default();
            let config_file = command.strip_prefix("wdio run ").unwrap_or_else(|| {
                panic!("the {} suite's script {:?} is not a wdio run", suite.name, suite.script)
            });
            let config = std::fs::read_to_string(e2e.join(config_file)).expect("suite config");
            assert!(config.contains(suite.tally), "{config_file} does not write {}", suite.tally);
        }
    }

    // In the embedded WebKit provider `*=` never matches and never errors, so a
    // spec using it fails against a screen that renders correctly.
    fn partial_text_selector_lines(source: &str) -> Vec<usize> {
        source
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("*="))
            .map(|(at, _)| at + 1)
            .collect()
    }

    #[test]
    fn the_partial_text_detector_names_the_line() {
        let spec = "const a = $(\"button=send\");\nconst b = $(\"span*=sending\");\n";
        assert_eq!(partial_text_selector_lines(spec), vec![2]);
    }

    #[test]
    fn no_e2e_spec_selects_by_partial_text() {
        let e2e = crate::repo_root().expect("repo root").join("ui/e2e");
        for dir in ["specs", "walkthrough"] {
            for entry in std::fs::read_dir(e2e.join(dir)).expect("an e2e source dir") {
                let path = entry.expect("spec entry").path();
                let source = std::fs::read_to_string(&path).expect("spec source");
                let lines = partial_text_selector_lines(&source);
                assert!(lines.is_empty(), "{} uses *= on lines {lines:?}", path.display());
            }
        }
    }

    // wry answers each WebKit scheme task inside objc2's exception catch, so a
    // task WebKit already stopped throws and is dropped. Under panic = "abort"
    // that catch cannot run, and the throw aborts the app instead.
    fn release_aborts_on_panic(cargo_toml: &str) -> bool {
        let mut in_release = false;
        cargo_toml.lines().map(str::trim).any(|line| {
            if line.starts_with('[') {
                in_release = line == "[profile.release]";
                return false;
            }
            in_release
                && line.split_once('=').is_some_and(|(key, value)| {
                    key.trim() == "panic" && value.trim() == "\"abort\""
                })
        })
    }

    #[test]
    fn the_abort_detector_reads_only_the_release_profile() {
        assert!(release_aborts_on_panic("[profile.release]\nlto = true\npanic = \"abort\"\n"));
        assert!(!release_aborts_on_panic(
            "[profile.dev]\npanic = \"abort\"\n[profile.release]\nlto = true\n"
        ));
        assert!(!release_aborts_on_panic("[profile.release]\npanic = \"unwind\"\n"));
    }

    #[test]
    fn the_shell_unwinds_so_wry_can_catch_a_stopped_task() {
        let manifest = crate::repo_root().expect("repo root").join("ui/src-tauri/Cargo.toml");
        let source = std::fs::read_to_string(&manifest).expect("the shell's Cargo.toml");
        assert!(!release_aborts_on_panic(&source), "{} sets panic = \"abort\"", manifest.display());
    }

    fn literal_after<'a>(source: &'a str, prefix: &str) -> &'a str {
        let start = source.find(prefix).unwrap_or_else(|| panic!("no {prefix}")) + prefix.len();
        let rest = &source[start..];
        &rest[..rest.find('"').expect("a closing quote")]
    }

    fn handler_names(source: &str) -> BTreeSet<String> {
        let start = source.find("generate_handler![").expect("a generate_handler!") + 18;
        let list = &source[start..start + source[start..].find(']').expect("a closing ]")];
        list.split(',')
            .map(|path| path.trim().rsplit("::").next().unwrap_or_default().to_string())
            .filter(|name| !name.is_empty())
            .collect()
    }

    fn after_generics(text: &str) -> &str {
        let Some(mut rest) = text.strip_prefix('<') else { return text };
        let mut depth = 1;
        while depth > 0 {
            let at = rest.find(['<', '>']).expect("a closing >");
            depth += if rest.as_bytes()[at] == b'<' { 1 } else { -1 };
            rest = &rest[at + 1..];
        }
        rest
    }

    fn invoked_commands(source: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for callee in ["invoke", "call"] {
            for (at, _) in source.match_indices(callee) {
                let standalone = !source[..at]
                    .ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                let args = after_generics(&source[at + callee.len()..]);
                if let Some(name) = args.strip_prefix("(\"").filter(|_| standalone) {
                    let name = &name[..name.find('"').expect("a closing quote")];
                    if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                        names.insert(name.to_string());
                    }
                }
            }
        }
        names
    }

    fn app_sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("an app source dir") {
            let path = entry.expect("app entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let script = path.extension().is_some_and(|ext| ext == "ts" || ext == "tsx");
            if path.is_dir() {
                app_sources(&path, found);
            } else if script && !name.contains(".test.") {
                found.push(path);
            }
        }
    }

    #[test]
    fn the_command_detectors_read_paths_generics_and_callees() {
        let handlers =
            "x.invoke_handler(tauri::generate_handler![\n  a::set_proxy,\n  open_route\n]);";
        assert_eq!(
            handler_names(handlers),
            BTreeSet::from(["set_proxy".into(), "open_route".into()])
        );
        let app = r#"invoke<Record<string, string>>("interface_names"); deps.invoke<{ a: 1 }>("proxy_status");
            call<boolean>("notice_dismissed"); go({ t: "re-invoke" }); recall("not_one"); invoke("Title")"#;
        assert_eq!(
            invoked_commands(app),
            BTreeSet::from([
                "interface_names".into(),
                "proxy_status".into(),
                "notice_dismissed".into()
            ])
        );
    }

    // The e2e and the walkthrough run the scenario build's list and the app ships
    // the real one, so a command missing from either fails only in the other.
    #[test]
    fn both_shell_builds_register_exactly_the_commands_the_app_invokes() {
        let repo = crate::repo_root().expect("repo root");
        let shell = |file: &str| {
            let path = repo.join("ui/src-tauri/src").join(file);
            handler_names(&std::fs::read_to_string(&path).expect("a shell source"))
        };
        let real = shell("lib.rs");
        let mut scenario = shell("scenario.rs");
        assert!(scenario.remove("scenario_script"), "the scenario build registers scenario_script");
        assert_eq!(scenario, real, "the scenario build's commands differ from the real build's");

        let mut sources = Vec::new();
        app_sources(&repo.join("ui/src"), &mut sources);
        let invoked: BTreeSet<String> = sources
            .iter()
            .flat_map(|path| invoked_commands(&std::fs::read_to_string(path).expect("app source")))
            .collect();
        assert_eq!(invoked, real, "the app invokes other commands than the shell registers");
    }

    // The shell opens the check request and the app finds its row by host and
    // query key, so the two must name the same URL or the task never settles.
    #[test]
    fn the_check_url_the_shell_opens_is_the_one_the_app_reads() {
        let repo = crate::repo_root().expect("repo root");
        let read = |path: &str| std::fs::read_to_string(repo.join(path)).expect(path);
        let route = read("ui/src-tauri/src/route.rs");
        let app = read("ui/src/ownRequest.ts");
        assert_eq!(
            literal_after(&route, "const REQUEST_URL: &str = \""),
            format!(
                "https://{}/?{}=",
                literal_after(&app, "export const REQUEST_HOST = \""),
                literal_after(&app, "const MARK = \"")
            )
        );
    }

    fn block_after<'a>(source: &'a str, opener: &str, closer: &str) -> &'a str {
        let start = source.find(opener).unwrap_or_else(|| panic!("no {opener}")) + opener.len();
        let rest = &source[start..];
        &rest[..rest.find(closer).unwrap_or_else(|| panic!("no {closer} after {opener}"))]
    }

    fn kebab(variant: &str) -> String {
        variant.chars().enumerate().fold(String::new(), |mut out, (at, c)| {
            if c.is_ascii_uppercase() && at > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            out
        })
    }

    // A route or a mark the shell refuses opens nothing, and the step's button
    // swallows the refusal, so a drift between the two sides is a dead click.
    #[test]
    fn the_shell_opens_every_route_the_app_names_with_the_marks_the_app_makes() {
        let repo = crate::repo_root().expect("repo root");
        let read = |path: &str| std::fs::read_to_string(repo.join(path)).expect(path);
        let route = read("ui/src-tauri/src/route.rs");
        let setup = read("ui/src/setup.ts");
        let marks = read("ui/src/ownRequest.ts");

        let shell_routes: BTreeSet<String> = block_after(&route, "pub enum Route {", "\n}")
            .lines()
            .filter_map(|line| line.trim().split([' ', ',', '{']).next())
            .filter(|name| !name.is_empty())
            .map(kebab)
            .collect();
        let app_routes: BTreeSet<String> = block_after(&setup, "export type RouteName =", ";")
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect();
        assert_eq!(app_routes, shell_routes, "RouteName and the shell's Route differ");

        assert!(
            route.contains("c.is_ascii_lowercase() || c.is_ascii_digit()"),
            "valid_mark no longer reads a lowercase base-36 window"
        );
        assert!(marks.contains("Math.random().toString(36)"), "the window mark is not base 36");
        assert!(marks.contains("`${WINDOW_MARK}.${sent}`"), "the mark is not <window>.<n>");
        let width: usize = block_after(&marks, ".toString(36).padStart(", ",")
            .parse()
            .expect("deviceMark's width");
        let longest: usize =
            block_after(&route, "(1..=", ")").parse().expect("valid_device_mark's longest");
        assert!(width <= longest, "deviceMark makes {width} characters, the shell takes {longest}");
    }

    // pnpm 10+ ignores the `pnpm` field and says so only as an install warning,
    // so a pin placed there stops applying without turning anything red.
    fn carries_a_pnpm_field(package_json: &str) -> bool {
        let parsed: serde_yaml::Value = serde_yaml::from_str(package_json).expect("valid json");
        parsed.get("pnpm").is_some()
    }

    #[test]
    fn the_pnpm_field_detector_reads_the_top_level_key() {
        assert!(carries_a_pnpm_field(r#"{"name":"a","pnpm":{"overrides":{}}}"#));
        assert!(!carries_a_pnpm_field(r#"{"name":"a","scripts":{"pnpm":"x"}}"#));
    }

    #[test]
    fn no_tracked_package_json_carries_a_pnpm_field() {
        let repo = crate::repo_root().expect("repo root");
        let listed = std::process::Command::new("git")
            .args(["ls-files", "*package.json"])
            .current_dir(&repo)
            .output()
            .expect("git ls-files");
        let listed = String::from_utf8(listed.stdout).expect("utf-8 paths");
        assert!(
            listed.lines().count() >= 2,
            "expected ui/ and ui/e2e/ package.json, got {listed:?}"
        );
        for path in listed.lines() {
            let source = std::fs::read_to_string(repo.join(path)).expect("package.json");
            assert!(
                !carries_a_pnpm_field(&source),
                "{path} carries a pnpm field, which pnpm ignores"
            );
        }
    }

    #[test]
    fn the_e2e_workspace_pins_the_tauri_service_exactly() {
        let repo = crate::repo_root().expect("repo root");
        let manifest =
            std::fs::read_to_string(repo.join("ui/e2e/package.json")).expect("ui/e2e/package.json");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&manifest).expect("valid json");
        let pinned = parsed["devDependencies"]["@wdio/tauri-service"].as_str().unwrap_or("");
        assert!(
            pinned.split('.').count() == 3 && pinned.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "ui/e2e keeps no lockfile, so a range lets the service float onto native-utils it was not built against: {pinned:?}"
        );
        let workspace = std::fs::read_to_string(repo.join("ui/e2e/pnpm-workspace.yaml"))
            .expect("ui/e2e/pnpm-workspace.yaml");
        let workspace: serde_yaml::Value = serde_yaml::from_str(&workspace).expect("valid yaml");
        assert!(
            workspace["overrides"]["@wdio/native-utils"].is_null(),
            "the service pins its own native-utils; an override pairs it with one it was not built against"
        );
    }

    #[test]
    fn unknown_ui_subcommand_is_usage_error() {
        let err = run(&["frobnicate"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
    }

    #[test]
    fn bench_takes_no_arguments_and_is_documented() {
        let err = run(&["bench", "--forever"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
        assert!(super::super::USAGE.contains("cargo xtask ui bench"));
    }

    #[test]
    fn walkthrough_takes_no_arguments_and_is_documented() {
        let err = run(&["walkthrough", "--only", "macos-this"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
        assert!(super::super::USAGE.contains("cargo xtask ui walkthrough"));
    }

    #[test]
    fn specimens_rejects_an_unknown_flag() {
        let err = run(&["specimens", "--rebuild-everything"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
    }

    fn window_of(repo: &Path, config: &str) -> serde_yaml::Value {
        let source = std::fs::read_to_string(repo.join("ui/src-tauri").join(config)).expect(config);
        // JSON is a YAML subset, so the reader already in the tree parses it.
        let parsed: serde_yaml::Value = serde_yaml::from_str(&source).expect("valid json");
        parsed["app"]["windows"][0].clone()
    }

    // The flow list's header holds one line down to 846px once the capture acts
    // sit behind one trigger (E-EXPORT.W9, Tune T8), so a window the user can
    // drag below that reaches a width where the header wraps to two lines. A
    // merged config replaces the windows array whole, so each overlay that
    // declares the window must keep the minimum too.
    #[test]
    fn the_window_cannot_be_dragged_narrower_than_the_header_holds_one_line() {
        let repo = crate::repo_root().expect("repo root");
        for config in ["tauri.conf.json", "tauri.conf.e2e.json", "tauri.conf.walkthrough.json"] {
            let min_width = window_of(&repo, config)["minWidth"].as_u64().expect("minWidth");
            assert!(
                min_width >= 850,
                "{config}: minWidth is {min_width}, and the header wraps below 846"
            );
        }
    }

    // A config that lists capabilities enables only those, so a capability the
    // app ships and the e2e config omits leaves its windows unable to invoke.
    #[test]
    fn the_e2e_build_enables_every_capability_the_app_ships() {
        let shell = crate::repo_root().expect("repo root").join("ui/src-tauri");
        let config =
            std::fs::read_to_string(shell.join("tauri.conf.e2e.json")).expect("e2e config");
        let config: serde_yaml::Value = serde_yaml::from_str(&config).expect("valid json");
        let enabled: BTreeSet<String> = config["app"]["security"]["capabilities"]
            .as_sequence()
            .expect("the e2e config lists its capabilities")
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect();
        for entry in std::fs::read_dir(shell.join("capabilities")).expect("capabilities dir") {
            let path = entry.expect("capability entry").path();
            let source = std::fs::read_to_string(&path).expect("a capability");
            let capability: serde_yaml::Value = serde_yaml::from_str(&source).expect("valid json");
            let identifier = capability["identifier"].as_str().expect("an identifier");
            assert!(
                enabled.contains(identifier),
                "{} is not enabled in the e2e config",
                path.display()
            );
        }
    }

    // WebDriver reaches only the windows the wdio capability names, so a window
    // family the app opens and that capability omits cannot be driven by a spec.
    #[test]
    fn the_e2e_driver_reaches_every_window_the_app_opens() {
        let shell = crate::repo_root().expect("repo root").join("ui/src-tauri");
        let config =
            std::fs::read_to_string(shell.join("tauri.conf.e2e.json")).expect("e2e config");
        let config: serde_yaml::Value = serde_yaml::from_str(&config).expect("valid json");
        let driven: BTreeSet<String> = config["app"]["security"]["capabilities"]
            .as_sequence()
            .expect("the e2e config lists its capabilities")
            .iter()
            .find(|entry| entry["identifier"].as_str() == Some("wdio"))
            .expect("an inline wdio capability")["windows"]
            .as_sequence()
            .expect("the wdio capability names its windows")
            .iter()
            .filter_map(|window| window.as_str().map(str::to_string))
            .collect();
        for entry in std::fs::read_dir(shell.join("capabilities")).expect("capabilities dir") {
            let path = entry.expect("capability entry").path();
            let source = std::fs::read_to_string(&path).expect("a capability");
            let capability: serde_yaml::Value = serde_yaml::from_str(&source).expect("valid json");
            for window in capability["windows"].as_sequence().expect("windows") {
                let window = window.as_str().expect("a window label");
                assert!(driven.contains(window), "{}: wdio cannot drive {window}", path.display());
            }
        }
    }

    // Tauri builds config windows before the setup hook, and the shell may open
    // its window only once it knows its core's address (E-ONEAPP.W2).
    #[test]
    fn every_config_leaves_the_window_to_the_setup_hook() {
        let repo = crate::repo_root().expect("repo root");
        for config in ["tauri.conf.json", "tauri.conf.e2e.json", "tauri.conf.walkthrough.json"] {
            assert_eq!(
                window_of(&repo, config)["create"].as_bool(),
                Some(false),
                "{config}: the window must declare \"create\": false"
            );
        }
    }

    // The e2e asserts no picture, so its window stays off the screen. The
    // walkthrough films what a user sees, and a hidden page loads no recording.
    #[test]
    fn the_e2e_window_is_hidden_and_the_walkthrough_window_is_shown() {
        let repo = crate::repo_root().expect("repo root");
        let visible = |config| window_of(&repo, config)["visible"].as_bool().unwrap_or(true);
        assert!(!visible("tauri.conf.e2e.json"));
        assert!(visible("tauri.conf.walkthrough.json"));
    }
}
