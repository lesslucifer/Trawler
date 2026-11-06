use tauri::{AppHandle, Runtime};

use crate::lifecycle::{self, DialogRequest};

pub const MAIN_WINDOW: &str = "main";

#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>) {
    open(&app);
}

pub fn open<R: Runtime>(app: &AppHandle<R>) -> std::thread::JoinHandle<()> {
    lifecycle::request_dialog(app, DialogRequest::Settings)
}
