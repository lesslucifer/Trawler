#[path = "src/bundle.rs"]
mod bundle;

use std::path::Path;

fn main() {
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_some() {
        let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for input in bundle::BUNDLE_INPUTS {
            println!("cargo:rerun-if-changed={}", ui.join(input).display());
        }
        println!("cargo:rerun-if-changed={}", ui.join(bundle::BUNDLE_MARKER).display());
        println!("cargo:rerun-if-changed={}", ui.join(bundle::BUNDLE_FLAVOUR).display());
        let wanted = bundle::Flavour::wanted(std::env::var_os("CARGO_FEATURE_E2E").is_some());
        if let Some(problem) = bundle::ensure_fresh(&ui, wanted) {
            println!("cargo::error={problem}");
            return;
        }
    }
    tauri_build::build();
}
