use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

pub const NOTICE: &str = "THIRD-PARTY-NOTICES.txt";
const APP: &str = "trawler-ui";
const RULE: &str =
    "================================================================================";

const STANDARD: [(&str, &str); 4] = [
    ("MIT", include_str!("../licenses/MIT.txt")),
    ("Apache-2.0", include_str!("../licenses/Apache-2.0.txt")),
    ("BSD-3-Clause", include_str!("../licenses/BSD-3-Clause.txt")),
    ("MPL-2.0", include_str!("../licenses/MPL-2.0.txt")),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub registry: &'static str,
    pub name: String,
    pub version: String,
    pub license: String,
    pub authors: Vec<String>,
    pub texts: Vec<String>,
}

impl Package {
    pub fn attribution(&self) -> String {
        format!("{} {} {} ({})", self.registry, self.name, self.version, self.license)
    }
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    if !args.is_empty() {
        return Err(super::USAGE.to_string());
    }
    std::fs::write(repo.join(NOTICE), generate(repo)?)
        .map_err(|e| format!("write {NOTICE}: {e}"))?;
    Ok((0, format!("wrote {NOTICE}")))
}

pub fn generate(repo: &Path) -> Result<String, String> {
    let own =
        std::fs::read_to_string(repo.join("LICENSE")).map_err(|e| format!("read LICENSE: {e}"))?;
    let mut packages = crates(repo)?;
    packages.extend(npm_packages(repo)?);
    render(&own, &packages)
}

/// Each distinct licence text once, followed by every package it covers.
pub fn render(own: &str, packages: &[Package]) -> Result<String, String> {
    let mut sections: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let mut without_text = Vec::new();
    for package in packages {
        let (texts, user) = if package.texts.is_empty() {
            let Some(text) = standard_for(&package.license) else {
                without_text.push(package.attribution());
                continue;
            };
            let holders = if package.authors.is_empty() {
                "its authors".to_string()
            } else {
                package.authors.join("; ")
            };
            (vec![normalized(text)], format!("{}, copyright {holders}", package.attribution()))
        } else {
            (package.texts.clone(), package.attribution())
        };
        for text in texts {
            let key = text.split_whitespace().collect::<Vec<_>>().join(" ");
            sections.entry(key).or_insert_with(|| (text, BTreeSet::new())).1.insert(user.clone());
        }
    }
    if !without_text.is_empty() {
        return Err(format!(
            "no licence file and no standard text for: {}",
            without_text.join(", ")
        ));
    }
    let mut sections: Vec<(String, BTreeSet<String>)> = sections.into_values().collect();
    sections.sort_by(|a, b| a.1.first().cmp(&b.1.first()).then_with(|| a.0.cmp(&b.0)));
    let mut out = format!(
        "Trawler\n\n{}\n\n{RULE}\nThird-party software\n\nTrawler includes the software below. Each licence text is followed by the\npackages it covers. A package that ships no licence file is listed under the\nstandard text of a licence it offers, with its authors as the copyright holders.\n",
        normalized(own)
    );
    for (text, users) in sections {
        let _ = write!(out, "{RULE}\n\n{text}\n\nCovers:\n");
        for user in users {
            let _ = writeln!(out, "  {user}");
        }
        out.push('\n');
    }
    Ok(out)
}

fn standard_for(expression: &str) -> Option<&'static str> {
    if expression.contains(" AND ") {
        return None;
    }
    let offered: BTreeSet<&str> = expression
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')))
        .collect();
    STANDARD.iter().find(|(id, _)| offered.contains(id)).map(|(_, text)| *text)
}

fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

fn is_licence_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("spdx"))
        && ["licen", "copying", "notice", "unlicense"].iter().any(|p| lower.starts_with(p))
}

pub fn texts_in(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.file_name().and_then(|n| n.to_str()).is_some_and(is_licence_file)
        })
        .collect();
    files.sort();
    let mut texts = Vec::new();
    for file in files {
        let bytes = std::fs::read(&file).map_err(|e| format!("read {}: {e}", file.display()))?;
        let text = normalized(&String::from_utf8_lossy(&bytes));
        if !text.is_empty() {
            texts.push(text);
        }
    }
    Ok(texts)
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Crate>,
    workspace_members: Vec<String>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Crate {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    authors: Vec<String>,
    manifest_path: PathBuf,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Deserialize)]
struct DepKind {
    kind: Option<String>,
}

// Normal dependencies only, on every platform: build scripts and dev-dependencies
// never reach an installer, and one notice serves every OS's installer.
fn crates(repo: &Path) -> Result<Vec<Package>, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("failed to spawn cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let metadata: Metadata =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata output: {e}"))?;
    let nodes: BTreeMap<&str, &Node> =
        metadata.resolve.nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let app = metadata
        .packages
        .iter()
        .find(|package| package.name == APP)
        .ok_or_else(|| format!("the workspace has no {APP} crate"))?;
    let mut linked = BTreeSet::new();
    let mut pending = vec![app.id.as_str()];
    while let Some(id) = pending.pop() {
        if !linked.insert(id) {
            continue;
        }
        let node = nodes.get(id).ok_or_else(|| format!("cargo metadata resolves no {id}"))?;
        pending.extend(
            node.deps
                .iter()
                .filter(|dep| dep.dep_kinds.iter().any(|kind| kind.kind.is_none()))
                .map(|dep| dep.pkg.as_str()),
        );
    }
    let mut packages = Vec::new();
    for package in &metadata.packages {
        if !linked.contains(package.id.as_str()) || metadata.workspace_members.contains(&package.id)
        {
            continue;
        }
        let dir = package.manifest_path.parent().unwrap_or(Path::new("."));
        let mut texts = texts_in(dir)?;
        if let Some(file) = &package.license_file {
            let path = dir.join(file);
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            let text = normalized(&text);
            if !texts.contains(&text) {
                texts.push(text);
            }
        }
        packages.push(Package {
            registry: "crate",
            name: package.name.clone(),
            version: package.version.clone(),
            license: package.license.clone().unwrap_or_else(|| "see its licence file".into()),
            authors: package.authors.clone(),
            texts,
        });
    }
    Ok(packages)
}

#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    versions: Vec<String>,
    paths: Vec<PathBuf>,
    license: String,
    author: Option<String>,
}

fn npm_packages(repo: &Path) -> Result<Vec<Package>, String> {
    let pnpm = crate::package::Os::named(std::env::consts::OS)?.pnpm();
    let out = Command::new(pnpm)
        .args(["licenses", "list", "--prod", "--json"])
        .current_dir(repo.join("ui"))
        .output()
        .map_err(|e| format!("failed to spawn {pnpm}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pnpm licenses list failed (pnpm install in ui/ first): {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let listed: BTreeMap<String, Vec<NpmPackage>> =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("pnpm licenses output: {e}"))?;
    let mut packages = Vec::new();
    for package in listed.into_values().flatten() {
        for (version, path) in package.versions.iter().zip(&package.paths) {
            packages.push(Package {
                registry: "npm",
                name: package.name.clone(),
                version: version.clone(),
                license: package.license.clone(),
                authors: package.author.iter().cloned().collect(),
                texts: texts_in(path)?,
            });
        }
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, license: &str, texts: &[&str]) -> Package {
        Package {
            registry: "crate",
            name: name.to_string(),
            version: "1.0.0".to_string(),
            license: license.to_string(),
            authors: vec![format!("{name} author")],
            texts: texts.iter().map(|text| (*text).to_string()).collect(),
        }
    }

    #[test]
    fn trawler_s_own_licence_comes_first_and_each_text_once_with_every_package_it_covers() {
        let notice = render(
            "MIT License\r\n\r\nCopyright (c) 2026 Vu Luu\r\n",
            &[
                package("b", "MIT", &["Text  one\n"]),
                package("a", "MIT OR Apache-2.0", &["Text one", "Text two"]),
            ],
        )
        .unwrap();
        assert!(notice.starts_with("Trawler\n\nMIT License\n\nCopyright (c) 2026 Vu Luu\n\n"));
        assert_eq!(notice.matches("Text one").count() + notice.matches("Text  one").count(), 1);
        assert_eq!(notice.matches("Text two").count(), 1);
        let first = notice.find("Text").unwrap();
        let covers = &notice[first..notice[first..].find(RULE).map_or(notice.len(), |i| first + i)];
        assert!(covers.contains("  crate a 1.0.0 (MIT OR Apache-2.0)\n  crate b 1.0.0 (MIT)\n"));
    }

    #[test]
    fn a_package_without_a_licence_file_gets_the_standard_text_of_a_licence_it_offers() {
        let notice =
            render("MIT License", &[package("objc2", "Zlib OR Apache-2.0 OR MIT", &[])]).unwrap();
        assert!(notice.contains("Copyright (c) <year> <copyright holders>"), "{notice}");
        assert!(notice
            .contains("  crate objc2 1.0.0 (Zlib OR Apache-2.0 OR MIT), copyright objc2 author\n"));
        let mpl = render("MIT License", &[package("selectors", "MPL-2.0", &[])]).unwrap();
        assert!(mpl.contains("Mozilla Public License Version 2.0"));
    }

    #[test]
    fn a_package_with_no_licence_text_to_give_is_refused_by_name() {
        let refused = render(
            "MIT License",
            &[
                package("isc-only", "ISC", &[]),
                package("both", "MIT AND BSD-3-Clause", &[]),
                package("fine", "ISC", &["ISC text"]),
            ],
        )
        .unwrap_err();
        assert_eq!(
            refused,
            "no licence file and no standard text for: crate isc-only 1.0.0 (ISC), crate both 1.0.0 (MIT AND BSD-3-Clause)"
        );
    }

    #[test]
    fn licence_files_are_read_and_spdx_tags_are_not() {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [
            ("LICENSE-MIT", "mit\r\n"),
            ("COPYING", "copying"),
            ("LICENSE.spdx", "SPDX-License-Identifier: MIT"),
            ("README.md", "readme"),
        ] {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        assert_eq!(texts_in(dir.path()).unwrap(), ["copying", "mit"]);
    }

    #[test]
    fn the_committed_notice_is_the_one_the_lockfiles_generate() {
        let repo = crate::repo_root().unwrap();
        let committed = std::fs::read_to_string(repo.join(NOTICE)).unwrap_or_default();
        assert!(
            generate(&repo).unwrap() == committed,
            "{NOTICE} is stale against Cargo.lock / pnpm-lock.yaml: run cargo xtask licenses"
        );
    }

    #[test]
    fn the_notice_carries_the_copyleft_data_unicode_and_font_licence_texts() {
        let notice = std::fs::read_to_string(crate::repo_root().unwrap().join(NOTICE)).unwrap();
        for text in [
            "Mozilla Public License Version 2.0",
            "Community Data License Agreement - Permissive - Version 2.0",
            "UNICODE LICENSE V3",
            "SIL OPEN FONT LICENSE Version 1.1",
        ] {
            assert!(notice.contains(text), "{NOTICE} lacks {text}");
        }
        for font in ["inter", "dm-mono", "instrument-serif"] {
            assert!(notice.contains(&format!("npm @fontsource/{font} ")), "{NOTICE} lacks {font}");
        }
    }

    #[test]
    fn the_app_bundles_the_notice_as_a_resource_beside_its_binary() {
        let repo = crate::repo_root().unwrap();
        let conf = std::fs::read_to_string(repo.join("ui/src-tauri/tauri.conf.json")).unwrap();
        let conf: serde_json::Value = serde_json::from_str(&conf).unwrap();
        assert_eq!(conf["bundle"]["resources"][format!("../../{NOTICE}")], NOTICE);
    }

    #[test]
    fn licenses_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask licenses "));
    }
}
