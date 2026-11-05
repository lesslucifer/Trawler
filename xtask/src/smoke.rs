use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::package::{self, Os};

const LAUNCHD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
const CAPTURED_PATH: &str = "/smoke-capture";
const PATIENCE: Duration = Duration::from_mins(2);
const SHUTDOWN: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq)]
pub enum Source {
    MacApp(PathBuf),
    NsisSetup(PathBuf),
    AppImage(PathBuf),
}

pub fn pick(os: Os, artifacts: &[PathBuf]) -> Result<Source, String> {
    let (suffix, wanted) = match os {
        Os::MacOs => (".app", "Trawler.app"),
        Os::Windows => ("-setup.exe", "an NSIS -setup.exe"),
        Os::Linux => (".AppImage", "an .AppImage"),
    };
    let found = artifacts.iter().find(|path| {
        path.file_name().and_then(|n| n.to_str()).is_some_and(|name| name.ends_with(suffix))
    });
    let path = found.ok_or(format!("the build left no {wanted} to smoke-test: {artifacts:?}"))?;
    Ok(match os {
        Os::MacOs => Source::MacApp(path.clone()),
        Os::Windows => Source::NsisSetup(path.clone()),
        Os::Linux => Source::AppImage(path.clone()),
    })
}

fn is_installed_executable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    Path::new(&lower).extension().is_some_and(|e| e == "exe") && !lower.starts_with("uninstall")
}

pub fn installer_args(install_dir: &Path) -> Vec<OsString> {
    let mut destination = OsString::from("/D=");
    destination.push(install_dir);
    vec!["/S".into(), destination]
}

pub fn core_args(ca_dir: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--core".into(), "--lifeline".into(), "--ca-dir".into()];
    args.push(ca_dir.into());
    args.extend(["--proxy-port", "0", "--api-port", "0"].map(OsString::from));
    args
}

pub fn parse_ready(line: &str) -> Result<(SocketAddr, SocketAddr), String> {
    let ready: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| format!("the ready line {line:?} is not JSON: {e}"))?;
    let addr = |key: &str| {
        ready[key]
            .as_str()
            .and_then(|text| text.parse().ok())
            .ok_or(format!("the ready line {line:?} has no {key} address"))
    };
    Ok((addr("proxy")?, addr("api")?))
}

struct Program {
    path: PathBuf,
    leading_args: Vec<OsString>,
}

fn only_file(dir: &Path, keep: impl Fn(&str) -> bool) -> Result<PathBuf, String> {
    let names = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut found: Vec<PathBuf> = names
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.file_name().and_then(|n| n.to_str()).is_some_and(&keep))
        .collect();
    if found.len() == 1 {
        Ok(found.remove(0))
    } else {
        Err(format!("expected one executable in {}, found {found:?}", dir.display()))
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("chmod {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn install(source: &Source, into: &Path) -> Result<Program, String> {
    match source {
        Source::MacApp(app) => Ok(Program {
            path: only_file(&app.join("Contents/MacOS"), |_| true)?,
            leading_args: vec![],
        }),
        Source::AppImage(image) => {
            make_executable(image)?;
            Ok(Program {
                path: image.clone(),
                leading_args: vec!["--appimage-extract-and-run".into()],
            })
        }
        Source::NsisSetup(setup) => {
            let status = Command::new(setup)
                .args(installer_args(into))
                .status()
                .map_err(|e| format!("run {}: {e}", setup.display()))?;
            if !status.success() {
                return Err(format!("{} exited with {status}", setup.display()));
            }
            let path = only_file(into, is_installed_executable)?;
            Ok(Program { path, leading_args: vec![] })
        }
    }
}

struct Core {
    child: Child,
}

impl Core {
    fn launch(program: &Program, home: &Path) -> Result<(Self, SocketAddr, SocketAddr), String> {
        let mut command = Command::new(&program.path);
        if cfg!(target_os = "macos") {
            command.env_clear().env("PATH", LAUNCHD_PATH);
        }
        let child = command
            .env("HOME", home)
            .current_dir(home)
            .args(&program.leading_args)
            .args(core_args(&home.join("ca")))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("launch {}: {e}", program.path.display()))?;
        let mut core = Self { child };
        let stdout = core.child.stdout.take().ok_or("no stdout")?;
        let (sent, line) = mpsc::channel();
        std::thread::spawn(move || {
            let mut first = String::new();
            let _ = BufReader::new(stdout).read_line(&mut first);
            let _ = sent.send(first);
        });
        let line = line
            .recv_timeout(PATIENCE)
            .map_err(|_| "the core printed no ready line".to_string())?;
        let (proxy, api) = parse_ready(&line)?;
        Ok((core, proxy, api))
    }

    fn quit(mut self) -> Result<(), String> {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + SHUTDOWN;
        while Instant::now() < deadline {
            if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
                return Ok(());
            }
            std::thread::park_timeout(Duration::from_millis(100));
        }
        Err("the core did not exit when its lifeline closed".to_string())
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve_once() -> Result<SocketAddr, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
        let Ok(clone) = stream.try_clone() else { return };
        let mut request = BufReader::new(clone);
        let mut line = String::new();
        while request.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
            line.clear();
        }
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    Ok(addr)
}

fn http(to: SocketAddr, request: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(to).map_err(|e| format!("connect {to}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    Ok(response)
}

fn get(api: SocketAddr, path: &str) -> Result<String, String> {
    http(api, &format!("GET {path} HTTP/1.1\r\nHost: {api}\r\nConnection: close\r\n\r\n"))
}

fn eventually(what: &str, mut holds: impl FnMut() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if holds() {
            return Ok(());
        }
        std::thread::park_timeout(Duration::from_millis(100));
    }
    Err(format!("timed out waiting for {what}"))
}

fn capture_through_the_proxy(proxy: SocketAddr, api: SocketAddr) -> Result<(), String> {
    let origin = serve_once()?;
    let request = format!(
        "GET http://{origin}{CAPTURED_PATH} HTTP/1.1\r\nHost: {origin}\r\nConnection: close\r\n\r\n"
    );
    let response = http(proxy, &request)?;
    if !response.starts_with("HTTP/1.1 200") {
        return Err(format!("the proxy answered {response:?}"));
    }
    eventually("GET /flows to list the request", || {
        get(api, "/flows").is_ok_and(|flows| flows.contains(CAPTURED_PATH))
    })?;
    let ca = get(api, "/ca.crt")?;
    if ca.contains("BEGIN CERTIFICATE") {
        Ok(())
    } else {
        Err(format!("GET /ca.crt served no certificate: {ca:?}"))
    }
}

pub fn parse(args: &[&str]) -> Result<package::Options, String> {
    let options = package::parse(args)?;
    if options.stage == package::Stage::BuildAndBundle {
        Ok(options)
    } else {
        Err(super::USAGE.to_string())
    }
}

pub fn run(args: &[&str], repo: &Path) -> Result<(i32, String), String> {
    let options = parse(args)?;
    let os = Os::named(std::env::consts::OS)?;
    let plan = package::plan(os, &options, &package::UNIVERSAL_PARTS, false)?;
    let artifacts = package::artifacts(&repo.join("target").join(&plan.bundle_dir))?;
    let source = pick(os, &artifacts)?;

    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let program = install(&source, &scratch.path().join("installed"))?;
    let (core, proxy, api) = Core::launch(&program, scratch.path())?;
    capture_through_the_proxy(proxy, api)?;
    core.quit()?;
    for port in [proxy, api] {
        eventually(&format!("{port} to be free"), || TcpListener::bind(port).is_ok())?;
    }
    Ok((0, format!("smoke passed: {source:?} captured a request and released {proxy} and {api}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn each_os_smokes_its_own_installer() {
        let built = paths(&[
            "b/macos/Trawler.app",
            "b/dmg/T.dmg",
            "b/nsis/T_x64-setup.exe",
            "b/msi/T.msi",
            "b/appimage/T.AppImage",
            "b/deb/T.deb",
        ]);
        assert_eq!(pick(Os::MacOs, &built).unwrap(), Source::MacApp("b/macos/Trawler.app".into()));
        assert_eq!(
            pick(Os::Windows, &built).unwrap(),
            Source::NsisSetup("b/nsis/T_x64-setup.exe".into())
        );
        assert_eq!(
            pick(Os::Linux, &built).unwrap(),
            Source::AppImage("b/appimage/T.AppImage".into())
        );
    }

    #[test]
    fn a_bundle_without_the_installer_is_refused_naming_it() {
        let refusal = pick(Os::Windows, &paths(&["b/msi/T.msi"])).unwrap_err();
        assert!(refusal.contains("-setup.exe") && refusal.contains("T.msi"), "{refusal}");
    }

    #[test]
    fn the_smoke_takes_the_package_s_flags_but_no_stage() {
        assert!(parse(&["--universal", "--profile", "p"]).unwrap().universal);
        for staged in ["--build-only", "--bundle-only"] {
            assert!(parse(&[staged]).unwrap_err().contains("cargo xtask smoke"), "{staged}");
        }
    }

    #[test]
    fn the_silent_installer_gets_its_destination_last_and_unquoted() {
        let args = installer_args(Path::new("C:/t/installed"));
        assert_eq!(args, ["/S", "/D=C:/t/installed"].map(OsString::from));
    }

    #[test]
    fn the_core_runs_headless_on_free_ports_with_a_lifeline_and_its_own_ca_dir() {
        let args: Vec<String> = core_args(Path::new("/t/ca"))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args.join(" "), "--core --lifeline --ca-dir /t/ca --proxy-port 0 --api-port 0");
    }

    #[test]
    fn the_ready_line_gives_both_addresses_and_anything_else_is_named() {
        let (proxy, api) =
            parse_ready(r#"{"proxy":"127.0.0.1:7420","api":"127.0.0.1:7421"}"#).unwrap();
        assert_eq!((proxy.port(), api.port()), (7420, 7421));
        assert!(parse_ready("not json").unwrap_err().contains("not JSON"));
        assert!(parse_ready(r#"{"proxy":"127.0.0.1:1"}"#).unwrap_err().contains("no api"));
    }

    #[test]
    fn the_executable_is_the_only_file_the_installer_left_besides_the_uninstaller() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Trawler-UI.EXE", "Uninstall.exe", "notes.txt"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let keep = is_installed_executable;
        assert_eq!(only_file(dir.path(), keep).unwrap(), dir.path().join("Trawler-UI.EXE"));
        std::fs::write(dir.path().join("second.exe"), "").unwrap();
        assert!(only_file(dir.path(), keep).unwrap_err().contains("expected one executable"));
    }

    #[test]
    fn smoke_is_documented() {
        assert!(crate::USAGE.contains("cargo xtask smoke [--universal] [--profile <name>]"));
    }
}
