use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Window, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

pub type Records = BTreeMap<String, Geometry>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Moved { x: i32, y: i32 },
    Resized { width: u32, height: u32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub maximized: bool,
    pub minimized: bool,
}

fn records_path(config_dir: &Path) -> PathBuf {
    config_dir.join("windows.json")
}

pub fn read_records(config_dir: &Path) -> Records {
    std::fs::read(records_path(config_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn write_records(config_dir: &Path, records: &Records) -> std::io::Result<()> {
    crate::layout::replace_whole(&records_path(config_dir), &serde_json::to_vec(records)?)
}

pub fn placed(
    record: Geometry,
    frame: Frame,
    monitors: &[WorkArea],
    min: MinSize,
) -> Option<Geometry> {
    let outer_width = record.width.saturating_add(frame.width);
    let outer_height = record.height.saturating_add(frame.height);
    let overlapping = monitors
        .iter()
        .map(|area| (overlap(area, record.x, record.y, outer_width, outer_height), area))
        .filter(|(shared, _)| *shared > 0)
        .max_by_key(|(shared, _)| *shared)
        .map(|(_, area)| area);
    let (host, centred) = match overlapping {
        Some(area) => (area, false),
        None => (monitors.first()?, true),
    };

    let width = record
        .width
        .min(host.width.saturating_sub(frame.width))
        .max(physical(min.width, host.scale));
    let height = record
        .height
        .min(host.height.saturating_sub(frame.height))
        .max(physical(min.height, host.scale));
    let outer_width = width.saturating_add(frame.width);
    let outer_height = height.saturating_add(frame.height);
    let (x, y) = if centred {
        (centre(host.x, host.width, outer_width), centre(host.y, host.height, outer_height))
    } else {
        (
            inside(record.x, host.x, host.width, outer_width),
            inside(record.y, host.y, host.height, outer_height),
        )
    };
    Some(Geometry { x, y, width, height, maximized: record.maximized })
}

pub fn tracked(mut record: Geometry, change: Change, shown: Shown) -> Geometry {
    if shown.minimized {
        return record;
    }
    record.maximized = shown.maximized;
    if !shown.maximized {
        match change {
            Change::Moved { x, y } => (record.x, record.y) = (x, y),
            Change::Resized { width, height } => (record.width, record.height) = (width, height),
        }
    }
    record
}

fn overlap(area: &WorkArea, x: i32, y: i32, width: u32, height: u32) -> i64 {
    let span = |start: i32, len: u32, area_start: i32, area_len: u32| {
        let end =
            (i64::from(start) + i64::from(len)).min(i64::from(area_start) + i64::from(area_len));
        (end - i64::from(start).max(i64::from(area_start))).max(0)
    };
    span(x, width, area.x, area.width) * span(y, height, area.y, area.height)
}

fn inside(start: i32, area_start: i32, area_len: u32, len: u32) -> i32 {
    let last = i64::from(area_start) + i64::from(area_len) - i64::from(len);
    i32::try_from(i64::from(start).min(last).max(i64::from(area_start))).unwrap_or(area_start)
}

fn centre(area_start: i32, area_len: u32, len: u32) -> i32 {
    let offset = (i64::from(area_len) - i64::from(len)).max(0) / 2;
    i32::try_from(i64::from(area_start) + offset).unwrap_or(area_start)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn physical(points: f64, scale: f64) -> u32 {
    (points * scale).ceil().max(0.0) as u32
}

struct Tracker(Mutex<Records>);

impl Tracker {
    fn records(&self) -> std::sync::MutexGuard<'_, Records> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn install(app: &AppHandle) {
    let records = crate::setup::config_dir(app).map(|dir| read_records(&dir)).unwrap_or_default();
    app.manage(Tracker(Mutex::new(records)));
    for window in app.webview_windows().values() {
        restore(&window.as_ref().window());
    }
}

pub fn restore(window: &Window) {
    let Some(tracker) = window.try_state::<Tracker>() else { return };
    let saved = tracker.records().get(window.label()).copied();
    let applied = saved.and_then(|record| {
        placed(record, frame_of(window), &work_areas(window), min_size_of(window))
    });
    if let Some(geometry) = applied {
        apply(window, geometry);
    }
    if let Some(record) = applied.or_else(|| current(window)) {
        tracker.records().insert(window.label().to_owned(), record);
    }
}

pub fn track(window: &Window, event: &WindowEvent) {
    let change = match event {
        WindowEvent::Moved(position) => Change::Moved { x: position.x, y: position.y },
        WindowEvent::Resized(size) => Change::Resized { width: size.width, height: size.height },
        WindowEvent::Destroyed => return save(window.app_handle()),
        _ => return,
    };
    let Some(tracker) = window.try_state::<Tracker>() else { return };
    let shown = Shown {
        maximized: window.is_maximized().unwrap_or(false),
        minimized: window.is_minimized().unwrap_or(false),
    };
    let mut records = tracker.records();
    if let Some(record) = records.get_mut(window.label()) {
        *record = tracked(*record, change, shown);
    }
}

pub fn save(app: &AppHandle) {
    let Some(tracker) = app.try_state::<Tracker>() else { return };
    let records = tracker.records().clone();
    let written = crate::setup::config_dir(app)
        .and_then(|dir| write_records(&dir, &records).map_err(|error| error.to_string()));
    if let Err(error) = written {
        eprintln!("saving the window geometry failed: {error}");
    }
}

fn apply(window: &Window, geometry: Geometry) {
    let applied = window
        .set_size(PhysicalSize::new(geometry.width, geometry.height))
        .and_then(|()| window.set_position(PhysicalPosition::new(geometry.x, geometry.y)))
        .and_then(|()| if geometry.maximized { window.maximize() } else { Ok(()) });
    if let Err(error) = applied {
        eprintln!("restoring the window geometry of {} failed: {error}", window.label());
    }
}

fn current(window: &Window) -> Option<Geometry> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(Geometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: window.is_maximized().unwrap_or(false),
    })
}

fn frame_of(window: &Window) -> Frame {
    match (window.outer_size(), window.inner_size()) {
        (Ok(outer), Ok(inner)) => Frame {
            width: outer.width.saturating_sub(inner.width),
            height: outer.height.saturating_sub(inner.height),
        },
        _ => Frame::default(),
    }
}

fn work_areas(window: &Window) -> Vec<WorkArea> {
    let primary = window.primary_monitor().ok().flatten().map(|monitor| *monitor.position());
    let mut monitors = window.available_monitors().unwrap_or_default();
    monitors.sort_by_key(|monitor| Some(*monitor.position()) != primary);
    monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            WorkArea {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
                scale: monitor.scale_factor(),
            }
        })
        .collect()
}

fn min_size_of(window: &Window) -> MinSize {
    window
        .app_handle()
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == window.label())
        .map(|config| MinSize {
            width: config.min_width.unwrap_or_default(),
            height: config.min_height.unwrap_or_default(),
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("trawler-test-windows-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const LAPTOP: WorkArea = WorkArea { x: 0, y: 50, width: 3024, height: 1800, scale: 2.0 };
    const SIDE: WorkArea = WorkArea { x: 3024, y: 0, width: 1920, height: 1080, scale: 1.0 };
    const NO_FRAME: Frame = Frame { width: 0, height: 0 };
    const MIN: MinSize = MinSize { width: 850.0, height: 600.0 };

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Geometry {
        Geometry { x, y, width, height, maximized: false }
    }

    #[test]
    fn a_window_inside_its_monitor_returns_unchanged() {
        let saved = rect(100, 200, 2000, 1400);
        assert_eq!(placed(saved, NO_FRAME, &[LAPTOP, SIDE], MIN), Some(saved));
        let beside = rect(3100, 40, 1000, 700);
        assert_eq!(placed(beside, NO_FRAME, &[LAPTOP, SIDE], MinSize::default()), Some(beside));
    }

    #[test]
    fn a_window_on_a_gone_monitor_is_centred_on_the_primary_at_its_saved_size() {
        let saved = rect(3200, 100, 1800, 1300);
        assert_eq!(placed(saved, NO_FRAME, &[LAPTOP], MIN), Some(rect(612, 300, 1800, 1300)));
    }

    #[test]
    fn a_window_larger_than_its_monitor_shrinks_into_the_work_area() {
        let saved = rect(3000, 0, 2560, 1440);
        assert_eq!(
            placed(saved, NO_FRAME, &[SIDE], MinSize::default()),
            Some(rect(3024, 0, 1920, 1080))
        );
    }

    #[test]
    fn a_window_hanging_off_an_edge_moves_fully_inside() {
        assert_eq!(
            placed(rect(-300, 0, 1800, 1200), NO_FRAME, &[LAPTOP], MIN),
            Some(rect(0, 50, 1800, 1200))
        );
        assert_eq!(
            placed(rect(2500, 1500, 1800, 1200), NO_FRAME, &[LAPTOP], MIN),
            Some(rect(1224, 650, 1800, 1200))
        );
    }

    #[test]
    fn a_window_below_its_minimum_is_raised_in_its_monitors_pixels() {
        assert_eq!(
            placed(rect(0, 50, 900, 700), NO_FRAME, &[LAPTOP], MIN),
            Some(rect(0, 50, 1700, 1200))
        );
        assert_eq!(
            placed(rect(3024, 0, 400, 300), NO_FRAME, &[SIDE], MIN),
            Some(rect(3024, 0, 850, 600))
        );
    }

    #[test]
    fn a_window_straddling_two_monitors_is_hosted_by_the_larger_share() {
        let straddling = rect(2800, 100, 1000, 700);
        assert_eq!(
            placed(straddling, NO_FRAME, &[LAPTOP, SIDE], MinSize::default()),
            Some(rect(3024, 100, 1000, 700))
        );
    }

    #[test]
    fn no_known_monitor_places_nothing() {
        assert_eq!(placed(rect(0, 0, 1440, 800), NO_FRAME, &[], MIN), None);
    }

    #[test]
    fn the_frame_stays_inside_the_work_area_too() {
        let frame = Frame { width: 0, height: 56 };
        assert_eq!(
            placed(rect(0, 1000, 1800, 1800), frame, &[LAPTOP], MIN),
            Some(rect(0, 50, 1800, 1744))
        );
    }

    #[test]
    fn maximized_is_carried_through_with_the_rect_to_return_to() {
        let saved = Geometry { maximized: true, ..rect(-500, 50, 1800, 1200) };
        assert_eq!(
            placed(saved, NO_FRAME, &[LAPTOP], MIN),
            Some(Geometry { maximized: true, ..rect(0, 50, 1800, 1200) })
        );
    }

    #[test]
    fn moves_and_resizes_update_the_record_only_while_the_window_is_normal() {
        let record = rect(10, 20, 1800, 1200);
        let normal = Shown::default();
        let maximized = Shown { maximized: true, minimized: false };
        let minimized = Shown { maximized: false, minimized: true };

        assert_eq!(
            tracked(record, Change::Moved { x: 40, y: 60 }, normal),
            rect(40, 60, 1800, 1200)
        );
        assert_eq!(
            tracked(record, Change::Resized { width: 2000, height: 1300 }, normal),
            rect(10, 20, 2000, 1300)
        );
        assert_eq!(
            tracked(record, Change::Resized { width: 3024, height: 1800 }, maximized),
            Geometry { maximized: true, ..record }
        );
        assert_eq!(tracked(record, Change::Moved { x: -32000, y: -32000 }, minimized), record);
        assert_eq!(
            tracked(
                Geometry { maximized: true, ..record },
                Change::Resized { width: 1800, height: 1200 },
                normal
            ),
            record
        );
    }

    fn two_labels() -> Records {
        Records::from([
            ("main".to_owned(), rect(100, 200, 2000, 1400)),
            ("inspector-1".to_owned(), Geometry { maximized: true, ..rect(3100, 40, 1000, 700) }),
        ])
    }

    fn saved(dir: &Path, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(records_path(dir), text).unwrap();
    }

    #[test]
    fn written_records_read_back_unchanged() {
        let dir = TempDir::new("round-trip");
        write_records(&dir.0, &two_labels()).unwrap();
        assert_eq!(read_records(&dir.0), two_labels());
    }

    #[test]
    fn no_dir_and_no_file_read_as_no_records() {
        let dir = TempDir::new("absent");
        assert_eq!(read_records(&dir.0), Records::new());
        std::fs::create_dir_all(&dir.0).unwrap();
        assert_eq!(read_records(&dir.0), Records::new());
    }

    #[test]
    fn bytes_that_are_not_json_read_as_no_records() {
        let dir = TempDir::new("corrupt");
        saved(&dir.0, r#"{"main":{"x":1"#);
        assert_eq!(read_records(&dir.0), Records::new());
        std::fs::write(records_path(&dir.0), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_records(&dir.0), Records::new());
    }

    #[test]
    fn any_other_shape_reads_as_no_records() {
        let dir = TempDir::new("shape");
        for shape in [
            r#"{"main":{"x":1,"y":2,"width":3,"height":4}}"#,
            r#"{"main":{"x":1,"y":2,"width":3,"height":4,"maximized":false,"fullscreen":false}}"#,
            r#"{"main":{"x":"1","y":2,"width":3,"height":4,"maximized":false}}"#,
            r#"{"main":{"x":1,"y":2,"width":-3,"height":4,"maximized":false}}"#,
            r#"{"main":{"x":1,"y":2,"width":3,"height":4,"maximized":false},"inspector":null}"#,
            r#"[{"x":1,"y":2,"width":3,"height":4,"maximized":false}]"#,
            "null",
        ] {
            saved(&dir.0, shape);
            assert_eq!(read_records(&dir.0), Records::new(), "{shape}");
        }
    }

    #[test]
    fn a_write_makes_the_dir_replaces_the_file_and_leaves_no_staged_copy() {
        let dir = TempDir::new("replace");
        let nested = dir.0.join("nested");
        write_records(&nested, &two_labels()).unwrap();
        let only_main = Records::from([("main".to_owned(), rect(0, 0, 1700, 1200))]);
        write_records(&nested, &only_main).unwrap();
        assert_eq!(read_records(&nested), only_main);
        assert!(!nested.join("windows.json.staged").exists());
    }
}
