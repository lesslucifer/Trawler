// `cargo xtask ui walkthrough` (E-ONBREV.W1): the setup screen filmed on every
// host × platform, chapter by chapter, for the owner to read against what
// UC-ONBOARD says each moment should show. The script lives beside the e2e
// (ui/e2e/walkthrough/); this module judges it, runs it one launch at a time,
// and turns what the runs filmed into a video per reel and one index page.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

pub const FINDINGS: std::ops::RangeInclusive<u32> = 1..=30;
pub const SUPERSEDED: [(&str, &str); 1] = [("R20", "W3.D8")];
pub const REVAMP_FINDINGS: [&str; 57] = [
    "W3.D6", "W3.D8", "W12.E1", "W12.E2", "W12.E3", "W12.E4", "W13.E1", "W13.I1", "W13.I2",
    "W13.R1", "W14.E3", "W14.I1", "W14.I2", "W14.N2", "W15.E1", "W15.E2", "W15.I2", "W15.N2",
    "W16.E1", "W16.E2", "W16.E3", "W16.I1", "W16.I3", "W16.W4", "W17.D1", "W17.E1", "W17.E2",
    "W17.E3", "W17.I1", "W17.I2", "W17.I3", "W17.I4", "W17.I5", "W17.N1", "W17.W4", "W17.I6",
    "W17.S1", "W17.S2", "W17.S3", "W18.E1", "W18.E2", "W18.W1", "W18.W2", "W18.W3", "W18.W4",
    "W18.I1", "W18.M1", "W18.N1", "W18.N4", "W19.W1", "W19.W2", "W19.W3", "W19.W4", "W19.W5",
    "W19.W6", "W19.I1", "W19.M1",
];
pub const HOSTS: [&str; 3] = ["macos", "windows", "linux"];
pub const PLATFORMS: [&str; 8] =
    ["this", "mac", "windows", "ubuntu", "ios-sim", "android-sim", "ios", "android"];

const CHAPTER_SECONDS: u32 = 5;
const RUN_VAR: &str = "TRAWLER_WALKTHROUGH_RUN";
const OUT_VAR: &str = "TRAWLER_WALKTHROUGH_OUT";

#[derive(Debug, Deserialize)]
pub struct Plan {
    pub reels: Vec<PlanReel>,
}

#[derive(Debug, Deserialize)]
pub struct PlanReel {
    pub id: String,
    pub host: String,
    pub platform: String,
    pub launches: Vec<PlanLaunch>,
}

#[derive(Debug, Deserialize)]
pub struct PlanLaunch {
    pub id: String,
    pub chapters: Vec<PlanChapter>,
}

#[derive(Debug, Deserialize)]
pub struct PlanChapter {
    pub id: String,
    pub title: String,
    pub expect: String,
    pub findings: Vec<String>,
}

fn superseded_by(finding: &str) -> Option<&'static str> {
    SUPERSEDED.iter().find(|(old, _)| *old == finding).map(|(_, successor)| *successor)
}

pub fn parse_plan(raw: &str) -> Result<Plan, String> {
    serde_yaml::from_str(raw).map_err(|e| format!("the walkthrough plan does not parse: {e}"))
}

/// Refused before anything is built: a finding no chapter names would go
/// unfilmed, and a reel missing from the grid would leave a host × platform the
/// owner never sees.
pub fn judge_plan(plan: &Plan) -> Result<(), String> {
    let mut problems = Vec::new();
    let named: BTreeSet<&str> = plan
        .reels
        .iter()
        .flat_map(|r| &r.launches)
        .flat_map(|l| &l.chapters)
        .flat_map(|c| c.findings.iter().map(String::as_str))
        .collect();
    for n in FINDINGS {
        let finding = format!("R{n}");
        let filmed_as = superseded_by(&finding).unwrap_or(&finding);
        if !named.contains(filmed_as) {
            problems.push(format!("no chapter names {filmed_as}"));
        }
    }
    for finding in &named {
        if let Some(successor) = superseded_by(finding) {
            problems.push(format!("a chapter names {finding}, which {successor} superseded"));
            continue;
        }
        let reviewed = finding
            .strip_prefix('R')
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| FINDINGS.contains(&n));
        if !reviewed && !REVAMP_FINDINGS.contains(finding) {
            problems.push(format!("a chapter names {finding}, which is no finding of the review"));
        }
    }
    for host in HOSTS {
        for platform in PLATFORMS {
            let id = format!("{host}-{platform}");
            if !plan.reels.iter().any(|r| r.id == id && r.host == host && r.platform == platform) {
                problems.push(format!("no reel {id}"));
            }
        }
    }
    for reel in &plan.reels {
        if reel.launches.is_empty() {
            problems.push(format!("reel {} has no launch", reel.id));
        }
        let mut launches = BTreeSet::new();
        for launch in &reel.launches {
            if !launches.insert(launch.id.as_str()) {
                problems.push(format!("reel {} has two launches {}", reel.id, launch.id));
            }
            if launch.chapters.is_empty() {
                problems.push(format!("{}/{} films no chapter", reel.id, launch.id));
            }
            let mut chapters = BTreeSet::new();
            for chapter in &launch.chapters {
                if !chapters.insert(chapter.id.as_str()) {
                    problems
                        .push(format!("{}/{} has two chapters {}", reel.id, launch.id, chapter.id));
                }
                if chapter.expect.trim().is_empty() {
                    problems
                        .push(format!("{}/{}/{} expects nothing", reel.id, launch.id, chapter.id));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("the walkthrough plan is refused:\n  {}", problems.join("\n  ")))
    }
}

#[derive(Debug, Deserialize)]
struct Manifest {
    chapters: Vec<Recorded>,
}

#[derive(Debug, Deserialize)]
struct Recorded {
    id: String,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filmed {
    Frame(String),
    Skipped(String),
}

pub fn manifest_path(out: &Path, reel: &str, launch: &str) -> PathBuf {
    out.join(reel).join(format!("{launch}.json"))
}

/// What one run filmed, chapter for chapter against its script: a frame on
/// disk, or a skip with its reason. Anything else — no manifest, a chapter
/// missing or out of order, a frame that is not there — is a run that did not
/// film what it was asked to, and is red.
pub fn judge_run(out: &Path, reel: &str, launch: &PlanLaunch) -> Result<Vec<Filmed>, String> {
    let at = format!("{reel}/{}", launch.id);
    let path = manifest_path(out, reel, &launch.id);
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| format!("{at}: the run left no manifest, so it filmed nothing"))?;
    let manifest: Manifest = serde_yaml::from_str(&raw)
        .map_err(|e| format!("{at}: the manifest does not parse: {e}"))?;
    let mut filmed = Vec::new();
    for (i, chapter) in launch.chapters.iter().enumerate() {
        let Some(recorded) = manifest.chapters.get(i) else {
            return Err(format!(
                "{at}: stopped before chapter {} ({}), {} of {} filmed",
                i + 1,
                chapter.id,
                i,
                launch.chapters.len()
            ));
        };
        if recorded.id != chapter.id {
            return Err(format!("{at}: chapter {} is {}, not {}", i + 1, recorded.id, chapter.id));
        }
        filmed.push(match (&recorded.frame, &recorded.skipped) {
            (Some(frame), None) if out.join(reel).join(frame).is_file() => {
                Filmed::Frame(frame.clone())
            }
            (Some(frame), None) => return Err(format!("{at}/{}: no frame at {frame}", chapter.id)),
            (None, Some(why)) if !why.trim().is_empty() => Filmed::Skipped(why.clone()),
            _ => return Err(format!("{at}/{}: neither a frame nor a reason to skip", chapter.id)),
        });
    }
    if manifest.chapters.len() > launch.chapters.len() {
        return Err(format!("{at}: filmed chapters its script does not have"));
    }
    Ok(filmed)
}

pub struct Shot {
    pub frame: String,
    pub caption: String,
}

pub fn caption(n: usize, total: usize, chapter: &PlanChapter) -> String {
    let named = if chapter.findings.is_empty() {
        "holds".to_string()
    } else {
        chapter.findings.join(" · ")
    };
    format!("{n}/{total} · {} [{named}]\nshould show: {}", chapter.title, chapter.expect)
}

fn stamp(seconds: u32, separator: char) -> String {
    format!("{:02}:{:02}:{:02}{separator}000", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

pub fn srt(shots: &[Shot]) -> String {
    let mut out = String::new();
    for (i, shot) in shots.iter().enumerate() {
        let start = u32::try_from(i).unwrap_or(u32::MAX).saturating_mul(CHAPTER_SECONDS);
        let _ = write!(
            out,
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            stamp(start, ','),
            stamp(start + CHAPTER_SECONDS, ','),
            shot.caption
        );
    }
    out
}

pub fn vtt(shots: &[Shot]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (i, shot) in shots.iter().enumerate() {
        let start = u32::try_from(i).unwrap_or(u32::MAX).saturating_mul(CHAPTER_SECONDS);
        let _ = write!(
            out,
            "{} --> {}\n{}\n\n",
            stamp(start, '.'),
            stamp(start + CHAPTER_SECONDS, '.'),
            shot.caption.replace("-->", "->")
        );
    }
    out
}

/// ffmpeg's concat demuxer ignores the last entry's duration, so the last frame
/// is listed twice.
pub fn ffconcat(shots: &[Shot]) -> String {
    let mut out = String::from("ffconcat version 1.0\n");
    for shot in shots {
        let _ = write!(
            out,
            "file '{}'\nduration {CHAPTER_SECONDS}\n",
            shot.frame.replace('\'', "'\\''")
        );
    }
    if let Some(last) = shots.last() {
        let _ = writeln!(out, "file '{}'", last.frame.replace('\'', "'\\''"));
    }
    out
}

pub struct StitchJob {
    pub concat: PathBuf,
    pub captions: PathBuf,
    pub video: PathBuf,
}

pub fn ffmpeg_args(job: &StitchJob) -> Vec<String> {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    [
        "-y",
        "-loglevel",
        "error",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        &path(&job.concat),
        "-i",
        &path(&job.captions),
        "-map",
        "0:v",
        "-map",
        "1:s",
        "-vf",
        "fps=30,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p",
        "-c:v",
        "libx264",
        "-crf",
        "20",
        "-c:s",
        "mov_text",
        "-metadata:s:s:0",
        "language=eng",
        "-movflags",
        "+faststart",
        &path(&job.video),
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

pub fn ffmpeg(job: &StitchJob) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(ffmpeg_args(job))
        .status()
        .map_err(|e| format!("failed to launch ffmpeg (is it installed?): {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg could not stitch {}", job.video.display()))
    }
}

pub struct ReelResult<'a> {
    pub reel: &'a PlanReel,
    pub outcome: Result<Vec<(&'a PlanLaunch, &'a PlanChapter, Filmed)>, String>,
}

/// Everything the runs left, turned into what the owner reads: per reel its
/// caption tracks and its video, then the index over all of them. A reel any of
/// whose runs is red gets no video — a video would claim the reel was filmed —
/// and the whole assembly is red, naming every red run.
pub fn assemble(
    out: &Path,
    plan: &Plan,
    stitch: &mut dyn FnMut(&StitchJob) -> Result<(), String>,
) -> Result<String, String> {
    let mut results = Vec::new();
    for reel in &plan.reels {
        let mut filmed = Vec::new();
        let mut failure = None;
        for launch in &reel.launches {
            match judge_run(out, &reel.id, launch) {
                Ok(chapters) => {
                    filmed
                        .extend(launch.chapters.iter().zip(chapters).map(|(c, f)| (launch, c, f)));
                }
                Err(why) => {
                    failure = Some(why);
                    break;
                }
            }
        }
        let outcome = match failure {
            Some(why) => Err(why),
            None => stitch_reel(out, reel, &filmed, stitch).map(|()| filmed),
        };
        results.push(ReelResult { reel, outcome });
    }
    std::fs::write(out.join("index.html"), index_html(&results))
        .map_err(|e| format!("could not write the index: {e}"))?;
    let red: Vec<&str> =
        results.iter().filter_map(|r| r.outcome.as_ref().err().map(String::as_str)).collect();
    if red.is_empty() {
        let frames: usize = results
            .iter()
            .filter_map(|r| r.outcome.as_ref().ok())
            .map(|f| f.iter().filter(|(_, _, f)| matches!(f, Filmed::Frame(_))).count())
            .sum();
        Ok(format!(
            "walkthrough complete - {} reels, {frames} chapters filmed, {}",
            results.len(),
            out.join("index.html").display()
        ))
    } else {
        Err(format!(
            "walkthrough red - {} of {} reels did not film:\n  {}",
            red.len(),
            results.len(),
            red.join("\n  ")
        ))
    }
}

fn stitch_reel(
    out: &Path,
    reel: &PlanReel,
    filmed: &[(&PlanLaunch, &PlanChapter, Filmed)],
    stitch: &mut dyn FnMut(&StitchJob) -> Result<(), String>,
) -> Result<(), String> {
    let total = filmed.len();
    let shots: Vec<Shot> = filmed
        .iter()
        .enumerate()
        .filter_map(|(i, (_, chapter, f))| match f {
            Filmed::Frame(frame) => {
                Some(Shot { frame: frame.clone(), caption: caption(i + 1, total, chapter) })
            }
            Filmed::Skipped(_) => None,
        })
        .collect();
    let dir = out.join(&reel.id);
    let write = |name: &str, body: String| {
        std::fs::write(dir.join(name), body).map_err(|e| format!("{}: {e}", reel.id))
    };
    write("frames.ffconcat", ffconcat(&shots))?;
    write("captions.srt", srt(&shots))?;
    std::fs::write(out.join(format!("{}.vtt", reel.id)), vtt(&shots))
        .map_err(|e| format!("{}: {e}", reel.id))?;
    stitch(&StitchJob {
        concat: dir.join("frames.ffconcat"),
        captions: dir.join("captions.srt"),
        video: out.join(format!("{}.mp4", reel.id)),
    })
    .map_err(|e| format!("{}: {e}", reel.id))
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const INDEX_STYLE: &str = r#"
:root { --bg:#f7f6f3; --paper:#fff; --ink:#1d1c1a; --mute:#6b6862; --line:#dedbd4; --ok:#2f7d4f; --warn:#b3261e; --chip:#ecebe6; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --bg:#1b1a18; --paper:#252321; --ink:#ecebe6; --mute:#a19d95; --line:#3a3733; --ok:#6fcf97; --warn:#f28b82; --chip:#33302c; } }
:root[data-theme="dark"] { --bg:#1b1a18; --paper:#252321; --ink:#ecebe6; --mute:#a19d95; --line:#3a3733; --ok:#6fcf97; --warn:#f28b82; --chip:#33302c; }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink); font:15px/1.5 system-ui, sans-serif; }
main { max-width: 1100px; margin: 0 auto; padding: 24px 16px 64px; }
h1 { font-size: 26px; margin: 0 0 4px; }
h2 { font-size: 20px; margin: 40px 0 8px; }
.mute { color: var(--mute); }
.red { color: var(--warn); }
nav { display:flex; flex-wrap:wrap; gap:6px; margin: 16px 0; }
nav a, .chip { background: var(--chip); border-radius: 6px; padding: 2px 8px; color: var(--ink); text-decoration: none; font-size: 13px; }
video, img { width: 100%; height: auto; border: 1px solid var(--line); border-radius: 6px; background: var(--paper); }
ol { list-style: none; padding: 0; display: grid; gap: 20px; }
li { background: var(--paper); border: 1px solid var(--line); border-radius: 8px; padding: 12px; }
li header { display:flex; flex-wrap:wrap; align-items:baseline; gap:8px; margin-bottom: 8px; }
label { display:flex; gap:8px; align-items:flex-start; margin-top: 8px; }
input[type=checkbox] { margin-top: 4px; width: 18px; height: 18px; flex: none; }
"#;

const INDEX_SCRIPT: &str = r"
for (const box of document.querySelectorAll('input[data-tick]')) {
  const key = 'walkthrough:' + box.dataset.tick;
  try { box.checked = localStorage.getItem(key) === '1'; } catch {}
  box.addEventListener('change', () => { try { localStorage.setItem(key, box.checked ? '1' : '0'); } catch {} });
}
";

pub fn index_html(results: &[ReelResult<'_>]) -> String {
    let mut page = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Setup walkthrough</title>\n<style>{INDEX_STYLE}</style>\n</head>\n<body>\n<main>\n<h1>Setup walkthrough</h1>\n<p class=\"mute\">Each chapter is one moment of the setup screen. Tick it when the frame shows what it should.</p>\n<nav>"
    );
    for result in results {
        let _ = write!(page, "<a href=\"#{0}\">{0}</a>", escape(&result.reel.id));
    }
    page.push_str("</nav>\n");
    for result in results {
        let id = escape(&result.reel.id);
        let _ = writeln!(page, "<h2 id=\"{id}\">{id}</h2>");
        match &result.outcome {
            Err(why) => {
                let _ = writeln!(page, "<p class=\"red\">not filmed: {}</p>", escape(why));
            }
            Ok(filmed) => {
                let _ = writeln!(
                    page,
                    "<video controls preload=\"metadata\" src=\"{id}.mp4\"><track kind=\"captions\" src=\"{id}.vtt\" srclang=\"en\" label=\"should show\" default></video>\n<ol>"
                );
                for (i, (launch, chapter, film)) in filmed.iter().enumerate() {
                    let chips: String = if chapter.findings.is_empty() {
                        "<span class=\"chip\">holds</span>".to_string()
                    } else {
                        chapter.findings.iter().fold(String::new(), |mut chips, f| {
                            let _ = write!(chips, "<span class=\"chip\">{}</span>", escape(f));
                            chips
                        })
                    };
                    let tick = escape(&format!("{}/{}/{}", result.reel.id, launch.id, chapter.id));
                    let _ = writeln!(
                        page,
                        "<li><header><strong>{}. {}</strong><span class=\"mute\">{} · {}</span>{chips}</header>",
                        i + 1,
                        escape(&chapter.title),
                        escape(&launch.id),
                        escape(&chapter.id)
                    );
                    match film {
                        Filmed::Frame(frame) => {
                            let _ = writeln!(
                                page,
                                "<img loading=\"lazy\" src=\"{id}/{}\" alt=\"{}\">",
                                escape(frame),
                                escape(&chapter.title)
                            );
                        }
                        Filmed::Skipped(why) => {
                            let _ =
                                writeln!(page, "<p class=\"mute\">skipped: {}</p>", escape(why));
                        }
                    }
                    let _ = writeln!(
                        page,
                        "<label><input type=\"checkbox\" data-tick=\"{tick}\"><span>should show: {}</span></label></li>",
                        escape(&chapter.expect)
                    );
                }
                page.push_str("</ol>\n");
            }
        }
    }
    let _ = writeln!(page, "</main>\n<script>{INDEX_SCRIPT}</script>\n</body>\n</html>");
    page
}

pub(crate) fn pnpm(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<bool, String> {
    let mut cmd = Command::new("pnpm");
    cmd.args(args).current_dir(dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.status()
        .map(|s| s.success())
        .map_err(|e| format!("failed to launch pnpm (is it installed?): {e}"))
}

pub(crate) fn required(ok: Result<bool, String>, what: &str) -> Result<(), String> {
    if ok? {
        Ok(())
    } else {
        Err(format!("{what} failed"))
    }
}

fn read_plan(e2e: &Path) -> Result<Plan, String> {
    let out = Command::new("pnpm")
        .args(["exec", "tsx", "walkthrough/plan.ts"])
        .current_dir(e2e)
        .output()
        .map_err(|e| format!("failed to launch pnpm (is it installed?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "the walkthrough plan did not print: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    parse_plan(&String::from_utf8_lossy(&out.stdout))
}

pub fn scenario_build(ui: &Path) -> Result<(), String> {
    required(pnpm(ui, &["install"], &[("VITE_E2E", "1")]), "pnpm install")?;
    required(pnpm(ui, &["build"], &[("VITE_E2E", "1")]), "pnpm build")?;
    let target = ui.join("src-tauri/target/scenario");
    let target = target.to_string_lossy();
    let build = [
        "tauri",
        "build",
        "--no-bundle",
        "--config",
        "src-tauri/tauri.conf.e2e.json",
        "--config",
        "src-tauri/tauri.conf.walkthrough.json",
        "--features",
        "scenario",
    ];
    required(pnpm(ui, &build, &[("CARGO_TARGET_DIR", &target)]), "the scenario build")
}

pub fn run(ui: &Path, repo: &Path) -> Result<(i32, String), String> {
    let e2e = ui.join("e2e");
    required(pnpm(&e2e, &["install"], &[]), "pnpm install in ui/e2e")?;
    let plan = read_plan(&e2e)?;
    judge_plan(&plan)?;

    scenario_build(ui)?;

    let out = repo.join("target/walkthrough");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let out_str = out.to_string_lossy();
    for reel in &plan.reels {
        for launch in &reel.launches {
            let id = format!("{}/{}", reel.id, launch.id);
            let conf = ["exec", "wdio", "run", "walkthrough/walkthrough.conf.ts"];
            // A red run is judged with the rest, from what it left: the verdict
            // is the manifest against the script, never wdio's exit alone.
            pnpm(&e2e, &conf, &[(RUN_VAR, &id), (OUT_VAR, &out_str)])?;
        }
    }
    assemble(&out, &plan, &mut |job| ffmpeg(job)).map(|summary| (0, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(id: &str, findings: &[&str]) -> PlanChapter {
        PlanChapter {
            id: id.into(),
            title: format!("{id} title"),
            expect: format!("{id} shows"),
            findings: findings.iter().map(ToString::to_string).collect(),
        }
    }

    fn full_plan() -> Plan {
        let mut reels = Vec::new();
        for host in HOSTS {
            for platform in PLATFORMS {
                reels.push(PlanReel {
                    id: format!("{host}-{platform}"),
                    host: host.into(),
                    platform: platform.into(),
                    launches: vec![PlanLaunch {
                        id: "walk".into(),
                        chapters: vec![chapter("a", &[])],
                    }],
                });
            }
        }
        let every: Vec<String> = FINDINGS
            .map(|n| format!("R{n}"))
            .map(|f| superseded_by(&f).map_or(f, str::to_string))
            .collect();
        reels[0].launches[0].chapters[0].findings = every;
        Plan { reels }
    }

    #[test]
    fn a_plan_naming_every_finding_on_every_reel_is_accepted() {
        assert_eq!(judge_plan(&full_plan()), Ok(()));
    }

    #[test]
    fn a_finding_no_chapter_names_is_refused_by_name() {
        let mut plan = full_plan();
        plan.reels[0].launches[0].chapters[0].findings.retain(|f| f != "R17");
        let err = judge_plan(&plan).unwrap_err();
        assert!(err.contains("no chapter names R17"), "{err}");
    }

    #[test]
    fn a_finding_the_review_does_not_have_is_refused() {
        let mut plan = full_plan();
        plan.reels[1].launches[0].chapters[0].findings.push("R31".into());
        assert!(judge_plan(&plan).unwrap_err().contains("R31, which is no finding"));
    }

    #[test]
    fn a_superseded_finding_is_filmed_as_its_successor() {
        let mut plan = full_plan();
        let findings = &mut plan.reels[0].launches[0].chapters[0].findings;
        assert!(findings.contains(&"W3.D8".to_string()));
        findings.retain(|f| f != "W3.D8");
        let err = judge_plan(&plan).unwrap_err();
        assert!(err.contains("no chapter names W3.D8"), "{err}");
    }

    #[test]
    fn naming_a_superseded_finding_is_refused_by_its_successor() {
        let mut plan = full_plan();
        plan.reels[1].launches[0].chapters[0].findings.push("R20".into());
        assert!(judge_plan(&plan).unwrap_err().contains("R20, which W3.D8 superseded"));
    }

    #[test]
    fn a_revamp_finding_counts_only_when_listed() {
        let mut plan = full_plan();
        plan.reels[1].launches[0].chapters[0].findings.push("W3.D6".into());
        plan.reels[1].launches[0].chapters[0].findings.push("W16.I1".into());
        plan.reels[1].launches[0].chapters[0].findings.push("W17.I5".into());
        assert_eq!(judge_plan(&plan), Ok(()));
        plan.reels[2].launches[0].chapters[0].findings.push("W3.D9".into());
        assert!(judge_plan(&plan).unwrap_err().contains("W3.D9, which is no finding"));
    }

    #[test]
    fn a_missing_host_platform_reel_is_refused() {
        let mut plan = full_plan();
        plan.reels.retain(|r| r.id != "linux-ios-sim");
        assert!(judge_plan(&plan).unwrap_err().contains("no reel linux-ios-sim"));
    }

    #[test]
    fn duplicate_ids_and_empty_expectations_are_refused() {
        let mut plan = full_plan();
        plan.reels[2].launches[0].chapters.push(chapter("a", &[]));
        plan.reels[3].launches[0].chapters[0].expect = "  ".into();
        plan.reels[4]
            .launches
            .push(PlanLaunch { id: "walk".into(), chapters: vec![chapter("b", &[])] });
        let err = judge_plan(&plan).unwrap_err();
        assert!(err.contains("has two chapters a"), "{err}");
        assert!(err.contains("expects nothing"), "{err}");
        assert!(err.contains("has two launches walk"), "{err}");
    }

    fn launch(ids: &[&str]) -> PlanLaunch {
        PlanLaunch { id: "walk".into(), chapters: ids.iter().map(|id| chapter(id, &[])).collect() }
    }

    fn run_dir(manifest: &str, frames: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let reel = dir.path().join("macos-this");
        std::fs::create_dir_all(&reel).unwrap();
        std::fs::write(reel.join("walk.json"), manifest).unwrap();
        for frame in frames {
            std::fs::write(reel.join(frame), b"png").unwrap();
        }
        dir
    }

    #[test]
    fn a_run_that_framed_or_skipped_every_scripted_chapter_is_green() {
        let dir = run_dir(
            r#"{"chapters":[{"id":"a","frame":"a.png"},{"id":"b","skipped":"not offered"}]}"#,
            &["a.png"],
        );
        assert_eq!(
            judge_run(dir.path(), "macos-this", &launch(&["a", "b"])).unwrap(),
            vec![Filmed::Frame("a.png".into()), Filmed::Skipped("not offered".into())]
        );
    }

    #[test]
    fn a_run_that_stopped_early_names_the_chapter_it_never_filmed() {
        let dir = run_dir(r#"{"chapters":[{"id":"a","frame":"a.png"}]}"#, &["a.png"]);
        let err = judge_run(dir.path(), "macos-this", &launch(&["a", "b"])).unwrap_err();
        assert_eq!(err, "macos-this/walk: stopped before chapter 2 (b), 1 of 2 filmed");
    }

    #[test]
    fn a_run_with_no_manifest_filmed_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let err = judge_run(dir.path(), "macos-this", &launch(&["a"])).unwrap_err();
        assert!(err.contains("left no manifest"), "{err}");
    }

    #[test]
    fn a_recorded_frame_missing_from_disk_is_red() {
        let dir = run_dir(r#"{"chapters":[{"id":"a","frame":"a.png"}]}"#, &[]);
        let err = judge_run(dir.path(), "macos-this", &launch(&["a"])).unwrap_err();
        assert!(err.contains("no frame at a.png"), "{err}");
    }

    #[test]
    fn a_chapter_out_of_order_or_without_frame_or_reason_is_red() {
        let dir = run_dir(r#"{"chapters":[{"id":"b","frame":"b.png"}]}"#, &["b.png"]);
        assert!(judge_run(dir.path(), "macos-this", &launch(&["a"]))
            .unwrap_err()
            .contains("is b, not a"));
        let dir = run_dir(r#"{"chapters":[{"id":"a","skipped":" "}]}"#, &[]);
        assert!(judge_run(dir.path(), "macos-this", &launch(&["a"]))
            .unwrap_err()
            .contains("neither a frame nor a reason"));
    }

    #[test]
    fn a_caption_says_what_the_screen_should_show_and_names_its_findings() {
        let c = chapter("cert-done", &["R12", "R14"]);
        assert_eq!(
            caption(4, 15, &c),
            "4/15 · cert-done title [R12 · R14]\nshould show: cert-done shows"
        );
        assert!(caption(1, 2, &chapter("x", &[])).contains("[holds]"));
    }

    fn shots(chapters: &[PlanChapter]) -> Vec<Shot> {
        chapters
            .iter()
            .enumerate()
            .map(|(i, c)| Shot {
                frame: format!("{}.png", c.id),
                caption: caption(i + 1, chapters.len(), c),
            })
            .collect()
    }

    #[test]
    fn each_chapter_holds_the_screen_for_the_same_span_in_both_caption_tracks() {
        let chapters = [chapter("a", &[]), chapter("b", &["R1"])];
        let shots = shots(&chapters);
        assert_eq!(
            srt(&shots),
            "1\n00:00:00,000 --> 00:00:05,000\n1/2 · a title [holds]\nshould show: a shows\n\n2\n00:00:05,000 --> 00:00:10,000\n2/2 · b title [R1]\nshould show: b shows\n\n"
        );
        let vtt = vtt(&shots);
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:05.000\n"), "{vtt}");
        assert!(vtt.contains("00:00:05.000 --> 00:00:10.000\n2/2"), "{vtt}");
    }

    #[test]
    fn a_caption_cannot_end_its_own_cue() {
        let mut c = chapter("a", &[]);
        c.expect = "a --> b".into();
        let shots = [Shot { frame: "a.png".into(), caption: caption(1, 1, &c) }];
        assert!(!vtt(&shots).split_once("\n\n").unwrap().1.contains("a --> b"));
    }

    #[test]
    fn the_concat_list_repeats_the_last_frame_for_its_duration_to_count() {
        let chapters = [chapter("a", &[]), chapter("b", &[])];
        assert_eq!(
            ffconcat(&shots(&chapters)),
            "ffconcat version 1.0\nfile 'a.png'\nduration 5\nfile 'b.png'\nduration 5\nfile 'b.png'\n"
        );
    }

    #[test]
    fn ffmpeg_reads_the_list_and_the_captions_and_writes_an_even_sized_h264() {
        let args = ffmpeg_args(&StitchJob {
            concat: "r/frames.ffconcat".into(),
            captions: "r/captions.srt".into(),
            video: "r.mp4".into(),
        });
        let joined = args.join(" ");
        assert!(
            joined.contains("-f concat -safe 0 -i r/frames.ffconcat -i r/captions.srt"),
            "{joined}"
        );
        assert!(joined.contains("-c:v libx264") && joined.contains("-c:s mov_text"), "{joined}");
        assert!(joined.contains("trunc(iw/2)*2"), "{joined}");
        assert_eq!(args.last().map(String::as_str), Some("r.mp4"));
    }

    #[test]
    fn the_index_escapes_captions_and_carries_one_tick_box_per_chapter() {
        let mut c = chapter("a", &["R1"]);
        c.expect = "<b>\"no\" & more</b>".into();
        let skipped = chapter("b", &[]);
        let l = launch(&["a", "b"]);
        let reel = PlanReel {
            id: "macos-this".into(),
            host: "macos".into(),
            platform: "this".into(),
            launches: vec![],
        };
        let failed = PlanReel {
            id: "linux-ios".into(),
            host: "linux".into(),
            platform: "ios".into(),
            launches: vec![],
        };
        let page = index_html(&[
            ReelResult {
                reel: &reel,
                outcome: Ok(vec![
                    (&l, &c, Filmed::Frame("walk-01-a.png".into())),
                    (&l, &skipped, Filmed::Skipped("not offered".into())),
                ]),
            },
            ReelResult { reel: &failed, outcome: Err("linux-ios/walk: <stopped>".into()) },
        ]);
        assert!(page.contains("&lt;b&gt;&quot;no&quot; &amp; more&lt;/b&gt;"));
        assert!(!page.contains("<b>\"no\""));
        assert_eq!(page.matches("type=\"checkbox\"").count(), 2);
        assert!(page.contains("data-tick=\"macos-this/walk/a\""));
        assert!(page.contains("src=\"macos-this/walk-01-a.png\""));
        assert!(page.contains("skipped: not offered"));
        assert!(page.contains("not filmed: linux-ios/walk: &lt;stopped&gt;"));
        assert!(page.contains("src=\"macos-this.mp4\"") && !page.contains("linux-ios.mp4"));
    }
}
