use std::path::Path;

use serde_json::{json, Map, Value};

const TEMPLATE: &str = ".github/release-notes.md";
const ARCHS: [&str; 2] = ["x86_64", "aarch64"];

fn has(name: &str, suffix: &str) -> bool {
    name.ends_with(suffix)
}

fn arch_of(name: &str) -> Result<&'static str, String> {
    if name.contains("aarch64") || name.contains("arm64") {
        Ok("aarch64")
    } else if name.contains("x86_64") || name.contains("x64") || name.contains("amd64") {
        Ok("x86_64")
    } else {
        Err(format!("{name} names no architecture"))
    }
}

/// The updater keys an installer fills, or `None` for a file that is not an updater target.
pub fn keys_of(name: &str) -> Result<Option<Vec<String>>, String> {
    if has(name, ".app.tar.gz") {
        return Ok(Some(
            ARCHS.iter().flat_map(|a| [format!("darwin-{a}"), format!("darwin-{a}-app")]).collect(),
        ));
    }
    let (platform, kind, plain) = if has(name, "-setup.exe") {
        ("windows", "nsis", true)
    } else if has(name, ".msi") {
        ("windows", "msi", false)
    } else if has(name, ".AppImage") {
        ("linux", "appimage", true)
    } else if has(name, ".deb") {
        ("linux", "deb", false)
    } else if has(name, ".rpm") {
        ("linux", "rpm", false)
    } else {
        return Ok(None);
    };
    let arch = arch_of(name)?;
    let mut keys = vec![format!("{platform}-{arch}-{kind}")];
    if plain {
        keys.push(format!("{platform}-{arch}"));
    }
    Ok(Some(keys))
}

fn required_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for arch in ARCHS {
        keys.extend(
            [
                "darwin-{}",
                "darwin-{}-app",
                "windows-{}",
                "windows-{}-nsis",
                "windows-{}-msi",
                "linux-{}",
                "linux-{}-appimage",
                "linux-{}-deb",
                "linux-{}-rpm",
            ]
            .map(|key| key.replace("{}", arch)),
        );
    }
    keys
}

pub struct Release<'a> {
    pub version: &'a str,
    pub notes: &'a str,
    pub pub_date: &'a str,
    pub download_base: &'a str,
}

/// `assets`: each file name with its `.sig` text, `None` when it has no signature.
pub fn manifest(
    release: &Release<'_>,
    assets: &[(String, Option<String>)],
) -> Result<Value, String> {
    let mut platforms = Map::new();
    for (name, signature) in assets {
        let Some(keys) = keys_of(name)? else { continue };
        let signature = signature.as_deref().ok_or(format!("{name} has no .sig"))?;
        for key in keys {
            let entry = json!({
                "signature": signature.trim(),
                "url": format!("{}/{name}", release.download_base),
            });
            platforms.insert(key, entry);
        }
    }
    let missing: Vec<String> =
        required_keys().into_iter().filter(|key| !platforms.contains_key(key)).collect();
    if !missing.is_empty() {
        return Err(format!("the release lacks updater entries: {}", missing.join(", ")));
    }
    Ok(json!({
        "version": release.version,
        "notes": release.notes,
        "pub_date": release.pub_date,
        "platforms": platforms,
    }))
}

pub fn notes(template: &str, version: &str) -> String {
    template.replace("{version}", version)
}

fn flag<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1])
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let usage = || crate::USAGE.to_string();
    let dist = Path::new(args.first().filter(|a| !a.starts_with('-')).ok_or_else(usage)?);
    let tag = flag(args, "--tag").ok_or_else(usage)?;
    let github_repo = flag(args, "--repo").ok_or_else(usage)?;
    let pub_date = flag(args, "--pub-date").ok_or_else(usage)?;
    let version = tag.strip_prefix('v').ok_or(format!("tag {tag} does not start with v"))?;
    let template = std::fs::read_to_string(repo.join(TEMPLATE))
        .map_err(|e| format!("read {TEMPLATE}: {e}"))?;
    let notes = notes(&template, version);

    let mut assets = Vec::new();
    let entries = std::fs::read_dir(dist).map_err(|e| format!("read {}: {e}", dist.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if has(&name, ".sig") || !entry.path().is_file() {
            continue;
        }
        let signature = std::fs::read_to_string(dist.join(format!("{name}.sig"))).ok();
        assets.push((name, signature));
    }
    assets.sort();
    let release = Release {
        version,
        notes: &notes,
        pub_date,
        download_base: &format!("https://github.com/{github_repo}/releases/download/{tag}"),
    };
    let latest = manifest(&release, &assets)?;
    let text = serde_json::to_string_pretty(&latest).map_err(|e| e.to_string())?;
    std::fs::write(dist.join("latest.json"), text).map_err(|e| e.to_string())?;
    std::fs::write(dist.join("notes.md"), &notes).map_err(|e| e.to_string())?;
    Ok((0, format!("wrote latest.json and notes.md for {tag} in {}", dist.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALLERS: [&str; 10] = [
        "Trawler.app.tar.gz",
        "Trawler_0.1.0_x64-setup.exe",
        "Trawler_0.1.0_arm64-setup.exe",
        "Trawler_0.1.0_x64_en-US.msi",
        "Trawler_0.1.0_arm64_en-US.msi",
        "Trawler_0.1.0_amd64.AppImage",
        "Trawler_0.1.0_aarch64.AppImage",
        "Trawler_0.1.0_amd64.deb",
        "Trawler_0.1.0_arm64.deb",
        "Trawler-0.1.0-1.x86_64.rpm",
    ];

    fn signed(names: &[&str]) -> Vec<(String, Option<String>)> {
        names.iter().map(|n| ((*n).to_string(), Some(format!("sig-of-{n}\n")))).collect()
    }

    fn full() -> Vec<(String, Option<String>)> {
        let mut names = INSTALLERS.to_vec();
        names.extend([
            "Trawler_0.1.0_aarch64.deb",
            "Trawler-0.1.0-1.aarch64.rpm",
            "Trawler_0.1.0_universal.dmg",
        ]);
        signed(&names)
    }

    fn release() -> Release<'static> {
        Release {
            version: "0.1.0",
            notes: "notes",
            pub_date: "2026-11-05T00:00:00Z",
            download_base: "https://example.test/dl",
        }
    }

    fn keys(name: &str) -> Vec<String> {
        keys_of(name).unwrap().unwrap()
    }

    #[test]
    fn the_universal_archive_fills_both_mac_architectures() {
        assert_eq!(
            keys("Trawler.app.tar.gz"),
            ["darwin-x86_64", "darwin-x86_64-app", "darwin-aarch64", "darwin-aarch64-app"]
        );
    }

    #[test]
    fn each_installer_fills_its_kind_and_the_appimage_and_nsis_fill_the_plain_key() {
        assert_eq!(
            keys("Trawler_0.1.0_arm64-setup.exe"),
            ["windows-aarch64-nsis", "windows-aarch64"]
        );
        assert_eq!(keys("Trawler_0.1.0_x64_en-US.msi"), ["windows-x86_64-msi"]);
        assert_eq!(keys("Trawler_0.1.0_amd64.AppImage"), ["linux-x86_64-appimage", "linux-x86_64"]);
        assert_eq!(keys("Trawler_0.1.0_aarch64.deb"), ["linux-aarch64-deb"]);
        assert_eq!(keys("Trawler-0.1.0-1.x86_64.rpm"), ["linux-x86_64-rpm"]);
    }

    #[test]
    fn a_dmg_is_no_updater_target_and_an_installer_without_an_architecture_is_refused() {
        assert_eq!(keys_of("Trawler_0.1.0_universal.dmg").unwrap(), None);
        assert!(keys_of("Trawler.deb").unwrap_err().contains("architecture"));
    }

    #[test]
    fn a_full_release_names_every_platform_with_its_signature_and_url() {
        let latest = manifest(&release(), &full()).unwrap();
        assert_eq!(latest["version"], "0.1.0");
        assert_eq!(latest["notes"], "notes");
        assert_eq!(latest["platforms"].as_object().unwrap().len(), required_keys().len());
        let entry = &latest["platforms"]["windows-x86_64-nsis"];
        assert_eq!(entry["signature"], "sig-of-Trawler_0.1.0_x64-setup.exe");
        assert_eq!(entry["url"], "https://example.test/dl/Trawler_0.1.0_x64-setup.exe");
    }

    #[test]
    fn an_installer_without_a_signature_is_refused_by_name() {
        let mut assets = full();
        assets[1].1 = None;
        let refusal = manifest(&release(), &assets).unwrap_err();
        assert!(refusal.contains("Trawler_0.1.0_x64-setup.exe has no .sig"), "{refusal}");
    }

    #[test]
    fn a_missing_target_is_refused_and_named() {
        let assets: Vec<_> = full().into_iter().filter(|(name, _)| !has(name, ".rpm")).collect();
        let refusal = manifest(&release(), &assets).unwrap_err();
        assert!(
            refusal.contains("linux-x86_64-rpm") && refusal.contains("linux-aarch64-rpm"),
            "{refusal}"
        );
        assert!(!refusal.contains("windows"), "{refusal}");
    }

    #[test]
    fn the_notes_carry_the_version_and_every_os_s_first_open_steps() {
        let template =
            std::fs::read_to_string(crate::repo_root().expect("repo root").join(TEMPLATE))
                .expect("the notes template");
        let text = notes(&template, "0.2.0-1");
        assert!(text.starts_with("# Trawler 0.2.0-1"), "{text}");
        for needle in
            ["## macOS", "Open Anyway", "## Windows", "Run anyway", "## Linux", "AppImage"]
        {
            assert!(text.contains(needle), "the notes lack {needle}");
        }
        assert!(!text.contains("{version}"));
    }

    #[test]
    fn run_writes_latest_json_and_notes_beside_the_artifacts() {
        let dist = tempfile::tempdir().unwrap();
        for (name, signature) in full() {
            std::fs::write(dist.path().join(&name), "bytes").unwrap();
            std::fs::write(dist.path().join(format!("{name}.sig")), signature.unwrap()).unwrap();
        }
        let repo = crate::repo_root().unwrap();
        let args = [
            dist.path().to_str().unwrap(),
            "--tag",
            "v0.1.0",
            "--repo",
            "o/r",
            "--pub-date",
            "2026-11-05T00:00:00Z",
        ];
        run(&args, &repo).unwrap();
        let latest: Value =
            serde_json::from_slice(&std::fs::read(dist.path().join("latest.json")).unwrap())
                .unwrap();
        assert_eq!(
            latest["platforms"]["darwin-aarch64"]["url"],
            "https://github.com/o/r/releases/download/v0.1.0/Trawler.app.tar.gz"
        );
        assert!(std::fs::read_to_string(dist.path().join("notes.md")).unwrap().contains("0.1.0"));
    }

    #[test]
    fn a_tag_without_v_is_refused() {
        let dist = tempfile::tempdir().unwrap();
        let args =
            [dist.path().to_str().unwrap(), "--tag", "0.1.0", "--repo", "o/r", "--pub-date", "d"];
        assert!(run(&args, &crate::repo_root().unwrap()).unwrap_err().contains("start with v"));
    }

    #[test]
    fn release_manifest_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask release-manifest <dist-dir>"));
    }
}
