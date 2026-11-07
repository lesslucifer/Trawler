#[path = "src/bundle.rs"]
mod bundle;
#[path = "src/windows_manifest.rs"]
mod windows_manifest;

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
        println!("cargo:rerun-if-changed={}", ui.join(bundle::BUNDLE_FLAVOUR).display());
        let wanted = bundle::Flavour::wanted(std::env::var_os("CARGO_FEATURE_E2E").is_some());
        if let Some(problem) = bundle::ensure_fresh(&ui, wanted) {
            println!("cargo::error={problem}");
            return;
        }
    }
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc {
        let out_dir =
            std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for every build script run");
        let manifest = Path::new(&out_dir).join("common-controls-v6.manifest");
        std::fs::write(&manifest, windows_manifest::COMMON_CONTROLS_V6)
            .expect("the build script writes into OUT_DIR");
        for directive in windows_manifest::link_directives(&manifest) {
            println!("{directive}");
        }
    }
    let windows = if msvc {
        tauri_build::WindowsAttributes::new_without_app_manifest()
    } else {
        tauri_build::WindowsAttributes::new()
    };
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        println!("cargo::error={error:#}");
    }
}
