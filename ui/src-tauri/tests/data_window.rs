use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};
use trawler_ui_lib::data_window::{
    self, frame, label_for, unframe, Caption, Payload, LABEL_PREFIX,
};
use trawler_ui_lib::settings::MAIN_WINDOW;
use trawler_ui_lib::window_geometry::{self, Change, Records, DATA_WINDOWS_RECORD};

fn app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = data_window::install(mock_builder())
        .invoke_handler(tauri::generate_handler![
            data_window::open_data_window,
            data_window::take_data_payload
        ])
        .build(mock_context(noop_assets()))
        .expect("a mock app");
    let main = WebviewWindowBuilder::new(&app, MAIN_WINDOW, tauri::WebviewUrl::default())
        .build()
        .expect("the main window");
    (app, main)
}

fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: InvokeBody,
) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().expect("a url"),
            body,
            headers: tauri::http::HeaderMap::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
}

fn payload(key: &str, title: &str, bytes: Vec<u8>) -> Payload {
    Payload {
        caption: Caption {
            key: key.into(),
            title: title.into(),
            type_hint: Some("application/octet-stream".into()),
        },
        bytes,
    }
}

fn open(main: &WebviewWindow<MockRuntime>, payload: &Payload) {
    invoke(main, "open_data_window", InvokeBody::Raw(frame(payload))).expect("the window opens");
}

fn claim(window: &WebviewWindow<MockRuntime>) -> Vec<u8> {
    match invoke(window, "take_data_payload", InvokeBody::default()).expect("a claim answers") {
        InvokeResponseBody::Raw(bytes) => bytes,
        InvokeResponseBody::Json(json) => panic!("the claim answered JSON: {json}"),
    }
}

fn data_labels(app: &App<MockRuntime>) -> Vec<String> {
    let mut labels: Vec<_> =
        app.webview_windows().into_keys().filter(|l| l.starts_with(LABEL_PREFIX)).collect();
    labels.sort();
    labels
}

fn window(app: &App<MockRuntime>, key: &str) -> WebviewWindow<MockRuntime> {
    app.get_webview_window(&label_for(key)).expect("the data window")
}

#[test]
fn ten_mebibytes_reach_the_data_window_byte_identical_and_only_once() {
    let (app, main) = app();
    let bytes: Vec<u8> =
        (0..10 * 1024 * 1024).map(|i: u32| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    let handed = payload("flow-7:response", "Response body · example.com", bytes);

    open(&main, &handed);
    let data = window(&app, "flow-7:response");

    assert_eq!(unframe(&claim(&data)), Ok(handed));
    assert!(claim(&data).is_empty());
}

#[test]
fn a_second_open_with_one_key_focuses_the_window_and_keeps_its_payload() {
    let (app, main) = app();
    let first = payload("k", "first title", b"first".to_vec());

    open(&main, &first);
    open(&main, &payload("k", "second title", b"second".to_vec()));

    assert_eq!(data_labels(&app), [label_for("k")]);
    let data = window(&app, "k");
    assert_eq!(unframe(&claim(&data)), Ok(first));
}

#[test]
fn another_key_opens_a_second_window() {
    let (app, main) = app();

    open(&main, &payload("request", "a", b"1".to_vec()));
    open(&main, &payload("response", "b", b"2".to_vec()));

    assert_eq!(data_labels(&app).len(), 2);
    assert_eq!(unframe(&claim(&window(&app, "response"))).expect("a payload").bytes, b"2");
}

#[test]
fn the_main_window_claims_nothing() {
    let (_app, main) = app();
    open(&main, &payload("k", "t", b"x".to_vec()));
    assert!(claim(&main).is_empty());
}

#[test]
fn a_body_that_is_not_a_frame_opens_nothing() {
    let (app, main) = app();
    assert!(invoke(&main, "open_data_window", InvokeBody::Raw(vec![0, 0, 0, 8])).is_err());
    assert!(invoke(&main, "open_data_window", InvokeBody::Json(serde_json::json!({}))).is_err());
    assert!(data_labels(&app).is_empty());
}

#[test]
fn the_main_window_destroyed_takes_every_data_window_with_it() {
    let (app, main) = app();
    open(&main, &payload("a", "a", b"1".to_vec()));
    open(&main, &payload("b", "b", b"2".to_vec()));

    let mut closed = data_window::on_destroyed(app.handle(), MAIN_WINDOW);

    closed.sort();
    assert_eq!(closed, data_labels(&app));
    assert_eq!(closed.len(), 2);
}

#[test]
fn a_data_window_destroyed_takes_only_its_unclaimed_payload() {
    let (app, main) = app();
    open(&main, &payload("a", "a", b"1".to_vec()));
    open(&main, &payload("b", "b", b"2".to_vec()));

    assert!(data_window::on_destroyed(app.handle(), &label_for("a")).is_empty());

    assert!(claim(&window(&app, "a")).is_empty());
    assert_eq!(unframe(&claim(&window(&app, "b"))).expect("b's payload").bytes, b"2");
}

fn tracked_app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let (app, main) = app();
    window_geometry::manage(app.handle(), Records::new());
    (app, main)
}

fn moved(app: &App<MockRuntime>, key: &str, x: i32, y: i32) {
    window_geometry::record_change(&window(app, key).as_ref().window(), Change::Moved { x, y });
}

#[test]
fn any_number_of_data_windows_write_one_shared_record() {
    let (app, main) = tracked_app();

    for key in ["flow-1:request", "flow-1:response", "flow-2:response", "a", "b"] {
        open(&main, &payload(key, key, b"x".to_vec()));
    }

    assert_eq!(data_labels(&app).len(), 5);
    assert_eq!(
        window_geometry::tracked_records(app.handle()).into_keys().collect::<Vec<_>>(),
        [DATA_WINDOWS_RECORD]
    );
}

#[test]
fn the_data_window_moved_last_writes_the_shared_record() {
    let (app, main) = tracked_app();
    open(&main, &payload("a", "a", b"1".to_vec()));
    open(&main, &payload("b", "b", b"2".to_vec()));

    moved(&app, "a", 40, 60);
    moved(&app, "b", 300, 200);

    let shared = window_geometry::tracked_records(app.handle())[DATA_WINDOWS_RECORD];
    assert_eq!((shared.x, shared.y), (300, 200));

    moved(&app, "a", 7, 9);

    let shared = window_geometry::tracked_records(app.handle())[DATA_WINDOWS_RECORD];
    assert_eq!((shared.x, shared.y), (7, 9));
}
