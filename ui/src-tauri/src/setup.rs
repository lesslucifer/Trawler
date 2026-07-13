// Trust boundary: the webview passes only PEM bytes; the privileged command is
// built here from fixed per-OS templates — never from IPC input.

use std::path::Path;
use std::process::Command;

fn install_command(os: &str, cert: &Path) -> Option<Vec<String>> {
    let cert = cert.to_string_lossy().into_owned();
    let argv: &[&str] = match os {
        "macos" => &[
            "security",
            "add-trusted-cert",
            "-d",
            "-r",
            "trustRoot",
            "-k",
            "/Library/Keychains/System.keychain",
        ],
        "windows" => &["certutil", "-addstore", "-f", "ROOT"],
        _ => return None,
    };
    let mut argv: Vec<String> = argv.iter().map(ToString::to_string).collect();
    argv.push(cert);
    Some(argv)
}

fn elevated_argv(os: &str, argv: &[String]) -> Vec<String> {
    match os {
        "macos" => {
            let script = argv.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" ");
            vec![
                "osascript".into(),
                "-e".into(),
                format!("do shell script \"{script}\" with administrator privileges"),
            ]
        }
        "windows" => {
            let args = argv[1..].iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(",");
            vec![
                "powershell".into(),
                "-NoProfile".into(),
                "-Command".into(),
                format!(
                    "$p = Start-Process -FilePath '{}' -ArgumentList {args} -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
                    argv[0]
                ),
            ]
        }
        _ => argv.to_vec(),
    }
}

#[tauri::command]
pub fn install_ca(pem: String) -> Result<(), String> {
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err("not a PEM certificate".into());
    }
    let os = std::env::consts::OS;
    let cert = std::env::temp_dir().join("trawler-ca.pem");
    let inner = install_command(os, &cert)
        .ok_or("no automatic install on this system — follow the guided steps")?;
    std::fs::write(&cert, &pem).map_err(|e| e.to_string())?;
    let argv = elevated_argv(os, &inner);
    let out = Command::new(&argv[0]).args(&argv[1..]).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(failure_message(&String::from_utf8_lossy(&out.stderr)))
    }
}

// The OS installer's own stderr is the only actionable signal on failure (e.g.
// macOS "authorization was denied since no user interaction was possible") — a
// bare exit code tells the user nothing, so surface the tool's message verbatim.
fn failure_message(stderr: &str) -> String {
    match stderr.trim() {
        "" => "install failed — the OS reported no detail; use the steps below".into(),
        detail => format!("install failed: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cert() -> PathBuf {
        PathBuf::from("/tmp/trawler-ca.pem")
    }

    #[test]
    fn macos_argv_targets_the_system_keychain() {
        assert_eq!(
            install_command("macos", &cert()).unwrap(),
            [
                "security",
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
                "-k",
                "/Library/Keychains/System.keychain",
                "/tmp/trawler-ca.pem",
            ]
            .map(String::from)
        );
    }

    #[test]
    fn windows_argv_targets_the_root_store() {
        assert_eq!(
            install_command("windows", &cert()).unwrap(),
            ["certutil", "-addstore", "-f", "ROOT", "/tmp/trawler-ca.pem"].map(String::from)
        );
    }

    #[test]
    fn other_systems_have_no_auto_path() {
        assert_eq!(install_command("linux", &cert()), None);
        assert_eq!(install_command("freebsd", &cert()), None);
    }

    #[test]
    fn macos_elevation_wraps_the_command_in_osascript() {
        let argv = elevated_argv("macos", &["security".into(), "-k".into()]);
        assert_eq!(argv[0], "osascript");
        assert_eq!(argv[1], "-e");
        assert_eq!(argv[2], "do shell script \"'security' '-k'\" with administrator privileges");
    }

    #[test]
    fn windows_elevation_wraps_the_command_in_a_uac_start_process() {
        let argv = elevated_argv("windows", &["certutil".into(), "-f".into(), "ROOT".into()]);
        assert_eq!(argv[0], "powershell");
        assert!(argv[3]
            .contains("Start-Process -FilePath 'certutil' -ArgumentList '-f','ROOT' -Verb RunAs"));
    }

    #[test]
    fn failure_message_surfaces_the_os_installer_stderr() {
        let msg = failure_message(
            "0:207: execution error: SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (1)\n",
        );
        assert!(msg.contains("authorization was denied"));
    }

    #[test]
    fn failure_message_falls_back_when_stderr_is_empty() {
        assert!(failure_message("   ").contains("use the steps below"));
    }

    #[test]
    fn install_ca_rejects_non_certificate_input() {
        let err = install_ca("BEGIN RSA PRIVATE KEY".into()).unwrap_err();
        assert!(err.contains("not a PEM certificate"));
    }
}
