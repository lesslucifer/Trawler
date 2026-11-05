use std::path::{Path, PathBuf};

use xtask::walkthrough::{assemble, parse_plan, Plan, StitchJob};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x78, 0xe1, 0xab, 0x05,
    0x00, 0x03, 0x80, 0x01, 0x60, 0x37, 0x6e, 0x7b, 0x87, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

// Two reels, the first with two launches the way macos-this opens with its
// launch runs, the second with a chapter its host does not offer.
fn plan() -> Plan {
    parse_plan(
        r#"{"reels":[
          {"id":"macos-this","host":"macos","platform":"this","launches":[
            {"id":"slow-core","chapters":[{"id":"checking","title":"Launch","expect":"no setup paints","findings":["R1"]}]},
            {"id":"walk","chapters":[
              {"id":"first-paint","title":"First paint","expect":"setup","findings":[]},
              {"id":"cert-done","title":"Cert done","expect":"no combined button","findings":["R12"]}]}]},
          {"id":"windows-ios-sim","host":"windows","platform":"ios-sim","launches":[
            {"id":"walk","chapters":[
              {"id":"picker","title":"Picker","expect":"no iOS Simulator","findings":["R8"]},
              {"id":"chosen","title":"Chosen","expect":"proxy steps","findings":[]}]}]}]}"#,
    )
    .unwrap()
}

fn film(out: &Path, reel: &str, launch: &str, chapters: &[(&str, Option<&str>)]) {
    let dir = out.join(reel);
    std::fs::create_dir_all(&dir).unwrap();
    let mut recorded = Vec::new();
    for (i, (id, skipped)) in chapters.iter().enumerate() {
        if let Some(why) = skipped {
            recorded.push(format!(r#"{{"id":"{id}","skipped":"{why}"}}"#));
        } else {
            let frame = format!("{launch}-{:02}-{id}.png", i + 1);
            std::fs::write(dir.join(&frame), PNG_1X1).unwrap();
            recorded.push(format!(r#"{{"id":"{id}","frame":"{frame}"}}"#));
        }
    }
    std::fs::write(
        dir.join(format!("{launch}.json")),
        format!(r#"{{"reel":"{reel}","launch":"{launch}","chapters":[{}]}}"#, recorded.join(",")),
    )
    .unwrap();
}

fn film_everything(out: &Path) {
    film(out, "macos-this", "slow-core", &[("checking", None)]);
    film(out, "macos-this", "walk", &[("first-paint", None), ("cert-done", None)]);
    film(
        out,
        "windows-ios-sim",
        "walk",
        &[("picker", None), ("chosen", Some("not offered on windows"))],
    );
}

struct Stitched {
    videos: Vec<PathBuf>,
}

fn stitcher(stitched: &mut Stitched) -> impl FnMut(&StitchJob) -> Result<(), String> + '_ {
    |job| {
        assert!(
            job.concat.is_file() && job.captions.is_file(),
            "stitched before its inputs existed"
        );
        std::fs::write(&job.video, b"mp4").unwrap();
        stitched.videos.push(job.video.clone());
        Ok(())
    }
}

#[test]
fn every_reel_that_filmed_gets_a_video_caption_tracks_and_a_place_in_the_index() {
    let out = tempfile::tempdir().unwrap();
    film_everything(out.path());
    let plan = plan();
    let mut stitched = Stitched { videos: Vec::new() };

    let summary = assemble(out.path(), &plan, &mut stitcher(&mut stitched)).unwrap();

    assert!(summary.starts_with("walkthrough complete - 2 reels, 4 chapters filmed"), "{summary}");
    assert_eq!(
        stitched.videos,
        vec![out.path().join("macos-this.mp4"), out.path().join("windows-ios-sim.mp4")]
    );
    for reel in ["macos-this", "windows-ios-sim"] {
        assert!(out.path().join(format!("{reel}.vtt")).is_file(), "{reel} has no caption track");
    }

    let concat = std::fs::read_to_string(out.path().join("macos-this/frames.ffconcat")).unwrap();
    let listed: Vec<&str> = concat.lines().filter(|l| l.starts_with("file ")).collect();
    assert_eq!(
        listed,
        vec![
            "file 'slow-core-01-checking.png'",
            "file 'walk-01-first-paint.png'",
            "file 'walk-02-cert-done.png'",
            "file 'walk-02-cert-done.png'"
        ],
        "the launches' frames follow each other in script order"
    );
    let vtt = std::fs::read_to_string(out.path().join("macos-this.vtt")).unwrap();
    assert!(vtt.contains("3/3 · Cert done [R12]\nshould show: no combined button"), "{vtt}");

    let index = std::fs::read_to_string(out.path().join("index.html")).unwrap();
    assert_eq!(
        index.matches("type=\"checkbox\"").count(),
        5,
        "one tick-box per chapter, skips included"
    );
    assert!(index.contains("src=\"macos-this/walk-02-cert-done.png\""));
    assert!(index.contains("skipped: not offered on windows"));
    assert!(index.contains("<video") && index.contains("src=\"windows-ios-sim.vtt\""));
}

#[test]
fn a_chapter_whose_anchor_never_painted_turns_the_assembly_red_and_gets_its_reel_no_video() {
    let out = tempfile::tempdir().unwrap();
    film_everything(out.path());
    film(out.path(), "macos-this", "walk", &[("first-paint", None)]);
    let plan = plan();
    let mut stitched = Stitched { videos: Vec::new() };

    let err = assemble(out.path(), &plan, &mut stitcher(&mut stitched)).unwrap_err();

    assert!(err.starts_with("walkthrough red - 1 of 2 reels did not film"), "{err}");
    assert!(
        err.contains("macos-this/walk: stopped before chapter 2 (cert-done), 1 of 2 filmed"),
        "{err}"
    );
    assert_eq!(stitched.videos, vec![out.path().join("windows-ios-sim.mp4")]);
    let index = std::fs::read_to_string(out.path().join("index.html")).unwrap();
    assert!(index.contains("not filmed: macos-this/walk: stopped before chapter 2"), "{index}");
    assert!(!index.contains("src=\"macos-this.mp4\""));
}

#[test]
fn a_launch_that_never_ran_is_a_reel_that_did_not_film() {
    let out = tempfile::tempdir().unwrap();
    film(out.path(), "macos-this", "walk", &[("first-paint", None), ("cert-done", None)]);
    film(out.path(), "windows-ios-sim", "walk", &[("picker", None), ("chosen", None)]);
    let plan = plan();
    let mut stitched = Stitched { videos: Vec::new() };

    let err = assemble(out.path(), &plan, &mut stitcher(&mut stitched)).unwrap_err();

    assert!(err.contains("macos-this/slow-core: the run left no manifest"), "{err}");
}

#[test]
fn a_stitch_that_fails_is_red_for_its_reel() {
    let out = tempfile::tempdir().unwrap();
    film_everything(out.path());
    let plan = plan();

    let err = assemble(out.path(), &plan, &mut |job: &StitchJob| {
        if job.video.ends_with("windows-ios-sim.mp4") {
            Err("ffmpeg could not stitch".to_string())
        } else {
            Ok(())
        }
    })
    .unwrap_err();

    assert!(err.contains("windows-ios-sim: ffmpeg could not stitch"), "{err}");
}
