use trawler_ui_lib::layout::{read_layout, write_layout, Layout};

const WIRE: &str = include_str!("../../src/layoutWire.json");

#[derive(serde::Deserialize)]
struct LayoutWriteArgs {
    layout: Layout,
}

#[test]
fn the_layout_the_app_sends_is_the_layout_it_reads_back() {
    let dir = std::env::temp_dir().join(format!("trawler-it-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let wire: serde_json::Value = serde_json::from_str(WIRE).unwrap();
    let args: LayoutWriteArgs =
        serde_json::from_value(serde_json::json!({ "layout": wire.clone() })).unwrap();
    write_layout(&dir, &args.layout).unwrap();

    let read = read_layout(&dir).expect("the written layout reads back");
    let fresh = read_layout(&dir.join("never-written"));
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(serde_json::to_value(read).unwrap(), wire);
    assert_eq!(serde_json::to_value(fresh).unwrap(), serde_json::Value::Null);
}
