use serde::Deserialize;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Runtime, Url};

pub const SOURCE_CODE: &str = "https://github.com/lesslucifer/Trawler";
pub const USER_GUIDE: &str = "https://github.com/lesslucifer/Trawler/blob/main/GUIDE.md";
const NEW_ISSUE: &str = "https://github.com/lesslucifer/Trawler/issues/new";
const ISSUE_TEMPLATE: &str = "bug_report.yml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Link {
    Guide,
    Issue,
    Source,
}

/// What a report says about this copy: never the traffic, never the machine's name.
#[derive(Debug, PartialEq, Eq)]
pub struct System {
    pub os: String,
    pub cpu: &'static str,
    pub install: &'static str,
}

impl System {
    pub fn this_one() -> Self {
        Self {
            os: format!("{} {}", os_name(std::env::consts::OS), tauri_plugin_os::version()),
            cpu: cpu_name(std::env::consts::ARCH),
            install: install_kind(tauri::utils::platform::bundle_type().as_ref()),
        }
    }
}

pub fn os_name(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

pub fn cpu_name(arch: &'static str) -> &'static str {
    match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// The issue form's install values; a copy run from `target/` has no bundle.
pub fn install_kind(bundle: Option<&BundleType>) -> &'static str {
    match bundle {
        Some(BundleType::App | BundleType::Dmg) => "dmg",
        Some(BundleType::Nsis) => "setup.exe",
        Some(BundleType::Msi) => "msi",
        Some(BundleType::Deb) => "deb",
        Some(BundleType::Rpm) => "rpm",
        Some(BundleType::AppImage) => "AppImage",
        None => "from source",
    }
}

pub fn issue_url(version: &str, system: &System) -> String {
    let os = format!("{} ({})", system.os, system.cpu);
    let params = [
        ("template", ISSUE_TEMPLATE),
        ("version", version),
        ("os", &os),
        ("install", system.install),
    ];
    Url::parse_with_params(NEW_ISSUE, params).map_or_else(|_| NEW_ISSUE.to_string(), String::from)
}

pub fn address<R: Runtime>(app: &AppHandle<R>, link: Link) -> String {
    match link {
        Link::Guide => USER_GUIDE.to_string(),
        Link::Source => SOURCE_CODE.to_string(),
        Link::Issue => issue_url(&app.package_info().version.to_string(), &System::this_one()),
    }
}

/// Opens the link in the browser; a browser that would not open hands back
/// the address for the user to copy.
pub fn open<R: Runtime>(app: &AppHandle<R>, link: Link) -> Result<(), String> {
    let address = address(app, link);
    tauri_plugin_opener::open_url(&address, None::<&str>).map_err(|_| address)
}

#[tauri::command]
pub fn open_link<R: Runtime>(app: AppHandle<R>, link: Link) -> Result<(), String> {
    open(&app, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac() -> System {
        System { os: "macOS 15.1".into(), cpu: "arm64", install: "dmg" }
    }

    #[test]
    fn a_report_carries_the_version_the_system_and_the_install_kind_and_nothing_else() {
        let url = Url::parse(&issue_url("0.1.0-5", &mac())).unwrap();
        assert_eq!(url.as_str().split('?').next(), Some(NEW_ISSUE));
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            [
                ("template", "bug_report.yml"),
                ("version", "0.1.0-5"),
                ("os", "macOS 15.1 (arm64)"),
                ("install", "dmg"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string()))
        );
    }

    #[test]
    fn a_report_encodes_what_it_carries() {
        let system = System { os: "Linux 24.04 & co".into(), cpu: "x64", install: "AppImage" };
        let url = issue_url("1.0.0+build", &system);
        assert!(url.contains("version=1.0.0%2Bbuild"), "{url}");
        assert!(url.contains("os=Linux+24.04+%26+co+%28x64%29"), "{url}");
    }

    #[test]
    fn the_install_kind_is_the_form_s_word_for_each_bundle() {
        let kinds = [
            (Some(BundleType::App), "dmg"),
            (Some(BundleType::Dmg), "dmg"),
            (Some(BundleType::Nsis), "setup.exe"),
            (Some(BundleType::Msi), "msi"),
            (Some(BundleType::Deb), "deb"),
            (Some(BundleType::Rpm), "rpm"),
            (Some(BundleType::AppImage), "AppImage"),
            (None, "from source"),
        ];
        for (bundle, kind) in kinds {
            assert_eq!(install_kind(bundle.as_ref()), kind, "{bundle:?}");
        }
    }

    #[test]
    fn the_os_and_cpu_read_as_a_user_names_them() {
        assert_eq!(os_name("macos"), "macOS");
        assert_eq!(os_name("windows"), "Windows");
        assert_eq!(os_name("linux"), "Linux");
        assert_eq!(cpu_name("aarch64"), "arm64");
        assert_eq!(cpu_name("x86_64"), "x64");
    }

    #[test]
    fn the_webview_names_a_link_and_never_an_address() {
        for (word, link) in
            [("guide", Link::Guide), ("issue", Link::Issue), ("source", Link::Source)]
        {
            assert_eq!(serde_json::from_str::<Link>(&format!("\"{word}\"")).unwrap(), link);
        }
        assert!(serde_json::from_str::<Link>("\"https://example.com\"").is_err());
    }

    #[test]
    fn the_guide_is_the_root_guide_on_the_project() {
        assert_eq!(USER_GUIDE, format!("{SOURCE_CODE}/blob/main/GUIDE.md"));
    }
}
