use trawler_ui_lib::view_prefs::{read_view_prefs, write_view_prefs, ViewPrefs};

const WIRE: &str = include_str!("../../src/viewPrefsWire.json");

#[derive(serde::Deserialize)]
struct ViewPrefsWriteArgs {
    prefs: ViewPrefs,
}

#[test]
fn the_prefs_the_app_sends_read_back_beside_another_versions_field_and_a_broken_file_reads_as_defaults(
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("view-prefs.json"), r#"{"query":"host:api"}"#).unwrap();

    let wire: serde_json::Value = serde_json::from_str(WIRE).unwrap();
    let args: ViewPrefsWriteArgs =
        serde_json::from_value(serde_json::json!({ "prefs": wire.clone() })).unwrap();
    write_view_prefs(dir.path(), &args.prefs).unwrap();

    let mut expected = wire;
    expected["query"] = "host:api".into();
    assert_eq!(serde_json::to_value(read_view_prefs(dir.path())).unwrap(), expected);

    std::fs::write(dir.path().join("view-prefs.json"), b"{\"leg\":").unwrap();
    assert_eq!(read_view_prefs(dir.path()), ViewPrefs::default());
}
