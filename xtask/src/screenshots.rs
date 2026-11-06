use super::walkthrough::{pnpm, required};
use std::path::{Path, PathBuf};

pub const OUT: &str = ".github/screenshots";

pub const LAUNCHES: [(&str, &[&str]); 2] = [
    (
        "attached",
        &[
            "s1-main-window",
            "s2-setup-this-machine",
            "s3-filter-chips",
            "s4-inspector-headers",
            "s5-send-again",
            "s6-setup-iphone",
            "s10-capture-menu",
            "s11-settings-general",
        ],
    ),
    ("owned", &["s7-settings-capture-service"]),
];

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub fn missing_shots(out: &Path, shots: &[&str]) -> Vec<String> {
    shots
        .iter()
        .filter(|shot| {
            std::fs::read(out.join(format!("{shot}.png")))
                .map_or(true, |bytes| !bytes.starts_with(PNG_SIGNATURE))
        })
        .map(|shot| format!("{shot}.png"))
        .collect()
}

// The committed shots sit where the run writes, so a shot the spec skipped
// would otherwise pass on last run's file.
pub fn clear_shots(out: &Path, shots: &[&str]) -> Result<(), String> {
    for shot in shots {
        let path = out.join(format!("{shot}.png"));
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("{}: {e}", path.display()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set".to_string())
}

// The app keeps its CA, layout and answers under HOME, so each launch gets a
// HOME it deletes; cargo and rustup still need the user's own.
fn launch(e2e: &Path, out: &Path, name: &str) -> Result<(), String> {
    let real = home_dir()?;
    let home = tempfile::tempdir().map_err(|e| format!("a temp HOME: {e}"))?;
    let tool_home =
        |var: &str, dir: &str| std::env::var_os(var).unwrap_or_else(|| real.join(dir).into());
    let home = home.path().to_string_lossy();
    let cargo_home = tool_home("CARGO_HOME", ".cargo");
    let rustup_home = tool_home("RUSTUP_HOME", ".rustup");
    let out = out.to_string_lossy();
    let env = [
        ("HOME", &*home),
        ("CARGO_HOME", &*cargo_home.to_string_lossy()),
        ("RUSTUP_HOME", &*rustup_home.to_string_lossy()),
        ("TRAWLER_SCREENSHOTS_LAUNCH", name),
        ("TRAWLER_SCREENSHOTS_OUT", &*out),
    ];
    let conf = ["exec", "wdio", "run", "screenshots/screenshots.conf.ts"];
    required(pnpm(e2e, &conf, &env), &format!("the {name} screenshot launch"))
}

pub fn run(ui: &Path, repo: &Path) -> Result<(i32, String), String> {
    let e2e = ui.join("e2e");
    required(pnpm(&e2e, &["install"], &[]), "pnpm install in ui/e2e")?;
    super::walkthrough::scenario_build(ui)?;
    let out = repo.join(OUT);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut missing = Vec::new();
    for (name, shots) in LAUNCHES {
        clear_shots(&out, shots)?;
        launch(&e2e, &out, name)?;
        missing.extend(missing_shots(&out, shots));
    }
    if missing.is_empty() {
        Ok((0, format!("ui screenshots complete - {OUT}/")))
    } else {
        Err(format!("no shot written for {}", missing.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_starts_without_the_shots_it_takes_and_keeps_the_hand_captured_ones() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.png", "s8-hand.png"] {
            std::fs::write(dir.path().join(name), PNG_SIGNATURE).unwrap();
        }
        clear_shots(dir.path(), &["a", "never-taken"]).unwrap();
        assert_eq!(missing_shots(dir.path(), &["a"]), ["a.png"]);
        assert!(dir.path().join("s8-hand.png").exists());
    }

    #[test]
    fn a_shot_is_taken_only_when_its_png_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), [PNG_SIGNATURE, b"rest"].concat()).unwrap();
        std::fs::write(dir.path().join("b.png"), "not a png").unwrap();
        assert_eq!(missing_shots(dir.path(), &["a", "b", "c"]), ["b.png", "c.png"]);
    }

    #[test]
    fn every_shot_the_script_takes_is_one_this_run_waits_for() {
        let spec = std::fs::read_to_string(
            crate::repo_root().unwrap().join("ui/e2e/screenshots/screenshots.spec.ts"),
        )
        .unwrap();
        let taken: Vec<&str> =
            spec.split("shoot(\"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
        let mut listed: Vec<&str> =
            LAUNCHES.iter().flat_map(|(_, shots)| shots.iter().copied()).collect();
        listed.sort_unstable();
        let mut taken_sorted = taken.clone();
        taken_sorted.sort_unstable();
        assert_eq!(taken_sorted, listed);
    }
}
