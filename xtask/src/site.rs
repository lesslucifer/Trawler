use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::package::Os;
use crate::public_text;
use crate::walkthrough::escape;

pub const BASE_URL: &str = "https://lesslucifer.github.io/Trawler/";
pub const CSP: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self'; style-src 'self'; script-src 'self'; base-uri 'none'; form-action 'none'\">";
pub const SCRIPT: &str = "site.js";
pub const SCRIPT_BUDGET: usize = 4096;
pub const FRAME: &str = "site";
pub const ASSETS: [(&str, &str); 5] = [
    ("ui/public/favicon.svg", "favicon.svg"),
    ("ui/src-tauri/icons/brand/wordmark-light.png", "wordmark-light.png"),
    ("ui/src-tauri/icons/brand/wordmark-dark.png", "wordmark-dark.png"),
    ("ui/src-tauri/icons/brand/social-preview.png", "social-preview.png"),
    (".github/screenshots/s1-main-window.png", "s1-main-window.png"),
];
const LINKS: &str = "ui/src-tauri/src/links.rs";
const CHECKSUMS: &str = "SHA256SUMS";
const SYSTEMS: [Os; 3] = [Os::MacOs, Os::Windows, Os::Linux];
const MIN_CONTRAST: f64 = 4.5;
const DARK: &str = "@media (prefers-color-scheme: dark)";
const REFUSED_MARKUP: [&str; 8] = [
    "<iframe",
    "<form",
    "<object",
    "<embed",
    "<style",
    " style=",
    "javascript:",
    "http-equiv=\"refresh\"",
];
const SCRIPT_REFUSES: [&str; 18] = [
    "fetch",
    "XMLHttpRequest",
    "WebSocket",
    "sendBeacon",
    "EventSource",
    "import",
    "eval",
    "Function",
    "cookie",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "serviceWorker",
    "Worker",
    "SharedWorker",
    "WebTransport",
    "RTCPeerConnection",
    "://",
];

/// The path the site is served under, `/Trawler/`.
pub fn base_path() -> &'static str {
    let host_and_path = BASE_URL.split_once("://").map_or(BASE_URL, |(_, rest)| rest);
    host_and_path.find('/').map_or("/", |at| &host_and_path[at..])
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
}

fn arches(os: Os) -> &'static [&'static str] {
    match os {
        Os::MacOs => &["universal"],
        Os::Windows | Os::Linux => &["x64", "arm64"],
    }
}

/// Every `<kind>-<arch>` a final at `version` ships: one download row each.
pub fn installer_ids(version: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for os in SYSTEMS {
        for kind in public_text::installer_kinds(os, version) {
            ids.extend(arches(os).iter().map(|arch| format!("{kind}-{arch}")));
        }
    }
    ids
}

pub fn installer_id(file: &str) -> Result<Option<String>, String> {
    let Some(kind) = public_text::kind_of(file) else { return Ok(None) };
    let arch = if kind == "dmg" {
        if !file.contains("universal") {
            return Err(format!("{file} is not the universal dmg"));
        }
        "universal"
    } else if crate::release_manifest::arch_of(file)? == "aarch64" {
        "arm64"
    } else {
        "x64"
    };
    Ok(Some(format!("{kind}-{arch}")))
}

fn version_order(version: &str) -> Result<(u64, u64, u64, u64), String> {
    let bad = || format!("{version} is not <major>.<minor>.<patch>[-<n>]");
    let (core, pre) = version.split_once('-').map_or((version, None), |(c, p)| (c, Some(p)));
    let parts: Vec<u64> =
        core.split('.').map(|part| part.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
    let [major, minor, patch] = parts[..] else { return Err(bad()) };
    let pre = match pre {
        None => u64::MAX,
        Some(n) => n.parse().map_err(|_| bad())?,
    };
    Ok((major, minor, patch, pre))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Downloads {
    pub tag: String,
    pub version: String,
    pub files: BTreeMap<String, String>,
}

pub fn downloads(release: &Release, workspace: &str) -> Result<Downloads, String> {
    let tag = &release.tag_name;
    if release.draft {
        return Err(format!("{tag} is a draft; the site names a published release"));
    }
    let version = tag.strip_prefix('v').ok_or(format!("tag {tag} does not start with v"))?;
    if release.prerelease || version.contains('-') {
        return Err(format!("{tag} is a pre-release; the site names the latest final"));
    }
    if version_order(version)? > version_order(workspace)? {
        return Err(format!(
            "{tag} is newer than this tree's {workspace}; build the site from a tree that has it"
        ));
    }
    let mut files = BTreeMap::new();
    for asset in &release.assets {
        let Some(id) = installer_id(&asset.name)? else { continue };
        if !names_version(&asset.name, version) {
            return Err(format!("{} names no version {version}", asset.name));
        }
        if let Some(other) = files.insert(id.clone(), asset.name.clone()) {
            return Err(format!("{other} and {} both claim {id}", asset.name));
        }
    }
    let missing: Vec<String> =
        installer_ids(version).into_iter().filter(|id| !files.contains_key(id)).collect();
    if !missing.is_empty() {
        return Err(format!("{tag} lacks {}", missing.join(", ")));
    }
    if !release.assets.iter().any(|asset| asset.name == CHECKSUMS) {
        return Err(format!("{tag} lacks {CHECKSUMS}"));
    }
    Ok(Downloads { tag: tag.clone(), version: version.to_string(), files })
}

fn names_version(file: &str, version: &str) -> bool {
    let part_of_a_number = |c: char| c.is_ascii_digit() || c == '.';
    file.match_indices(version).any(|(at, _)| {
        !file[..at].chars().next_back().is_some_and(part_of_a_number)
            && !file[at + version.len()..].chars().next().is_some_and(part_of_a_number)
    })
}

pub fn str_const(rust: &str, name: &str) -> Option<String> {
    let value = rust.split(&format!("const {name}: &str = \"")).nth(1)?;
    value.split('"').next().map(String::from)
}

fn inner<'a>(html: &'a str, open: &str, close: &str) -> Option<(&'a str, &'a str)> {
    let start = html.find(open)? + open.len();
    let body_start = start + html[start..].find('>')? + 1;
    let length = html[body_start..].find(close)?;
    Some((&html[body_start..body_start + length], &html[body_start + length + close.len()..]))
}

fn plain(what: &str, html: &str) -> Result<String, String> {
    let text = html.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.contains(['<', '>', '&']) {
        return Err(format!("README.md's {what} is not one line of plain text: {text:?}"));
    }
    Ok(text)
}

/// The README's `<h3>` pitch and the centred line under it.
pub fn pitch_and_sub(readme: &str) -> Result<(String, String), String> {
    let (pitch, rest) = inner(readme, "<h3", "</h3>").ok_or("README.md has no <h3> pitch")?;
    let (sub, _) = inner(rest, "<p align=\"center\"", "</p>")
        .ok_or("README.md has no <p align=\"center\"> line under the pitch")?;
    Ok((plain("pitch", pitch)?, plain("sub line", sub)?))
}

/// The lines under `heading` up to the next heading.
pub fn section<'a>(file: &str, markdown: &'a str, heading: &str) -> Result<Vec<&'a str>, String> {
    let mut lines = markdown.lines().skip_while(|line| line.trim_end() != heading);
    if lines.next().is_none() {
        return Err(format!("{file} has no {heading:?} heading"));
    }
    Ok(lines.take_while(|line| !line.starts_with('#')).collect())
}

fn blocks<'a>(lines: &[&'a str]) -> Vec<Vec<&'a str>> {
    lines
        .split(|line| line.trim().is_empty())
        .filter(|block| !block.is_empty())
        .map(<[&str]>::to_vec)
        .collect()
}

pub fn first_block<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    blocks(lines).into_iter().next().unwrap_or_default()
}

fn block_refusal(line: &str) -> Option<&'static str> {
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if line.trim_start().len() != line.len() {
        Some("an indented line")
    } else if line.starts_with('#') {
        Some("a heading")
    } else if line.starts_with('>') {
        Some("a quote")
    } else if line.starts_with('|') {
        Some("a table")
    } else if line.starts_with("```") {
        Some("a code fence")
    } else if digits > 0 && (line[digits..].starts_with(". ") || line[digits..].starts_with(") ")) {
        Some("a numbered list")
    } else if line.starts_with("* ") || line.starts_with("+ ") {
        Some("a list marked other than -")
    } else {
        None
    }
}

/// Bullets and paragraphs of `code`, **bold**, [links](..) and text; any other
/// markdown is refused, so a construct this renderer would print raw fails the build.
pub fn render(lines: &[&str], blob: &str) -> Result<String, String> {
    let mut html = Vec::new();
    for block in blocks(lines) {
        if let Some(refusal) = block.iter().find_map(|line| block_refusal(line)) {
            return Err(format!("{refusal} is outside the site's markdown: {block:?}"));
        }
        let items: Vec<&str> = block.iter().filter_map(|line| line.strip_prefix("- ")).collect();
        if items.len() == block.len() {
            let rendered: Vec<String> = items
                .iter()
                .map(|item| Ok(format!("<li>{}</li>", inline(item, blob)?)))
                .collect::<Result<_, String>>()?;
            html.push(format!("<ul>\n{}\n</ul>", rendered.join("\n")));
        } else if items.is_empty() {
            html.push(format!("<p>{}</p>", inline(&block.join(" "), blob)?));
        } else {
            return Err(format!("a list run into a paragraph: {block:?}"));
        }
    }
    Ok(html.join("\n"))
}

fn link_target(target: &str, blob: &str) -> Result<String, String> {
    if target.is_empty() || target.contains(char::is_whitespace) {
        return Err(format!("a link target with a title or no address: {target:?}"));
    }
    if target.starts_with('#') {
        return Err(format!("a link to {target} has no page on the site"));
    }
    if target.contains("://") {
        Ok(target.to_string())
    } else {
        Ok(format!("{blob}{target}"))
    }
}

fn inline(text: &str, blob: &str) -> Result<String, String> {
    let mut html = String::new();
    let mut bold = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix('`') {
            let end = after.find('`').ok_or(format!("an unclosed ` in {text:?}"))?;
            let _ = write!(html, "<code>{}</code>", escape(&after[..end]));
            rest = &after[end + 1..];
        } else if let Some(after) = rest.strip_prefix("**") {
            html.push_str(if bold { "</strong>" } else { "<strong>" });
            bold = !bold;
            rest = after;
        } else if let Some(after) = rest.strip_prefix('[') {
            let (label, tail) =
                after.split_once("](").ok_or(format!("a [ that opens no link in {text:?}"))?;
            let (target, tail) =
                tail.split_once(')').ok_or(format!("an unclosed link in {text:?}"))?;
            let href = escape(&link_target(target, blob)?);
            let _ = write!(html, "<a href=\"{href}\">{}</a>", inline(label, blob)?);
            rest = tail;
        } else if rest.starts_with("![") || rest.starts_with("://") || "*_\\~<".contains(c) {
            return Err(format!("{c:?} at {rest:?} is outside the site's markdown"));
        } else {
            html.push_str(&escape(&c.to_string()));
            rest = &rest[c.len_utf8()..];
        }
    }
    if bold {
        return Err(format!("an unclosed ** in {text:?}"));
    }
    Ok(html)
}

pub fn fill(file: &str, text: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut filled = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("{{") {
        filled.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let end = after.find("}}").ok_or(format!("{file}: a {{{{ marker is never closed"))?;
        let name = &after[..end];
        let value = values.get(name).ok_or(format!("{file}: no marker {{{{{name}}}}}"))?;
        filled.push_str(value);
        rest = &after[end + 2..];
    }
    filled.push_str(rest);
    Ok(filled)
}

fn read(repo: &Path, file: &str) -> Result<String, String> {
    std::fs::read_to_string(repo.join(file)).map_err(|e| format!("read {file}: {e}"))
}

/// What the checks hold the built files against.
pub struct Facts<'a> {
    pub repo: &'a Path,
    pub source: String,
    pub guide: String,
    pub release: Downloads,
}

impl<'a> Facts<'a> {
    pub fn of(repo: &'a Path, release: Downloads) -> Result<Self, String> {
        let links = read(repo, LINKS)?;
        let named = |name| str_const(&links, name).ok_or(format!("{LINKS} has no {name}"));
        Ok(Self { repo, source: named("SOURCE_CODE")?, guide: named("USER_GUIDE")?, release })
    }

    fn blob(&self) -> String {
        format!("{}/blob/main/", self.source)
    }
}

fn values(facts: &Facts<'_>) -> Result<BTreeMap<String, String>, String> {
    let downloads = &facts.release;
    let readme = read(facts.repo, "README.md")?;
    let privacy = read(facts.repo, "PRIVACY.md")?;
    let (pitch, sub) = pitch_and_sub(&readme)?;
    let source = &facts.source;
    let blob = &facts.blob();
    let tag = &downloads.tag;
    let mut values = BTreeMap::from([
        ("pitch".to_string(), escape(&pitch)),
        ("sub".to_string(), escape(&sub)),
        (
            "features".to_string(),
            render(&section("README.md", &readme, "## What you can do")?, blob)?,
        ),
        (
            "privacy".to_string(),
            render(&first_block(&section("PRIVACY.md", &privacy, "# Privacy")?), blob)?,
        ),
        ("version".to_string(), escape(&downloads.version)),
        ("release".to_string(), escape(&format!("{source}/releases/tag/{tag}"))),
        ("latest".to_string(), escape(&format!("{source}/releases/latest"))),
        (
            "sha256sums".to_string(),
            escape(&format!("{source}/releases/download/{tag}/{CHECKSUMS}")),
        ),
        ("guide".to_string(), escape(&facts.guide)),
        ("source".to_string(), escape(source)),
        ("base".to_string(), escape(BASE_URL)),
        ("path".to_string(), escape(base_path())),
    ]);
    for (id, file) in &downloads.files {
        let href = format!("{source}/releases/download/{tag}/{file}");
        values.insert(format!("href:{id}"), escape(&href));
        values.insert(format!("file:{id}"), escape(file));
    }
    Ok(values)
}

fn has_extension(file: &str, extension: &str) -> bool {
    Path::new(file).extension().is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
}

fn is_text(file: &str) -> bool {
    has_extension(file, "html") || has_extension(file, "css") || has_extension(file, "js")
}

/// The site's files by name, filled and checked; nothing is written.
pub fn build(repo: &Path, release_json: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let release: Release =
        serde_json::from_str(release_json).map_err(|e| format!("the release JSON: {e}"))?;
    let facts = Facts::of(repo, downloads(&release, &crate::package::version_of(repo)?)?)?;
    let values = values(&facts)?;
    let mut files = BTreeMap::new();
    let frame = std::fs::read_dir(repo.join(FRAME)).map_err(|e| format!("read {FRAME}/: {e}"))?;
    for entry in frame {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if !path.is_file() {
            return Err(format!("{FRAME}/{name} is not a file; the frame is flat"));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("read {FRAME}/{name}: {e}"))?;
        let bytes = if is_text(&name) {
            let text = String::from_utf8(bytes).map_err(|e| format!("{FRAME}/{name}: {e}"))?;
            fill(&format!("{FRAME}/{name}"), &text, &values)?.into_bytes()
        } else {
            bytes
        };
        files.insert(name, bytes);
    }
    for (from, to) in ASSETS {
        let bytes = std::fs::read(repo.join(from)).map_err(|e| format!("read {from}: {e}"))?;
        if files.insert(to.to_string(), bytes).is_some() {
            return Err(format!("{FRAME}/{to} and the asset {from} share a name"));
        }
    }
    let problems = problems(&files, &facts);
    if !problems.is_empty() {
        return Err(format!("the site is refused:\n  {}", problems.join("\n  ")));
    }
    Ok(files)
}

fn starts_an_attribute(html: &str, at: usize) -> bool {
    !html[..at].ends_with(|c: char| c.is_alphanumeric() || "-_.".contains(c))
}

/// Every address (`//`, `http:`, `https:`) outside an `href` value or a `content`
/// value under `BASE_URL`: an address in any other attribute or in text may be fetched.
fn stray_addresses(name: &str, html: &str) -> BTreeSet<String> {
    let lower = html.to_ascii_lowercase();
    let mut problems = BTreeSet::new();
    let found = ["//", "http:", "https:"].into_iter().flat_map(|mark| lower.match_indices(mark));
    for (at, _) in found {
        let before = &html[..at];
        let attribute = before.rfind('"').and_then(|open| {
            let named = before[..open].strip_suffix('=')?;
            let attribute =
                named.rsplit(|c: char| !(c.is_alphanumeric() || "-:".contains(c))).next()?;
            Some((attribute.to_ascii_lowercase(), &html[open + 1..]))
        });
        let allowed = match &attribute {
            Some((attribute, _)) if attribute == "href" => true,
            Some((attribute, value)) if attribute == "content" => value.starts_with(BASE_URL),
            _ => false,
        };
        if !allowed {
            let start = before.rfind(['"', '\'', ' ', '\n', '>', '=']).map_or(0, |at| at + 1);
            let address =
                html[start..].split(['"', '\'', ' ', '<', '>']).next().unwrap_or_default();
            problems.insert(format!(
                "{name}: {address} is an address outside an href, a request outside the site"
            ));
        }
    }
    problems
}

fn event_handlers(name: &str, html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let tags = lower.split('<').skip(1).filter_map(|rest| rest.split('>').next());
    let mut problems = Vec::new();
    for tag in tags {
        for (at, _) in tag.match_indices("on").filter(|(at, _)| starts_an_attribute(tag, *at)) {
            let rest = &tag[at..];
            let length = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
            if length > 2 && rest[length..].trim_start().starts_with('=') {
                problems.push(format!("{name}: an inline handler {}", &rest[..length]));
            }
        }
    }
    problems
}

fn script_tag_problems(files: &BTreeMap<String, Vec<u8>>, name: &str, html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut problems = Vec::new();
    for (at, _) in lower.match_indices("<script") {
        let rest = &html[at..];
        let Some(end) = rest.find('>') else {
            problems.push(format!("{name}: an unclosed <script"));
            continue;
        };
        let loads = match attribute_values(&rest[..end], "src")[..] {
            [src] => resolve(files, name, src).ok(),
            _ => None,
        };
        if loads.as_deref() != Some(SCRIPT) {
            problems.push(format!("{name}: a <script> that does not load {SCRIPT} by src"));
        }
        if !lower[at + end + 1..].starts_with("</script>") {
            problems.push(format!("{name}: an inline script"));
        }
    }
    problems
}

/// The one script reaches no network, storage or code from a string, by name:
/// a belt over the CSP's `default-src 'none'`, kept small enough to read whole.
pub fn script_problems(name: &str, js: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if js.len() > SCRIPT_BUDGET {
        problems.push(format!("{name}: {} bytes, over the {SCRIPT_BUDGET}-byte budget", js.len()));
    }
    let identifier = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    for word in SCRIPT_REFUSES {
        let named = js.match_indices(word).any(|(at, _)| {
            !js[..at].chars().next_back().is_some_and(identifier)
                && !js[at + word.len()..].chars().next().is_some_and(identifier)
        });
        if named || (word == "://" && js.contains(word)) {
            problems.push(format!("{name}: names {word}"));
        }
    }
    problems
}

fn attribute_values<'a>(html: &'a str, attribute: &str) -> Vec<&'a str> {
    let opener = format!("{attribute}=\"");
    html.to_ascii_lowercase()
        .match_indices(&opener)
        .filter(|(at, _)| starts_an_attribute(html, *at))
        .filter_map(|(at, _)| {
            let start = at + opener.len();
            html[start..].find('"').map(|end| &html[start..start + end])
        })
        .collect()
}

fn unquoted_attributes(name: &str, html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut problems = Vec::new();
    for attribute in ["href", "src", "srcset"] {
        for (at, _) in lower.match_indices(attribute) {
            let after = &lower[at + attribute.len()..];
            if starts_an_attribute(&lower, at)
                && after.trim_start().starts_with('=')
                && !after.starts_with("=\"")
            {
                problems.push(format!("{name}: a {attribute} not written {attribute}=\"…\""));
            }
        }
    }
    problems
}

fn src_targets(html: &str) -> Vec<&str> {
    let mut found = attribute_values(html, "src");
    for srcset in attribute_values(html, "srcset") {
        found.extend(srcset.split(',').filter_map(|candidate| candidate.split_whitespace().next()));
    }
    found
}

fn css_urls(css: &str) -> Vec<&str> {
    css.to_ascii_lowercase()
        .match_indices("url(")
        .filter_map(|(at, _)| {
            let start = at + "url(".len();
            css[start..]
                .find(')')
                .map(|end| css[start..start + end].trim().trim_matches(['"', '\'']))
        })
        .collect()
}

fn is_absolute(target: &str) -> bool {
    target.starts_with("//")
        || target.split(['/', '#']).next().is_some_and(|head| head.contains(':'))
}

fn unescaped(attribute: &str) -> String {
    attribute
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn resolve(files: &BTreeMap<String, Vec<u8>>, from: &str, target: &str) -> Result<String, String> {
    let (path, anchor) = target.split_once('#').unwrap_or((target, ""));
    let joined = if path.is_empty() {
        PathBuf::from(from)
    } else if let Some(from_root) = path.strip_prefix(base_path()) {
        PathBuf::from(from_root)
    } else if path.starts_with('/') {
        return Err(format!("{from}: {target} is outside {}", base_path()));
    } else {
        Path::new(from).parent().unwrap_or(Path::new("")).join(path)
    };
    let mut parts = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::Normal(name) => parts.push(name),
            Component::CurDir => {}
            _ if parts.pop() => {}
            _ => return Err(format!("{from}: {target} leaves the site")),
        }
    }
    let mut name = parts.to_string_lossy().replace('\\', "/");
    if name.is_empty() || path.ends_with('/') {
        name = format!("{name}{}index.html", if name.is_empty() { "" } else { "/" });
    }
    let file = files.get(&name).ok_or(format!("{from}: {target} (no {name} in the site)"))?;
    if !anchor.is_empty() && !String::from_utf8_lossy(file).contains(&format!("id=\"{anchor}\"")) {
        return Err(format!("{from}: {target} (no id=\"{anchor}\" in {name})"));
    }
    Ok(name)
}

fn blob_problem(facts: &Facts<'_>, from: &str, href: &str) -> Option<String> {
    let path = href.strip_prefix(&facts.blob())?;
    let (file, anchor) = path.split_once('#').unwrap_or((path, ""));
    if !public_text::exists_as_named(facts.repo, Path::new(file)) {
        return Some(format!("{from}: {href} (the tree has no {file})"));
    }
    if anchor.is_empty() || !has_extension(file, "md") {
        return None;
    }
    let text = std::fs::read_to_string(facts.repo.join(file)).unwrap_or_default();
    (!public_text::anchors(&text).contains(anchor))
        .then(|| format!("{from}: {href} (no heading #{anchor} in {file})"))
}

fn link_tags(html: &str) -> Vec<&str> {
    html.to_ascii_lowercase()
        .match_indices("<link")
        .filter_map(|(at, _)| {
            let start = at + "<link".len();
            html[start..].find('>').map(|end| &html[start..start + end])
        })
        .collect()
}

fn html_problems(
    files: &BTreeMap<String, Vec<u8>>,
    facts: &Facts<'_>,
    name: &str,
    html: &str,
    downloads: &mut Vec<String>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if html.matches("Content-Security-Policy").count() != 1 || !html.contains(CSP) {
        problems.push(format!("{name}: the CSP meta is missing, altered or doubled; want {CSP}"));
    }
    problems.extend(unquoted_attributes(name, html));
    problems.extend(stray_addresses(name, html));
    problems.extend(event_handlers(name, html));
    problems.extend(script_tag_problems(files, name, html));
    let lower = html.to_lowercase();
    problems.extend(
        REFUSED_MARKUP
            .iter()
            .filter(|markup| lower.contains(*markup))
            .map(|markup| format!("{name}: carries {}", markup.trim())),
    );
    for tag in link_tags(html) {
        let rel = attribute_values(tag, "rel");
        let external = attribute_values(tag, "href").into_iter().any(is_absolute);
        let canonical = matches!(rel[..], [rel] if rel.eq_ignore_ascii_case("canonical"));
        if external && !canonical {
            problems.push(format!("{name}: <link{tag}> loads from outside the site"));
        }
    }
    for content in attribute_values(html, "content") {
        if let Some(local) = unescaped(content).strip_prefix(BASE_URL) {
            problems.extend(resolve(files, name, &format!("{}{local}", base_path())).err());
        }
    }
    for target in src_targets(html) {
        let target = unescaped(target);
        if is_absolute(&target) {
            problems.push(format!("{name}: {target} is a request outside the site"));
        } else if let Err(problem) = resolve(files, name, &target) {
            problems.push(problem);
        }
    }
    let download_prefix = format!("{}/releases/download/", facts.source);
    for href in attribute_values(html, "href").into_iter().map(unescaped) {
        if let Some(local) = href.strip_prefix(BASE_URL) {
            let local = format!("{}{local}", base_path());
            problems.extend(resolve(files, name, &local).err());
        } else if href == facts.source || href.starts_with(&format!("{}/", facts.source)) {
            if href.starts_with(&download_prefix) {
                downloads.push(href.clone());
            }
            problems.extend(blob_problem(facts, name, &href));
        } else if is_absolute(&href) {
            problems.push(format!("{name}: {href} goes outside {}", facts.source));
        } else {
            problems.extend(resolve(files, name, &href).err());
        }
    }
    problems
}

fn download_problems(facts: &Facts<'_>, hrefs: &[String]) -> Vec<String> {
    let release = &facts.release;
    let prefix = format!("{}/releases/download/{}/", facts.source, release.tag);
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();
    for href in hrefs {
        let Some(file) = href.strip_prefix(&prefix) else {
            problems.push(format!("{href} is not a {} download", release.tag));
            continue;
        };
        if file != CHECKSUMS && !release.files.values().any(|shipped| shipped == file) {
            problems.push(format!("{href} names no file of {}", release.tag));
        }
        match installer_id(file) {
            Ok(Some(id)) => {
                ids.insert(id);
            }
            Ok(None) => {}
            Err(problem) => problems.push(problem),
        }
    }
    let want = installer_ids(&release.version);
    if ids != want {
        let missing: Vec<&String> = want.difference(&ids).collect();
        let extra: Vec<&String> = ids.difference(&want).collect();
        problems.push(format!(
            "the download rows lack {missing:?} and add {extra:?} against Os::bundles at {}",
            release.version
        ));
    }
    problems
}

fn hex_colour(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#')?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).and_then(|d| u8::try_from(d).ok()))
        .collect::<Option<_>>()?;
    match digits[..] {
        [r, g, b] => Some([r * 17, g * 17, b * 17]),
        [r1, r2, g1, g2, b1, b2] => Some([r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2]),
        _ => None,
    }
}

fn luminance(rgb: [u8; 3]) -> f64 {
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

type Tokens = BTreeMap<String, String>;

/// The custom properties of the top-level `:root` and of the dark scheme's `:root`.
pub fn themes(css: &str) -> (Tokens, Tokens) {
    let mut light = Tokens::new();
    let mut dark = Tokens::new();
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut declare = |stack: &[String], text: &str| {
        let Some((":root", outer)) = stack.split_last().map(|(last, outer)| (last.as_str(), outer))
        else {
            return;
        };
        let theme = match outer {
            [] => &mut light,
            [media] if media == DARK => &mut dark,
            _ => return,
        };
        if let Some((name, value)) = text.split_once(':') {
            if name.trim().starts_with("--") {
                theme.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
    };
    let css = strip_comments(css);
    for c in css.chars() {
        match c {
            '{' => {
                stack.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
                text.clear();
            }
            '}' => {
                declare(&stack, &text);
                stack.pop();
                text.clear();
            }
            ';' => {
                declare(&stack, &text);
                text.clear();
            }
            _ => text.push(c),
        }
    }
    (light, dark)
}

fn strip_comments(css: &str) -> String {
    let mut kept = String::new();
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        kept.push_str(&rest[..at]);
        rest = rest[at..].find("*/").map_or("", |end| &rest[at + end + 2..]);
    }
    kept + rest
}

pub fn contrast_problems(file: &str, css: &str) -> Vec<String> {
    let (light, dark_overrides) = themes(css);
    let mut problems = Vec::new();
    if dark_overrides.is_empty() {
        problems.push(format!("{file}: no {DARK} :root theme"));
    }
    let mut dark = light.clone();
    dark.extend(dark_overrides);
    for (theme, tokens) in [("light", &light), ("dark", &dark)] {
        let colours = |prefix: &str| -> Vec<(&String, &String)> {
            tokens.iter().filter(|(name, _)| name.starts_with(prefix)).collect()
        };
        let (texts, backgrounds) = (colours("--text"), colours("--bg"));
        if texts.is_empty() || backgrounds.is_empty() {
            problems.push(format!("{file} {theme}: needs --text* and --bg* tokens"));
            continue;
        }
        for (text, text_value) in &texts {
            for (background, background_value) in &backgrounds {
                let (Some(fg), Some(bg)) = (hex_colour(text_value), hex_colour(background_value))
                else {
                    problems.push(format!(
                        "{file} {theme}: {text} {text_value} or {background} {background_value} is no #hex colour"
                    ));
                    continue;
                };
                let ratio = contrast(fg, bg);
                if ratio < MIN_CONTRAST {
                    problems.push(format!(
                        "{file} {theme}: {text} {text_value} on {background} {background_value} is {ratio:.2}:1, under {MIN_CONTRAST}:1"
                    ));
                }
            }
        }
    }
    problems
}

/// Everything that keeps the built site from shipping, one line each.
pub fn problems(files: &BTreeMap<String, Vec<u8>>, facts: &Facts<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut downloads = Vec::new();
    for (name, bytes) in files.iter().filter(|(name, _)| is_text(name)) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            problems.push(format!("{name}: not UTF-8"));
            continue;
        };
        problems.extend(public_text::denied_names(name, text));
        if has_extension(name, "html") {
            problems.extend(html_problems(files, facts, name, text, &mut downloads));
        } else if has_extension(name, "js") {
            if name == SCRIPT {
                problems.extend(script_problems(name, text));
            } else {
                problems.push(format!("{name}: a script other than {SCRIPT}"));
            }
        } else {
            if text.to_ascii_lowercase().contains("@import") {
                problems.push(format!("{name}: carries @import"));
            }
            if strip_comments(text).contains("//") {
                problems.push(format!("{name}: an address with // is a request outside the site"));
            }
            for target in css_urls(text) {
                if is_absolute(target) {
                    problems.push(format!("{name}: url({target}) is a request outside the site"));
                } else if let Err(problem) = resolve(files, name, target) {
                    problems.push(problem);
                }
            }
            problems.extend(contrast_problems(name, text));
        }
    }
    let index = files.get("index.html").map(|bytes| String::from_utf8_lossy(bytes));
    let guide = escape(&facts.guide);
    if !index.is_some_and(|html| {
        html.contains(&format!("href=\"{guide}\"")) || html.contains(&format!("href=\"{guide}#"))
    }) {
        problems.push(format!("index.html: no link to the guide {}", facts.guide));
    }
    problems.extend(download_problems(facts, &downloads));
    problems
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let usage = || crate::USAGE.to_string();
    let ["build", rest @ ..] = args else { return Err(usage()) };
    let release = crate::flag_value(rest, "--release").ok_or_else(usage)?;
    let out = PathBuf::from(crate::flag_value(rest, "--out").ok_or_else(usage)?);
    if rest.len() != 4 {
        return Err(usage());
    }
    let json = std::fs::read_to_string(&release).map_err(|e| format!("read {release}: {e}"))?;
    let files = build(repo, &json)?;
    if std::fs::read_dir(&out).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!("{} is not empty; remove it and build again", out.display()));
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("create {}: {e}", out.display()))?;
    for (name, bytes) in &files {
        std::fs::write(out.join(name), bytes).map_err(|e| format!("write {name}: {e}"))?;
    }
    Ok((0, format!("wrote the site ({} files) in {}", files.len(), out.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        crate::repo_root().unwrap()
    }

    const SOURCE: &str = "https://github.com/lesslucifer/Trawler";
    const BLOB: &str = "https://github.com/lesslucifer/Trawler/blob/main/";

    fn release(tag: &str, names: &[&str]) -> Release {
        Release {
            tag_name: tag.to_string(),
            draft: false,
            prerelease: false,
            assets: names.iter().map(|name| Asset { name: (*name).to_string() }).collect(),
        }
    }

    const FULL: [&str; 13] = [
        "Trawler_0.1.0_universal.dmg",
        "Trawler_0.1.0_x64-setup.exe",
        "Trawler_0.1.0_x64_en-US.msi",
        "Trawler_0.1.0_arm64-setup.exe",
        "Trawler_0.1.0_arm64_en-US.msi",
        "Trawler_0.1.0_amd64.AppImage",
        "Trawler_0.1.0_amd64.deb",
        "Trawler-0.1.0-1.x86_64.rpm",
        "Trawler_0.1.0_aarch64.AppImage",
        "Trawler_0.1.0_arm64.deb",
        "Trawler-0.1.0-1.aarch64.rpm",
        "Trawler.app.tar.gz",
        "SHA256SUMS",
    ];

    fn without(dropped: &str) -> Vec<&'static str> {
        FULL.iter().copied().filter(|name| *name != dropped).collect()
    }

    #[test]
    fn a_final_ships_eleven_rows_and_each_file_names_its_row() {
        let ids = installer_ids("0.1.0");
        assert_eq!(ids.len(), 11, "{ids:?}");
        assert!(!installer_ids("0.1.0-2").iter().any(|id| id.starts_with("rpm")));
        assert_eq!(installer_id("Trawler_0.1.0_arm64_en-US.msi").unwrap().unwrap(), "msi-arm64");
        assert_eq!(installer_id("Trawler_0.1.0_amd64.AppImage").unwrap().unwrap(), "appimage-x64");
        assert_eq!(installer_id("Trawler_0.1.0_universal.dmg").unwrap().unwrap(), "dmg-universal");
        assert_eq!(installer_id("Trawler_0.1.0_x64-setup.exe.sig").unwrap(), None);
        assert_eq!(installer_id("Trawler.app.tar.gz").unwrap(), None);
        assert!(installer_id("Trawler_0.1.0_aarch64.dmg").is_err());
    }

    #[test]
    fn a_full_final_names_each_row_s_file() {
        let found = downloads(&release("v0.1.0", &FULL), "0.1.0").unwrap();
        assert_eq!(found.version, "0.1.0");
        assert_eq!(found.files.len(), 11);
        assert_eq!(found.files["rpm-arm64"], "Trawler-0.1.0-1.aarch64.rpm");
    }

    #[test]
    fn a_draft_a_pre_release_or_a_missing_kind_is_refused() {
        let mut draft = release("v0.1.0", &FULL);
        draft.draft = true;
        assert!(downloads(&draft, "0.1.0").unwrap_err().contains("draft"));
        let mut pre = release("v0.1.0", &FULL);
        pre.prerelease = true;
        assert!(downloads(&pre, "0.1.0").unwrap_err().contains("pre-release"));
        assert!(downloads(&release("v0.1.0-3", &FULL), "0.1.0")
            .unwrap_err()
            .contains("pre-release"));
        let refusal =
            downloads(&release("v0.1.0", &without("Trawler_0.1.0_arm64_en-US.msi")), "0.1.0")
                .unwrap_err();
        assert_eq!(refusal, "v0.1.0 lacks msi-arm64");
        let refusal = downloads(&release("v0.1.0", &without("SHA256SUMS")), "0.1.0").unwrap_err();
        assert!(refusal.contains("SHA256SUMS"), "{refusal}");
        assert!(downloads(&release("0.1.0", &FULL), "0.1.0").unwrap_err().contains("start with v"));
    }

    #[test]
    fn two_files_for_one_row_or_a_file_of_another_version_is_refused() {
        let mut names = FULL.to_vec();
        names.push("Trawler_0.1.0_x64_de-DE.msi");
        assert!(downloads(&release("v0.1.0", &names), "0.1.0").unwrap_err().contains("msi-x64"));
        let mut names = without("Trawler_0.1.0_amd64.deb");
        names.push("Trawler_0.0.9_amd64.deb");
        let refusal = downloads(&release("v0.1.0", &names), "0.1.0").unwrap_err();
        assert!(refusal.contains("names no version 0.1.0"), "{refusal}");
        let mut names = without("Trawler_0.1.0_amd64.deb");
        names.push("Trawler_10.1.0_amd64.deb");
        let refusal = downloads(&release("v0.1.0", &names), "0.1.0").unwrap_err();
        assert!(refusal.contains("Trawler_10.1.0_amd64.deb names no version"), "{refusal}");
    }

    #[test]
    fn a_release_newer_than_the_tree_is_refused() {
        assert!(downloads(&release("v0.1.0", &FULL), "0.2.0-1").is_ok());
        let refusal = downloads(&release("v0.1.0", &FULL), "0.1.0-7").unwrap_err();
        assert!(refusal.contains("newer than this tree's 0.1.0-7"), "{refusal}");
        assert!(version_order("0.2.0").unwrap() > version_order("0.2.0-1").unwrap());
        assert!(version_order("0.10.0").unwrap() > version_order("0.9.9").unwrap());
        assert!(version_order("0.1").is_err() && version_order("0.1.0-rc.1").is_err());
    }

    #[test]
    fn the_subset_renders_bullets_paragraphs_code_bold_and_links() {
        let lines = [
            "- Filter with `status:4xx` & **more**.",
            "- See [the guide](GUIDE.md#install).",
            "",
            "One",
            "line.",
        ];
        assert_eq!(
            render(&lines, BLOB).unwrap(),
            "<ul>\n<li>Filter with <code>status:4xx</code> &amp; <strong>more</strong>.</li>\n\
             <li>See <a href=\"https://github.com/lesslucifer/Trawler/blob/main/GUIDE.md#install\">the guide</a>.</li>\n\
             </ul>\n<p>One line.</p>"
        );
    }

    #[test]
    fn markdown_outside_the_subset_is_refused_not_printed_raw() {
        for text in [
            "*one*",
            "_one_",
            "a \\* b",
            "~~gone~~",
            "<b>x</b>",
            "![i](a.png)",
            "[a] b",
            "[a](b",
            "[a](b.md \"t\")",
            "[top](#install)",
            "`open",
            "**open",
            "see https://example.com",
        ] {
            assert!(render(&[text], BLOB).is_err(), "{text}");
        }
        for lines in [
            &["## H"][..],
            &["> q"],
            &["| a |"],
            &["1. one"],
            &["* one"],
            &["- a", "  b"],
            &["- a", "b"],
            &["```", "x"],
        ] {
            assert!(render(lines, BLOB).is_err(), "{lines:?}");
        }
    }

    #[test]
    fn the_readme_s_pitch_and_sub_line_are_its_h3_and_the_line_under_it() {
        let readme = "<p align=\"center\">logo</p>\n<h3 align=\"center\">See it.</h3>\n\n<p align=\"center\">\n  A free\n  proxy.\n</p>\n";
        assert_eq!(
            pitch_and_sub(readme).unwrap(),
            ("See it.".to_string(), "A free proxy.".to_string())
        );
        assert!(pitch_and_sub("<h3>a <b>b</b></h3><p align=\"center\">c</p>").is_err());
        assert!(pitch_and_sub("<h3>a</h3>").is_err());
        assert!(pitch_and_sub("<h3>A &amp; B</h3><p align=\"center\">c</p>").is_err());
    }

    #[test]
    fn a_section_ends_at_the_next_heading_and_a_missing_one_is_named() {
        let markdown = "# Privacy\n\nLead.\nMore.\n\n| a |\n## Next\n";
        let lines = section("P.md", markdown, "# Privacy").unwrap();
        assert_eq!(first_block(&lines), ["Lead.", "More."]);
        assert!(section("P.md", markdown, "## What").unwrap_err().contains("P.md"));
    }

    #[test]
    fn markers_fill_and_an_unknown_or_open_one_is_refused() {
        let values = BTreeMap::from([("a".to_string(), "{{b}}".to_string())]);
        assert_eq!(fill("f", "x {{a}} {y}", &values).unwrap(), "x {{b}} {y}");
        assert_eq!(fill("f", "{{b}}", &values).unwrap_err(), "f: no marker {{b}}");
        assert!(fill("f", "{{a", &values).unwrap_err().contains("never closed"));
    }

    #[test]
    fn links_rs_consts_are_read_as_written() {
        let rust = "pub const SOURCE_CODE: &str = \"https://x.test/r\";\nconst B: &str = \"b\";\n";
        assert_eq!(str_const(rust, "SOURCE_CODE").unwrap(), "https://x.test/r");
        assert_eq!(str_const(rust, "C"), None);
    }

    #[test]
    fn the_base_url_is_the_project_s_github_io_address() {
        let links = std::fs::read_to_string(repo().join(LINKS)).unwrap();
        let source = str_const(&links, "SOURCE_CODE").unwrap();
        let (owner, name) =
            source.trim_start_matches("https://github.com/").split_once('/').unwrap();
        assert_eq!(BASE_URL, format!("https://{owner}.github.io/{name}/"));
        assert_eq!(base_path(), format!("/{name}/"));
    }

    fn facts(repo: &Path) -> Facts<'_> {
        Facts {
            repo,
            source: SOURCE.to_string(),
            guide: format!("{SOURCE}/blob/main/GUIDE.md"),
            release: downloads(&release("v0.1.0", &FULL), "0.1.0").unwrap(),
        }
    }

    const CSS: &str = ":root { --bg: #ffffff; --text: #000000; }\n@media (prefers-color-scheme: dark) { :root { --bg: #000000; --text: #ffffff; } }\n";

    fn page(body: &str) -> String {
        let rows: Vec<String> = FULL[..11]
            .iter()
            .map(|file| format!("<a href=\"{SOURCE}/releases/download/v0.1.0/{file}\">x</a>"))
            .collect();
        format!(
            "<head>{CSP}<link rel=\"stylesheet\" href=\"site.css\"></head><body id=\"top\">\
             <a href=\"{SOURCE}/blob/main/GUIDE.md\">g</a>{}{body}</body>",
            rows.join("")
        )
    }

    fn site(body: &str, css: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("index.html".to_string(), page(body).into_bytes()),
            ("site.css".to_string(), css.as_bytes().to_vec()),
            ("a.png".to_string(), Vec::new()),
            ("site.js".to_string(), b"document.documentElement.dataset.os = \"mac\";".to_vec()),
        ])
    }

    fn problems_of(body: &str) -> Vec<String> {
        let repo = repo();
        problems(&site(body, CSS), &facts(&repo))
    }

    #[test]
    fn a_page_of_resolving_links_and_its_rows_is_green() {
        let body = "<img src=\"a.png\" srcset=\"a.png 1x, /Trawler/a.png 2x\"><a href=\"#top\">t</a>\
                    <a href=\"https://lesslucifer.github.io/Trawler/\">home</a>\
                    <link rel=\"canonical\" href=\"https://lesslucifer.github.io/Trawler/\">\
                    <a href=\"https://github.com/lesslucifer/Trawler/blob/main/GUIDE.md#install\">i</a>";
        assert_eq!(problems_of(body), Vec::<String>::new());
    }

    #[test]
    fn a_link_that_misses_the_site_or_the_tree_is_red() {
        for (body, needle) in [
            ("<img src=\"b.png\">", "no b.png"),
            ("<a href=\"#gone\">x</a>", "no id=\"gone\""),
            ("<a href=\"../x.html\">x</a>", "leaves the site"),
            ("<a href=\"/other/\">x</a>", "outside /Trawler/"),
            ("<a href=\"https://example.com/\">x</a>", "goes outside"),
            ("<a href=\"https://github.com/lesslucifer/TrawlerX\">x</a>", "goes outside"),
            (
                "<a href=\"https://github.com/lesslucifer/Trawler/blob/main/NOPE.md\">x</a>",
                "no NOPE.md",
            ),
            (
                "<a href=\"https://github.com/lesslucifer/Trawler/blob/main/GUIDE.md#nope\">x</a>",
                "no heading #nope",
            ),
        ] {
            let problems = problems_of(body);
            assert!(problems.iter().any(|p| p.contains(needle)), "{body}: {problems:?}");
        }
    }

    #[test]
    fn any_request_outside_the_site_or_markup_the_csp_blocks_is_red() {
        for (body, needle) in [
            ("<img src=\"https://x.test/a.png\">", "outside the site"),
            ("<img srcset=\"a.png 1x, //x.test/a.png 2x\">", "outside the site"),
            ("<img src=\"data:image/png;base64,AA\">", "outside the site"),
            ("<link rel=\"icon\" href=\"https://x.test/f.svg\">", "loads from outside"),
            ("<LINK rel=\"stylesheet\" href=\"https://x.test/s.css\">", "loads from outside"),
            ("<script src=\"a.js\"></script>", "site.js"),
            ("<iframe></iframe>", "<iframe"),
            ("<form></form>", "<form"),
            ("<p style=\"color:red\">x</p>", "style="),
            ("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src *\">", "CSP"),
        ] {
            let problems = problems_of(body);
            assert!(problems.iter().any(|p| p.contains(needle)), "{body}: {problems:?}");
        }
        let repo = repo();
        let mut files = site("", CSS);
        files.insert("index.html".to_string(), page("").replace(CSP, "").into_bytes());
        assert!(problems(&files, &facts(&repo)).iter().any(|p| p.contains("CSP")));
    }

    #[test]
    fn an_attribute_in_another_case_or_quoting_is_still_held() {
        for (body, needle) in [
            ("<img SRC=\"https://x.test/a.png\">", "outside the site"),
            ("<a HREF=\"https://x.test/\">x</a>", "goes outside"),
            ("<img src='https://x.test/a.png'>", "not written src=\""),
            ("<img src=https://x.test/a.png>", "not written src=\""),
            ("<a href = \"https://x.test/\">x</a>", "not written href=\""),
            ("<img srcset='a.png 1x'>", "not written srcset=\""),
        ] {
            let problems = problems_of(body);
            assert!(problems.iter().any(|p| p.contains(needle)), "{body}: {problems:?}");
        }
        let repo = repo();
        let found = problems(
            &site("", &format!("{CSS}body {{ background: URL(https://x.test/a.png) }}")),
            &facts(&repo),
        );
        assert!(found.iter().any(|p| p.contains("outside the site")), "{found:?}");
        let found = problems(&site("", &format!("{CSS}@IMPORT \"x.css\";")), &facts(&repo));
        assert!(found.iter().any(|p| p.contains("@import")), "{found:?}");
    }

    #[test]
    fn a_url_glued_to_another_attribute_or_in_any_other_attribute_or_css_string_is_red() {
        for body in [
            "<img alt=\"x\"src=\"https://x.test/a.png\">",
            "<img/src=\"https://x.test/a.png\">",
            "<img alt=\"x\"src=\"//x.test/a.png\">",
            "<video poster=\"//x.test/p.png\"></video>",
            "<a href=\"#top\" ping=\"https://x.test/\">x</a>",
            "<p>see https://x.test/</p>",
            "<meta property=\"og:image\" content=\"https://x.test/s.png\">",
        ] {
            assert!(!problems_of(body).is_empty(), "{body}");
        }
        let body =
            "<meta property=\"og:image\" content=\"https://lesslucifer.github.io/Trawler/a.png\">";
        assert_eq!(problems_of(body), Vec::<String>::new());
        let repo = repo();
        for css in [
            "body { background: image-set(\"https://x.test/a.png\" 1x) }",
            "body { background: -webkit-image-set('//x.test/a.png' 1x) }",
        ] {
            let found = problems(&site("", &format!("{CSS}{css}")), &facts(&repo));
            assert!(found.iter().any(|p| p.contains("outside the site")), "{css}: {found:?}");
        }
    }

    #[test]
    fn the_one_script_is_site_js_by_src_with_no_inline_code() {
        let body =
            "<script src=\"site.js\" defer></script><script src=\"/Trawler/site.js\"></script>";
        assert_eq!(problems_of(body), Vec::<String>::new());
        for (body, needle) in [
            ("<script>alert(1)</script>", "site.js"),
            ("<SCRIPT src=\"site.js\">alert(1)</SCRIPT>", "inline"),
            ("<button onclick=\"go()\">x</button>", "onclick"),
            ("<img src=\"a.png\" ONLOAD=\"go()\">", "onload"),
            ("<meta http-equiv=\"refresh\" content=\"0;url=https:x.test\">", "refresh"),
        ] {
            let problems = problems_of(body);
            assert!(problems.iter().any(|p| p.contains(needle)), "{body}: {problems:?}");
        }
        assert_eq!(problems_of("<p>Turn it on: one = two.</p>"), Vec::<String>::new());
        let repo = repo();
        let mut files = site("", CSS);
        files.insert("more.js".to_string(), Vec::new());
        assert!(problems(&files, &facts(&repo)).iter().any(|p| p.contains("more.js")));
    }

    #[test]
    fn site_js_reaches_no_network_storage_or_eval_and_stays_small() {
        let benign = "const os = navigator.userAgentData?.platform ?? navigator.platform;\n\
                      // important: the page works without this\n\
                      document.querySelector(`[data-os]`)?.classList.add(\"mine\");";
        assert_eq!(script_problems("site.js", benign), Vec::<String>::new());
        for code in [
            "fetch('/x')",
            "new XMLHttpRequest()",
            "new WebSocket(u)",
            "navigator.sendBeacon(u)",
            "new EventSource(u)",
            "import('./x.js')",
            "import x from './x.js'",
            "eval(s)",
            "new Function(s)",
            "document.cookie = 'a=b'",
            "localStorage.setItem('a', 'b')",
            "sessionStorage.a = 1",
            "indexedDB.open('a')",
            "navigator.serviceWorker.register('s.js')",
            "new Worker('w.js')",
            "img.src = 'https://x.test/p.png'",
        ] {
            assert!(!script_problems("site.js", code).is_empty(), "{code}");
        }
        let big = format!("const a = \"{}\";", "x".repeat(SCRIPT_BUDGET));
        assert!(script_problems("site.js", &big)[0].contains("bytes"));
    }

    #[test]
    fn an_address_by_scheme_alone_a_download_the_release_lacks_or_a_missing_og_file_is_red() {
        for body in [
            "<a href=\"#top\" ping=\"https:x.test\">x</a>",
            "<video poster=\"http:x.test/p.png\"></video>",
            "<meta property=\"og:image\" content=\"https://lesslucifer.github.io/Trawler/nope.png\">",
        ] {
            assert!(!problems_of(body).is_empty(), "{body}");
        }
        let fake = format!(
            "<a href=\"{SOURCE}/releases/download/v0.1.0/Trawler_9.9.9_x64-setup.exe\">x</a>"
        );
        let found = problems_of(&fake);
        assert!(found.iter().any(|p| p.contains("no file of v0.1.0")), "{found:?}");
    }

    #[test]
    fn css_may_load_only_files_of_the_site() {
        let repo = repo();
        for (css, needle) in [
            ("@import \"x.css\";", "@import"),
            ("body { background: url(https://x.test/a.png) }", "outside the site"),
            ("body { background: url('b.png') }", "no b.png"),
        ] {
            let found = problems(&site("", &format!("{CSS}{css}")), &facts(&repo));
            assert!(found.iter().any(|p| p.contains(needle)), "{css}: {found:?}");
        }
        assert_eq!(
            problems(
                &site("", &format!("{CSS}body {{ background: url(\"a.png\") }}")),
                &facts(&repo)
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_denied_name_or_a_missing_guide_link_is_red() {
        assert!(problems_of("<p>Like Charles.</p>").iter().any(|p| p.contains("names charles")));
        let repo = repo();
        let mut files = site("", CSS);
        let page = page("").replace(&format!("{SOURCE}/blob/main/GUIDE.md\""), "#top\"");
        files.insert("index.html".to_string(), page.into_bytes());
        assert!(problems(&files, &facts(&repo)).iter().any(|p| p.contains("no link to the guide")));
    }

    #[test]
    fn download_rows_off_by_a_kind_or_a_tag_are_red() {
        let repo = repo();
        let mut files = site("", CSS);
        let msi = "Trawler_0.1.0_arm64_en-US.msi";
        let page = page("")
            .replace(&format!("<a href=\"{SOURCE}/releases/download/v0.1.0/{msi}\">x</a>"), "");
        files.insert("index.html".to_string(), page.into_bytes());
        let found = problems(&files, &facts(&repo));
        assert!(found.iter().any(|p| p.contains("lack [\"msi-arm64\"]")), "{found:?}");
        let found =
            problems_of(&format!("<a href=\"{SOURCE}/releases/download/v0.0.9/{msi}\">x</a>"));
        assert!(found.iter().any(|p| p.contains("not a v0.1.0 download")), "{found:?}");
    }

    #[test]
    fn contrast_is_held_in_both_themes_and_dark_inherits_what_it_keeps() {
        assert!((contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert_eq!(hex_colour("#fa0"), Some([255, 170, 0]));
        assert_eq!(contrast_problems("s.css", CSS), Vec::<String>::new());
        let light_fail = ":root { --bg: #ffffff; --text: #777777; }\n@media (prefers-color-scheme: dark) { :root { --bg: #000000; } }";
        let found = contrast_problems("s.css", light_fail);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("s.css light: --text #777777 on --bg #ffffff is 4.48:1"),
            "{found:?}"
        );
        let dark_fail = ":root { --bg: #ffffff; --text: #000000; --text-mute: #595959; }\n/* x */ @media (prefers-color-scheme: dark) { :root { --bg: #444444; --text: #ffffff; } }";
        let found = contrast_problems("s.css", dark_fail);
        assert!(found.len() == 1 && found[0].contains("dark: --text-mute #595959"), "{found:?}");
        assert!(contrast_problems("s.css", ":root { --bg: #fff; --text: #000; }")[0]
            .contains("no @media"));
        assert!(contrast_problems("s.css", ":root { --bg: white; --text: #000; }\n@media (prefers-color-scheme: dark) { :root { --bg: #000; --text: #fff; } }")
            .iter()
            .any(|p| p.contains("no #hex")));
    }

    #[test]
    fn the_site_frame_names_no_other_tool() {
        let frame = repo().join(FRAME);
        let named: Vec<String> = std::fs::read_dir(&frame)
            .unwrap()
            .flatten()
            .flat_map(|entry| {
                let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                public_text::denied_names(&entry.file_name().to_string_lossy(), &text)
            })
            .collect();
        assert!(named.is_empty(), "{named:?}");
    }

    #[test]
    fn site_build_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask site build --release <json> --out <dir>"));
    }

    #[test]
    fn an_extra_or_missing_argument_is_usage() {
        let repo = repo();
        for args in [
            &["build", "--release", "r.json"][..],
            &["build", "--release", "r", "--out", "o", "x"],
            &["make"],
        ] {
            assert_eq!(run(args, &repo).unwrap_err(), crate::USAGE, "{args:?}");
        }
    }
}
