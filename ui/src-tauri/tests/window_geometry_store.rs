use trawler_ui_lib::window_geometry::{
    placed, read_records, write_records, Frame, Geometry, MinSize, Records, WorkArea,
};

const LAPTOP: WorkArea = WorkArea { x: 0, y: 50, width: 3024, height: 1800, scale: 2.0 };
const SIDE: WorkArea = WorkArea { x: 3024, y: 0, width: 1920, height: 1080, scale: 1.0 };
const MAIN_MIN: MinSize = MinSize { width: 850.0, height: 600.0 };

#[test]
fn each_saved_window_reopens_where_it_was_left_and_a_broken_file_reopens_none() {
    let dir = std::env::temp_dir().join(format!("trawler-it-windows-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let saved = Records::from([
        (
            "main".to_owned(),
            Geometry { x: 120, y: 90, width: 2400, height: 1500, maximized: false },
        ),
        (
            "inspector-1".to_owned(),
            Geometry { x: 3300, y: 60, width: 1200, height: 800, maximized: true },
        ),
    ]);
    write_records(&dir, &saved).unwrap();

    let restored: Vec<_> = read_records(&dir)
        .into_iter()
        .map(|(label, record)| (label, placed(record, Frame::default(), &[LAPTOP, SIDE], MAIN_MIN)))
        .collect();
    std::fs::write(dir.join("windows.json"), b"{\"main\":").unwrap();
    let broken = read_records(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    let expected: Vec<_> = saved.into_iter().map(|(label, record)| (label, Some(record))).collect();
    assert_eq!(restored, expected);
    assert_eq!(broken, Records::new());
}
