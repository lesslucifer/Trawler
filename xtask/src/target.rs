use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::git::git;

pub const ISOLATE: &str = "cargo xtask target isolate";
const DIR: &str = "target";
const ARTIFACT_DIRS: [&str; 3] = ["deps", "build", ".fingerprint"];
const CLONE_BATCH: usize = 512;

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    match args {
        ["seed"] => seed_command(repo),
        ["isolate"] => isolate(repo),
        ["isolate", checkout] => isolate(Path::new(checkout)),
        ["prune"] => prune(repo),
        _ => Err(crate::USAGE.to_string()),
    }
}

/// Why a gate must not run in this checkout, or `None`.
pub fn refusal(repo: &Path) -> Option<String> {
    let link = fs::read_link(repo.join(DIR)).ok()?;
    Some(format!(
        "target REFUSED - {DIR} is a symlink to {}, so this checkout can build against and run another checkout's workspace crates\n      fix -> {ISOLATE}",
        link.display()
    ))
}

/// The crate and package names a checkout builds itself, normalised to `_`.
#[derive(Debug, Default)]
pub struct Workspace(BTreeSet<String>);

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Target {
    name: String,
}

impl Workspace {
    pub fn of(checkout: &Path) -> Result<Self, String> {
        let out = Command::new("cargo")
            .args(["metadata", "--no-deps", "--offline", "--format-version", "1"])
            .current_dir(checkout)
            .output()
            .map_err(|e| format!("failed to spawn cargo metadata: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "cargo metadata in {} failed: {}",
                checkout.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let metadata: Metadata = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("cargo metadata output: {e}"))?;
        Ok(Self::named(metadata.packages.iter().flat_map(|package| {
            std::iter::once(package.name.as_str())
                .chain(package.targets.iter().map(|t| t.name.as_str()))
        })))
    }

    pub fn named<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self(names.into_iter().map(normalised).collect())
    }

    /// Whether a `deps/`, `build/` or `.fingerprint/` entry was produced from this
    /// checkout's own sources.
    pub fn owns(&self, entry: &str) -> bool {
        let stem = normalised(crate_stem(entry));
        self.0.contains(&stem) || stem.strip_prefix("lib").is_some_and(|rest| self.0.contains(rest))
    }
}

fn normalised(name: &str) -> String {
    name.replace('-', "_")
}

// Cargo names an entry `<name>-<16 hex>[.ext…]`, except codegen-unit leftovers,
// which are `<name>.<cgu>.rcgu.o`. Package names may hold `-`, so the cut is at
// the hash, not at the first `-`.
fn crate_stem(entry: &str) -> &str {
    for (at, _) in entry.match_indices('-') {
        let rest = &entry[at + 1..];
        let hashed = rest.len() >= 16
            && rest.as_bytes()[..16].iter().all(u8::is_ascii_hexdigit)
            && rest[16..].chars().next().is_none_or(|c| c == '.');
        if hashed {
            return &entry[..at];
        }
    }
    entry.split('.').next().unwrap_or(entry)
}

/// Each cargo profile directory under `target`, relative to it:
/// `debug`, `release`, `<triple>/debug`, …
pub fn profiles(target: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for child in subdirs(target) {
        if target.join(&child).join(".fingerprint").is_dir() {
            found.push(PathBuf::from(&child));
            continue;
        }
        for grandchild in subdirs(&target.join(&child)) {
            if target.join(&child).join(&grandchild).join(".fingerprint").is_dir() {
                found.push(Path::new(&child).join(grandchild));
            }
        }
    }
    found.sort();
    found
}

fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn names_in(dir: &Path) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Seeded {
    Cloned { entries: usize, profiles: usize },
    Empty(String),
}

/// Fills an absent or empty `dest` with `source`'s third-party artifacts only.
pub fn seed(source: &Path, dest: &Path, own: &Workspace) -> Result<Seeded, String> {
    fs::create_dir_all(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    if !cfg!(target_os = "macos") {
        return Ok(Seeded::Empty("copy-on-write clones are macOS-only here".to_string()));
    }
    if !source.is_dir() {
        return Ok(Seeded::Empty(format!("{} does not exist", source.display())));
    }
    let profiles = profiles(source);
    let mut entries = 0;
    for profile in &profiles {
        for dir in ARTIFACT_DIRS {
            let from = source.join(profile).join(dir);
            if !from.is_dir() {
                continue;
            }
            let kept: Vec<PathBuf> = names_in(&from)?
                .into_iter()
                .filter(|name| !own.owns(name))
                .map(|name| from.join(name))
                .collect();
            let into = dest.join(profile).join(dir);
            fs::create_dir_all(&into).map_err(|e| format!("create {}: {e}", into.display()))?;
            for batch in kept.chunks(CLONE_BATCH) {
                if let Err(why) = clone_into(batch, &into) {
                    empty(dest)?;
                    return Ok(Seeded::Empty(why));
                }
            }
            entries += kept.len();
        }
    }
    Ok(Seeded::Cloned { entries, profiles: profiles.len() })
}

fn clone_into(sources: &[PathBuf], into: &Path) -> Result<(), String> {
    let out = Command::new("cp")
        .args(["-c", "-p", "-R"])
        .args(sources)
        .arg(into)
        .output()
        .map_err(|e| format!("failed to spawn cp: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("cp -c (clone) failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn empty(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))
}

fn main_checkout(repo: &Path) -> Result<PathBuf, String> {
    let listing = git(repo, &["worktree", "list", "--porcelain"])?;
    listing
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .ok_or_else(|| "git worktree list named no checkout".to_string())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn seed_command(repo: &Path) -> Result<(i32, String), String> {
    let dest = repo.join(DIR);
    let occupied = match fs::symlink_metadata(&dest) {
        Ok(meta) => {
            meta.file_type().is_symlink()
                || fs::read_dir(&dest).map_or(true, |mut e| e.next().is_some())
        }
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(e) => return Err(format!("stat {}: {e}", dest.display())),
    };
    if occupied {
        return Ok((
            1,
            format!(
                "target seed refused - {} already exists; a symlink is replaced by {ISOLATE}",
                dest.display()
            ),
        ));
    }
    seed_checkout(repo)
}

fn seed_checkout(checkout: &Path) -> Result<(i32, String), String> {
    let main = main_checkout(checkout)?;
    if same_dir(&main, checkout) {
        return Ok((
            1,
            "target seed refused - this is the main checkout, whose target is the seed source"
                .to_string(),
        ));
    }
    let own = Workspace::of(checkout)?;
    let (source, dest) = (main.join(DIR), checkout.join(DIR));
    Ok((
        0,
        match seed(&source, &dest, &own)? {
            Seeded::Cloned { entries, profiles } => format!(
                "target seeded - {entries} third-party entries cloned from {} across {profiles} profile(s); the first build compiles only this checkout's workspace crates",
                source.display()
            ),
            Seeded::Empty(why) => {
                format!("target created empty - {why}; the first build compiles every crate")
            }
        },
    ))
}

fn isolate(checkout: &Path) -> Result<(i32, String), String> {
    let target = checkout.join(DIR);
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Workspace::of(checkout)?;
            fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", target.display()))?;
            let (code, said) = seed_checkout(checkout)?;
            Ok((code, format!("{} no longer a symlink\n{said}", target.display())))
        }
        Ok(_) => Ok((0, format!("target already isolated - {} is a directory", target.display()))),
        Err(e) if e.kind() == ErrorKind::NotFound => seed_checkout(checkout),
        Err(e) => Err(format!("stat {}: {e}", target.display())),
    }
}

/// What [`prune_dir`] removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub entries: usize,
    pub bytes: u64,
}

/// Removes everything `own` built, plus `incremental/`, `examples/` and the
/// uplifted files, from each profile under `target`.
pub fn prune_dir(target: &Path, own: &Workspace) -> Result<Pruned, String> {
    let mut pruned = Pruned::default();
    for profile in profiles(target) {
        let profile = target.join(profile);
        let mut doomed: Vec<PathBuf> = Vec::new();
        for dir in ARTIFACT_DIRS {
            let dir = profile.join(dir);
            if dir.is_dir() {
                doomed.extend(
                    names_in(&dir)?.into_iter().filter(|n| own.owns(n)).map(|n| dir.join(n)),
                );
            }
        }
        for name in names_in(&profile)? {
            let path = profile.join(&name);
            let uplifted = fs::symlink_metadata(&path).is_ok_and(|m| !m.is_dir());
            if uplifted || name == "incremental" || name == "examples" {
                doomed.push(path);
            }
        }
        for path in doomed {
            pruned.bytes += size_of(&path);
            remove(&path)?;
            pruned.entries += 1;
        }
    }
    Ok(pruned)
}

fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path).map_or(0, |entries| entries.flatten().map(|e| size_of(&e.path())).sum())
}

const REMOVE_PASSES: usize = 8;

// macOS readdir can skip entries of a large directory while they are being
// unlinked, so one remove_dir_all pass ends in ENOTEMPTY with files left behind
// (met pruning a 13k-entry incremental/). Each further pass sees what the last
// one skipped.
fn remove(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("stat {}: {e}", path.display()))?;
    if !meta.is_dir() {
        return fs::remove_file(path).map_err(|e| format!("remove {}: {e}", path.display()));
    }
    let mut pass = 1;
    loop {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == ErrorKind::DirectoryNotEmpty && pass < REMOVE_PASSES => pass += 1,
            Err(e) => return Err(format!("remove {} (pass {pass}): {e}", path.display())),
        }
    }
}

fn free_kib(path: &Path) -> Option<u64> {
    let out = Command::new("df").arg("-k").arg(path).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()
}

fn gib(kib: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let gib = kib as f64 / (1024.0 * 1024.0);
    format!("{gib:.1} GiB")
}

fn prune(repo: &Path) -> Result<(i32, String), String> {
    if let Some(refusal) = refusal(repo) {
        return Ok((1, refusal));
    }
    let target = repo.join(DIR);
    let own = Workspace::of(repo)?;
    let before = free_kib(repo);
    let pruned = prune_dir(&target, &own)?;
    let folded = crate::tally::fold_legacy(repo)?;
    let after = free_kib(repo);
    let mut said = format!(
        "target pruned - {} entries, {} of workspace-crate artifacts, incremental/, examples/ and uplifted files removed from {}",
        pruned.entries,
        gib(pruned.bytes / 1024),
        target.display()
    );
    if let (Some(before), Some(after)) = (before, after) {
        let _ = write!(said, "\n      volume free: {} -> {}", gib(before), gib(after));
    }
    if let Some(records) = folded {
        let _ = write!(
            said,
            "\n      gate tally: {records} record(s) moved to {}",
            crate::tally::path(repo).display()
        );
    }
    Ok((0, said))
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::{
        crate_stem, profiles, prune_dir, refusal, seed, Pruned, Seeded, Workspace, ISOLATE,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    const H: &str = "0123456789abcdef";

    fn own() -> Workspace {
        Workspace::named(["trawler", "trawler-ui", "trawler_ui_lib", "xtask", "e2e_h2"])
    }

    fn file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    fn built_profile(target: &Path, profile: &str) {
        let p = target.join(profile);
        for name in [
            format!("libtrawler-{H}.rlib"),
            format!("xtask-{H}"),
            format!("e2e_h2-{H}.e2e_h2.1a2b-cgu.0.rcgu.o"),
            "trawler_ui_lib.9gd7j68ua49mmz25.13qj6ax.rcgu.o".to_string(),
            format!("libserde-{H}.rlib"),
            format!("libc-{H}.rmeta"),
            format!("serde-{H}.serde.0a1b-cgu.0.rcgu.o"),
        ] {
            file(&p.join("deps").join(name));
        }
        file(&p.join(".fingerprint").join(format!("trawler-ui-{H}")).join("lib-trawler_ui_lib"));
        file(&p.join(".fingerprint").join(format!("serde-{H}")).join("lib-serde"));
        file(&p.join("build").join(format!("trawler-ui-{H}")).join("output"));
        file(&p.join("build").join(format!("ring-{H}")).join("output"));
        file(&p.join("incremental").join(format!("trawler-{H}")).join("s-1"));
        file(&p.join("examples").join(format!("demo-{H}")));
        file(&p.join("xtask"));
        file(&p.join("libxtask.rlib"));
        file(&p.join(".cargo-lock"));
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    fn third_party(profile: &Path) -> (Vec<String>, Vec<String>, Vec<String>) {
        (
            names(&profile.join("deps")),
            names(&profile.join(".fingerprint")),
            names(&profile.join("build")),
        )
    }

    fn expected_third_party() -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut deps = vec![
            format!("libc-{H}.rmeta"),
            format!("libserde-{H}.rlib"),
            format!("serde-{H}.serde.0a1b-cgu.0.rcgu.o"),
        ];
        deps.sort();
        (deps, vec![format!("serde-{H}")], vec![format!("ring-{H}")])
    }

    #[test]
    fn the_crate_stem_is_cut_at_the_hash_or_the_first_dot() {
        assert_eq!(crate_stem(&format!("libtrawler-{H}.rlib")), "libtrawler");
        assert_eq!(crate_stem(&format!("xtask-{H}")), "xtask");
        assert_eq!(crate_stem(&format!("xtask-{H}.d")), "xtask");
        assert_eq!(crate_stem(&format!("trawler-ui-{H}")), "trawler-ui");
        assert_eq!(crate_stem(&format!("e2e_h2-{H}.e2e_h2.1a2b-cgu.0.rcgu.o")), "e2e_h2");
        assert_eq!(crate_stem("trawler_ui_lib.9gd7j68ua49mmz25.13qj6ax.rcgu.o"), "trawler_ui_lib");
        assert_eq!(crate_stem("build-script-build"), "build-script-build");
    }

    #[test]
    fn ownership_normalises_hyphens_and_strips_only_a_lib_prefix() {
        let own = own();
        assert!(own.owns(&format!("trawler-ui-{H}")));
        assert!(own.owns(&format!("libtrawler_ui_lib-{H}.rlib")));
        assert!(own.owns(&format!("libtrawler-{H}.rlib")));
        assert!(!own.owns(&format!("libc-{H}.rlib")));
        assert!(!own.owns(&format!("trawler_extra-{H}.rlib")));
        assert!(Workspace::named(["c"]).owns(&format!("libc-{H}.rlib")));
    }

    #[test]
    fn profiles_include_target_triple_profiles_and_skip_other_dirs() {
        let dir = TempDir::new().unwrap();
        built_profile(dir.path(), "debug");
        built_profile(dir.path(), "aarch64-pc-windows-msvc/release");
        file(&dir.path().join("walkthrough/index.html"));
        file(&dir.path().join("doc/trawler/index.html"));
        assert_eq!(
            profiles(dir.path()),
            vec![PathBuf::from("aarch64-pc-windows-msvc/release"), PathBuf::from("debug")]
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_seed_holds_only_third_party_artifacts() {
        let dir = TempDir::new().unwrap();
        let (source, dest) = (dir.path().join("main/target"), dir.path().join("wt/target"));
        built_profile(&source, "debug");
        built_profile(&source, "aarch64-pc-windows-msvc/debug");
        file(&source.join(".gate-tally"));
        file(&source.join(".lint-green"));
        file(&source.join("walkthrough/index.html"));

        let seeded = seed(&source, &dest, &own()).unwrap();

        assert_eq!(seeded, Seeded::Cloned { entries: 10, profiles: 2 });
        assert_eq!(names(&dest), vec!["aarch64-pc-windows-msvc".to_string(), "debug".to_string()]);
        let debug = dest.join("debug");
        assert_eq!(names(&debug), vec![".fingerprint", "build", "deps"]);
        assert_eq!(third_party(&debug), expected_third_party());
        assert_eq!(
            fs::read_to_string(debug.join(".fingerprint").join(format!("serde-{H}/lib-serde")))
                .unwrap(),
            "x"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn a_seed_keeps_the_source_mtimes() {
        let dir = TempDir::new().unwrap();
        let (source, dest) = (dir.path().join("main/target"), dir.path().join("wt/target"));
        built_profile(&source, "debug");
        let rlib = format!("debug/deps/libserde-{H}.rlib");
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        fs::File::options()
            .write(true)
            .open(source.join(&rlib))
            .unwrap()
            .set_modified(old)
            .unwrap();

        seed(&source, &dest, &own()).unwrap();

        assert_eq!(fs::metadata(dest.join(&rlib)).unwrap().modified().unwrap(), old);
    }

    #[test]
    fn a_missing_source_seeds_an_empty_target() {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("wt/target");
        let seeded = seed(&dir.path().join("absent"), &dest, &own()).unwrap();
        assert!(matches!(seeded, Seeded::Empty(_)), "{seeded:?}");
        assert!(dest.is_dir());
        assert!(names(&dest).is_empty());
    }

    #[test]
    fn prune_removes_what_the_workspace_built_and_keeps_third_party() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target");
        built_profile(&target, "debug");
        file(&target.join(".gate0-green"));

        let pruned = prune_dir(&target, &own()).unwrap();

        let debug = target.join("debug");
        assert_eq!(names(&debug), vec![".fingerprint", "build", "deps"]);
        assert_eq!(third_party(&debug), expected_third_party());
        assert_eq!(pruned, Pruned { entries: 11, bytes: 11 });
        assert!(target.join(".gate0-green").exists(), "prune leaves the target root alone");
    }

    #[test]
    fn a_symlinked_target_is_refused_with_the_isolate_line() {
        let dir = TempDir::new().unwrap();
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        let checkout = dir.path().join("wt");
        fs::create_dir(&checkout).unwrap();
        assert_eq!(refusal(&checkout), None, "no target at all is not refused");
        std::os::unix::fs::symlink(&shared, checkout.join("target")).unwrap();
        let said = refusal(&checkout).expect("a symlink must refuse");
        assert!(said.contains(ISOLATE) && said.contains("shared"), "{said}");
    }

    #[test]
    fn a_target_directory_is_not_refused() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        assert_eq!(refusal(dir.path()), None);
    }
}
