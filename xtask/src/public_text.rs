use std::collections::BTreeSet;
use std::path::Path;

use crate::package::Os;

pub const PUBLISHED: [&str; 9] = [
    "README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "CODE_OF_CONDUCT.md",
    "PRIVACY.md",
    "CHANGELOG.md",
    ".github/release-notes.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/ISSUE_TEMPLATE/config.yml",
];

pub const ISSUE_FORMS: [&str; 2] =
    [".github/ISSUE_TEMPLATE/bug_report.yml", ".github/ISSUE_TEMPLATE/feature_request.yml"];

pub const DENIED_NAMES: [&str; 9] = [
    "charles",
    "proxyman",
    "mitmproxy",
    "fiddler",
    "http toolkit",
    "httptoolkit",
    "burp",
    "wireshark",
    "requestly",
];

// Linked before the work unit that writes them lands; each entry must leave this
// list the moment its file exists, so its anchors are checked from then on.
pub const NOT_YET_WRITTEN: [&str; 1] = ["GUIDE.md"];

pub fn targets(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (open, close) in [("](", ')'), ("src=\"", '"'), ("srcset=\"", '"'), ("href=\"", '"')] {
        let mut rest = text;
        while let Some(at) = rest.find(open) {
            rest = &rest[at + open.len()..];
            if let Some(end) = rest.find(close) {
                found.push(rest[..end].trim().to_string());
            }
        }
    }
    found
}

pub fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == ' ' || *c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

pub fn anchors(markdown: &str) -> BTreeSet<String> {
    let mut fenced = false;
    let mut found = BTreeSet::new();
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && line.starts_with('#') {
            found.insert(slug(line.trim_start_matches('#')));
        }
    }
    found
}

pub fn broken_targets(repo: &Path, file: &str, text: &str) -> Vec<String> {
    let dir = Path::new(file).parent().unwrap_or(Path::new(""));
    let mut broken = Vec::new();
    for target in targets(text) {
        if target.contains("://") || target.starts_with("mailto:") {
            continue;
        }
        let (path, anchor) = target.split_once('#').unwrap_or((&target, ""));
        let resolved = if path.is_empty() { Path::new(file).to_path_buf() } else { dir.join(path) };
        let name = resolved.to_string_lossy().replace('\\', "/");
        if NOT_YET_WRITTEN.contains(&name.as_str()) {
            continue;
        }
        let Ok(content) = std::fs::read(repo.join(&resolved)) else {
            broken.push(format!("{file}: {target} (no {name})"));
            continue;
        };
        let markdown = resolved.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !anchor.is_empty()
            && markdown
            && !anchors(&String::from_utf8_lossy(&content)).contains(anchor)
        {
            broken.push(format!("{file}: {target} (no heading #{anchor} in {name})"));
        }
    }
    broken
}

pub fn denied_names(file: &str, text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    DENIED_NAMES
        .iter()
        .filter(|name| {
            lower.match_indices(*name).any(|(at, _)| {
                let before = lower[..at].chars().next_back();
                let after = lower[at + name.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            })
        })
        .map(|name| format!("{file}: names {name}"))
        .collect()
}

const KINDS: [(&str, &str); 6] = [
    (".dmg", "dmg"),
    ("-setup.exe", "nsis"),
    (".msi", "msi"),
    (".AppImage", "appimage"),
    (".deb", "deb"),
    (".rpm", "rpm"),
];

pub fn installer_kinds(os: Os) -> BTreeSet<&'static str> {
    os.bundles("0.1.0").split(',').filter(|kind| *kind != "app").collect()
}

fn os_of(row: &str) -> Option<Os> {
    let system = row.trim_start_matches('|').split('|').next()?;
    [("macOS", Os::MacOs), ("Windows", Os::Windows), ("Linux", Os::Linux)]
        .into_iter()
        .find(|(name, _)| system.contains(name))
        .map(|(_, os)| os)
}

pub fn install_table_drift(readme: &str) -> Vec<String> {
    let readme = format!("\n{readme}");
    let section = readme.split("\n## Install").nth(1).unwrap_or_default();
    let section = section.split("\n## ").next().unwrap_or_default();
    let mut drift = Vec::new();
    let mut seen = BTreeSet::new();
    for row in section.lines().filter(|line| line.starts_with('|')) {
        let Some(os) = os_of(row) else { continue };
        seen.insert(format!("{os:?}"));
        let named: BTreeSet<&str> = KINDS
            .iter()
            .filter(|(suffix, _)| row.contains(suffix))
            .map(|(_, kind)| *kind)
            .collect();
        let built = installer_kinds(os);
        if named != built {
            drift.push(format!("{os:?} row names {named:?}, package.rs builds {built:?}: {row}"));
        }
        if named.contains("rpm") && !row.to_lowercase().contains("final") {
            drift.push(format!(
                "{os:?} row offers .rpm without saying only final releases carry it: {row}"
            ));
        }
    }
    for os in ["MacOs", "Windows", "Linux"] {
        if !seen.contains(os) {
            drift.push(format!("the install table has no {os} row"));
        }
    }
    drift
}

pub fn form_problems(file: &str, yaml: &str) -> Vec<String> {
    let form: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(form) => form,
        Err(e) => return vec![format!("{file}: not YAML: {e}")],
    };
    let mut problems = Vec::new();
    for key in ["name", "description"] {
        if form[key].as_str().is_none_or(str::is_empty) {
            problems.push(format!("{file}: no {key}"));
        }
    }
    let Some(body) = form["body"].as_sequence().filter(|body| !body.is_empty()) else {
        problems.push(format!("{file}: no body"));
        return problems;
    };
    let mut ids = BTreeSet::new();
    for field in body {
        let kind = field["type"].as_str().unwrap_or_default();
        if !["markdown", "input", "textarea", "dropdown", "checkboxes"].contains(&kind) {
            problems.push(format!("{file}: a field of unknown type {kind:?}"));
        } else if kind == "markdown" {
            if field["attributes"]["value"].as_str().is_none() {
                problems.push(format!("{file}: a markdown field with no value"));
            }
        } else {
            match field["id"].as_str() {
                Some(id) if ids.insert(id.to_string()) => {}
                _ => problems.push(format!("{file}: a {kind} without a unique id")),
            }
            if field["attributes"]["label"].as_str().is_none() {
                problems.push(format!("{file}: a {kind} without a label"));
            }
            if ["dropdown", "checkboxes"].contains(&kind)
                && field["attributes"]["options"].as_sequence().is_none()
            {
                problems.push(format!("{file}: a {kind} without options"));
            }
        }
    }
    problems
}

pub fn config_problems(yaml: &str) -> Vec<String> {
    let config: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(config) => config,
        Err(e) => return vec![format!("config.yml: not YAML: {e}")],
    };
    let mut problems = Vec::new();
    if config["blank_issues_enabled"].as_bool() != Some(false) {
        problems.push("config.yml: blank issues are not turned off".to_string());
    }
    for link in config["contact_links"].as_sequence().into_iter().flatten() {
        for key in ["name", "url", "about"] {
            if link[key].as_str().is_none_or(str::is_empty) {
                problems.push(format!("config.yml: a contact link without {key}"));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        crate::repo_root().unwrap()
    }

    fn read(file: &str) -> String {
        std::fs::read_to_string(repo().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
    }

    #[test]
    fn links_images_and_html_sources_are_all_found() {
        let text = "[a](b.md#c) ![i](docs/x.png) <img src=\"y.png\"> <source srcset=\"z.png\"> <a href=\"#top\">";
        assert_eq!(targets(text), ["b.md#c", "docs/x.png", "y.png", "z.png", "#top"]);
    }

    #[test]
    fn a_heading_s_anchor_is_github_s() {
        assert_eq!(slug(" Contributing · Security · Licence"), "contributing--security--licence");
        assert_eq!(slug("Check your download"), "check-your-download");
        assert_eq!(slug("Why don't I see an app's traffic?"), "why-dont-i-see-an-apps-traffic");
    }

    #[test]
    fn a_hash_line_inside_a_code_fence_is_no_heading() {
        assert_eq!(
            anchors("# Top\n```sh\n# a comment\n```\n## Next\n"),
            BTreeSet::from(["top".to_string(), "next".to_string()])
        );
    }

    #[test]
    fn checkboxes_need_their_options() {
        let form = "name: B\ndescription: d\nbody:\n  - type: checkboxes\n    id: c\n    attributes: {label: C}\n";
        assert_eq!(form_problems("b.yml", form), ["b.yml: a checkboxes without options"]);
    }

    #[test]
    fn a_missing_file_or_heading_is_broken_and_a_url_or_pending_file_is_not() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("A.md"), "# Top\n## First run\n").unwrap();
        let text = "[ok](A.md#first-run) [self](#top) [gone](B.md) [bad](A.md#nope) \
                    [web](https://x.test/B.md) [guide](GUIDE.md#install)";
        assert_eq!(
            broken_targets(dir.path(), "A.md", text),
            ["A.md: B.md (no B.md)", "A.md: A.md#nope (no heading #nope in A.md)"]
        );
    }

    #[test]
    fn a_denied_name_is_found_whole_word_in_any_case_only() {
        assert_eq!(
            denied_names("R.md", "A take on Charles and HTTP Toolkit."),
            ["R.md: names charles", "R.md: names http toolkit"]
        );
        assert_eq!(denied_names("R.md", "Charleston, burpee"), Vec::<String>::new());
    }

    #[test]
    fn each_host_s_installers_are_what_package_rs_bundles_for_a_final() {
        assert_eq!(installer_kinds(Os::MacOs), BTreeSet::from(["dmg"]));
        assert_eq!(installer_kinds(Os::Windows), BTreeSet::from(["msi", "nsis"]));
        assert_eq!(installer_kinds(Os::Linux), BTreeSet::from(["appimage", "deb", "rpm"]));
    }

    #[test]
    fn an_install_row_naming_other_kinds_or_rpm_without_final_drifts() {
        let readme = "## Install\n| System | Download |\n|---|---|\n\
                      | macOS | `T_universal.dmg` |\n| Windows x64 | `T_x64-setup.exe` |\n\
                      | Linux x64 | `T.AppImage` · `T.deb` · `T.rpm` |\n## Next\n| Linux | x |\n";
        let drift = install_table_drift(readme);
        assert_eq!(drift.len(), 2, "{drift:?}");
        assert!(drift[0].starts_with("Windows row names {\"nsis\"}"), "{drift:?}");
        assert!(drift[1].contains("only final releases"), "{drift:?}");
        assert_eq!(install_table_drift("# none\n").len(), 3);
    }

    #[test]
    fn an_issue_form_needs_its_name_description_and_labelled_unique_fields() {
        let good =
            "name: Bug\ndescription: d\nbody:\n  - type: markdown\n    attributes: {value: hi}\n  \
                    - type: input\n    id: v\n    attributes: {label: Version}\n";
        assert_eq!(form_problems("b.yml", good), Vec::<String>::new());
        let bad = "name: Bug\nbody:\n  - type: input\n    id: v\n    attributes: {label: V}\n  \
                   - type: dropdown\n    id: v\n    attributes: {label: K}\n";
        assert_eq!(
            form_problems("b.yml", bad),
            [
                "b.yml: no description",
                "b.yml: a dropdown without a unique id",
                "b.yml: a dropdown without options"
            ]
        );
        assert_eq!(form_problems("b.yml", "name: [").len(), 1);
    }

    #[test]
    fn the_config_turns_blank_issues_off_and_names_each_link() {
        let good = "blank_issues_enabled: false\ncontact_links:\n  - {name: a, url: u, about: b}\n";
        assert_eq!(config_problems(good), Vec::<String>::new());
        assert_eq!(config_problems("contact_links:\n  - {name: a}\n").len(), 3);
    }

    #[test]
    fn every_published_link_and_image_resolves() {
        let repo = repo();
        let broken: Vec<String> = PUBLISHED
            .iter()
            .chain(&ISSUE_FORMS)
            .flat_map(|file| broken_targets(&repo, file, &read(file)))
            .collect();
        assert!(broken.is_empty(), "{broken:#?}");
    }

    #[test]
    fn a_file_still_listed_as_not_yet_written_does_not_exist() {
        for file in NOT_YET_WRITTEN {
            assert!(!repo().join(file).exists(), "{file} exists: drop it from NOT_YET_WRITTEN");
        }
    }

    #[test]
    fn no_published_file_names_another_tool() {
        let named: Vec<String> = PUBLISHED
            .iter()
            .chain(&ISSUE_FORMS)
            .flat_map(|file| denied_names(file, &read(file)))
            .collect();
        assert!(named.is_empty(), "{named:#?}");
    }

    #[test]
    fn the_readme_install_table_names_what_package_rs_builds() {
        let drift = install_table_drift(&read("README.md"));
        assert!(drift.is_empty(), "{drift:#?}");
    }

    #[test]
    fn the_issue_forms_and_their_config_parse() {
        let mut problems: Vec<String> =
            ISSUE_FORMS.iter().flat_map(|file| form_problems(file, &read(file))).collect();
        problems.extend(config_problems(&read(".github/ISSUE_TEMPLATE/config.yml")));
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
