#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match trawler_ui_lib::core_args(std::env::args_os().skip(1)) {
        Some(core) => std::process::exit(trawler::cli::main(core)),
        None => trawler_ui_lib::run(),
    }
}
