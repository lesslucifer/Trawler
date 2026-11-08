use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use xtask::site;

const FIXTURE: &str = "xtask/tests/fixtures/site/release-v0.1.0.json";

fn repo() -> PathBuf {
    xtask::repo_root().unwrap()
}

fn build_into(release: &Path, out: &Path) -> Result<(i32, String), String> {
    let args = ["build", "--release", release.to_str().unwrap(), "--out", out.to_str().unwrap()];
    site::run(&args, &repo())
}

fn written(out: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|entry| {
            assert!(entry.path().is_file(), "{} is a folder", entry.path().display());
            (entry.file_name().to_string_lossy().into_owned(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}

#[test]
fn the_v0_1_0_fixture_builds_the_frame_and_the_listed_assets_and_nothing_else() {
    let out = tempfile::tempdir().unwrap();
    let out = out.path().join("site");
    let (code, said) = build_into(&repo().join(FIXTURE), &out).unwrap();
    assert_eq!(code, 0, "{said}");
    let files = written(&out);
    let mut expected: Vec<String> = std::fs::read_dir(repo().join(site::FRAME))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .chain(site::ASSETS.iter().map(|(_, to)| (*to).to_string()))
        .collect();
    expected.sort();
    assert_eq!(files.keys().cloned().collect::<Vec<_>>(), expected);
    for page in ["index.html", "404.html", "site.css"] {
        assert!(files.contains_key(page), "no {page}");
    }
    assert!(!files.keys().any(|name| name.contains("docs")), "{:?}", files.keys());
    let repo = repo();
    let release: site::Release =
        serde_json::from_str(&std::fs::read_to_string(repo.join(FIXTURE)).unwrap()).unwrap();
    let facts = site::Facts::of(&repo, site::downloads(&release, "0.1.0").unwrap()).unwrap();
    assert_eq!(site::problems(&files, &facts), Vec::<String>::new());
}

#[test]
fn the_built_page_links_every_installer_kind_at_v0_1_0_and_names_the_readme_pitch() {
    let out = tempfile::tempdir().unwrap();
    build_into(&repo().join(FIXTURE), out.path()).unwrap();
    let index = std::fs::read_to_string(out.path().join("index.html")).unwrap();
    let prefix = "https://github.com/lesslucifer/Trawler/releases/download/v0.1.0/";
    let mut ids: Vec<String> = index
        .split(&format!("href=\"{prefix}"))
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter_map(|file| site::installer_id(file).unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids, site::installer_ids("0.1.0").into_iter().collect::<Vec<_>>());
    assert_eq!(ids.len(), 11);
    assert!(
        index.contains("<title>Trawler — See every HTTP and HTTPS request your apps make.</title>")
    );
    assert!(index.contains(site::CSP));
    assert!(index.contains("<script src=\"site.js\""), "the page loads site.js");
    assert!(!index.contains("{{"), "an unfilled marker");
}

#[test]
fn a_fixture_without_the_arm64_msi_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut release: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo().join(FIXTURE)).unwrap()).unwrap();
    release["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|asset| asset["name"] != "Trawler_0.1.0_arm64_en-US.msi");
    let fixture = dir.path().join("release.json");
    std::fs::write(&fixture, release.to_string()).unwrap();
    let out = dir.path().join("site");
    let refusal = build_into(&fixture, &out).unwrap_err();
    assert_eq!(refusal, "v0.1.0 lacks msi-arm64");
    assert!(!out.exists());
}

#[test]
fn a_draft_or_pre_release_fixture_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    for (field, needle) in [("draft", "draft"), ("prerelease", "pre-release")] {
        let mut release: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(repo().join(FIXTURE)).unwrap()).unwrap();
        release[field] = serde_json::Value::Bool(true);
        let fixture = dir.path().join(format!("{field}.json"));
        std::fs::write(&fixture, release.to_string()).unwrap();
        let refusal = build_into(&fixture, &dir.path().join(field)).unwrap_err();
        assert!(refusal.contains(needle), "{refusal}");
    }
}

#[test]
fn a_non_empty_out_dir_is_refused_and_kept() {
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("stale.html"), "old").unwrap();
    let refusal = build_into(&repo().join(FIXTURE), out.path()).unwrap_err();
    assert!(refusal.contains("is not empty"), "{refusal}");
    assert_eq!(std::fs::read_to_string(out.path().join("stale.html")).unwrap(), "old");
    assert!(!out.path().join("index.html").exists());
}
