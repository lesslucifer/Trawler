use std::path::Path;

pub const COMMON_CONTROLS_V6: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

pub fn link_directives(manifest_file: &Path) -> Vec<String> {
    vec![
        "cargo:rustc-link-arg-tests=/MANIFEST:EMBED".to_string(),
        format!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}", manifest_file.display()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directives_embed_the_manifest_file_into_test_executables() {
        let got = link_directives(Path::new("out/m.xml"));
        assert_eq!(
            got,
            [
                "cargo:rustc-link-arg-tests=/MANIFEST:EMBED",
                "cargo:rustc-link-arg-tests=/MANIFESTINPUT:out/m.xml"
            ]
        );
    }

    #[test]
    fn manifest_requires_common_controls_v6() {
        assert!(COMMON_CONTROLS_V6.contains("Microsoft.Windows.Common-Controls"));
        assert!(COMMON_CONTROLS_V6.contains(r#"version="6.0.0.0""#));
    }
}
