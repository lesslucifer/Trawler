use std::path::Path;
use std::process::Command;

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let dir = repo.join("ui");
    match args {
        ["build"] => {
            pnpm(&dir, &["install"])?;
            pnpm(&dir, &["build"])?;
            Ok((0, "ui build complete - bundle in ui/dist/".to_string()))
        }
        ["dev"] => {
            pnpm(&dir, &["install"])?;
            pnpm(&dir, &["tauri", "dev"])?;
            Ok((0, String::new()))
        }
        _ => Err(super::USAGE.to_string()),
    }
}

fn pnpm(dir: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new("pnpm")
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("failed to launch pnpm (is it installed?): {e}"))?;
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
}
