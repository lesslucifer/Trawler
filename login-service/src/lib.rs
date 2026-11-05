#![cfg(target_os = "macos")]

mod agent;

pub use agent::AgentService;

#[cfg(test)]
mod tests {
    #[test]
    fn exactly_one_module_allows_unsafe_code() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let allows: Vec<_> = std::fs::read_dir(src)
            .unwrap()
            .flatten()
            .filter(|entry| {
                std::fs::read_to_string(entry.path())
                    .unwrap()
                    .lines()
                    .any(|line| line.trim_start().starts_with("#![allow(unsafe_code)]"))
            })
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(allows, ["agent.rs"]);
    }
}
