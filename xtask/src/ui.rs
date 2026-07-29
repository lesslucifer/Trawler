use std::path::Path;
use std::process::Command;

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let dir = repo.join("ui");
    match args {
        ["build"] => {
            pnpm(&dir, &["install"], &[])?;
            pnpm(&dir, &["build"], &[])?;
            Ok((0, "ui build complete - bundle in ui/dist/".to_string()))
        }
        ["dev"] => {
            pnpm(&dir, &["install"], &[])?;
            pnpm(&dir, &["tauri", "dev"], &[])?;
            Ok((0, String::new()))
        }
        ["e2e"] => e2e(&dir),
        ["specimens"] => specimens(repo, "build-specimens.mjs"),
        ["specimens", "--check"] => specimens(repo, "check-render.mjs"),
        _ => Err(super::USAGE.to_string()),
    }
}

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
    pnpm(&e2e_dir, &["test"], &[])?;
    Ok((0, "ui e2e complete".to_string()))
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
    use super::run;
    use std::path::Path;

    #[test]
    fn unknown_ui_subcommand_is_usage_error() {
        let err = run(&["frobnicate"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
    }

    #[test]
    fn specimens_rejects_an_unknown_flag() {
        let err = run(&["specimens", "--rebuild-everything"], Path::new(".")).unwrap_err();
        assert!(err.contains("usage:"));
    }
}
