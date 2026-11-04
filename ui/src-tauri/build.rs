#[path = "src/bundle.rs"]
mod bundle;

use std::path::Path;

fn main() {
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_some() {
        let crate_dir = std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("cargo sets CARGO_MANIFEST_DIR for every build script run");
        let ui = Path::new(&crate_dir).join("..");
        for input in bundle::BUNDLE_INPUTS {
            println!("cargo:rerun-if-changed={}", ui.join(input).display());
        }
        println!("cargo:rerun-if-changed={}", ui.join(bundle::BUNDLE_MARKER).display());
        if let Some(problem) = bundle::ensure_fresh(&ui) {
            println!("cargo::error={problem}");
            return;
        }
    }
    tauri_build::build();
}
