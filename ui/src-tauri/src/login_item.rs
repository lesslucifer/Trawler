use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::BACKGROUND_FLAG;

pub const LABEL: &str = "dev.trawler.app";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LoginItem {
    pub on: bool,
    pub starts: Option<PathBuf>,
}

impl LoginItem {
    const OFF: Self = Self { on: false, starts: None };

    fn from_entry(starts: Option<PathBuf>) -> Self {
        Self { on: true, starts }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    LaunchAgent,
    Autostart,
}

impl Format {
    pub fn entry(self, root: &Path) -> PathBuf {
        match self {
            Self::LaunchAgent => root.join("Library/LaunchAgents").join(format!("{LABEL}.plist")),
            Self::Autostart => root.join("autostart").join(format!("{LABEL}.desktop")),
        }
    }

    pub fn render(self, exe: &str) -> String {
        match self {
            Self::LaunchAgent => launch_agent(exe),
            Self::Autostart => autostart(exe),
        }
    }

    pub fn starts(self, entry: &str) -> Option<PathBuf> {
        match self {
            Self::LaunchAgent => launch_agent_program(entry),
            Self::Autostart => autostart_program(entry),
        }
        .map(PathBuf::from)
    }
}

// A control character would end the desktop entry's Exec line, or the plist's XML.
fn utf8(exe: &Path) -> std::io::Result<&str> {
    exe.to_str().filter(|exe| !exe.chars().any(char::is_control)).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a path a login entry can name", exe.display()),
        )
    })
}

pub fn read(format: Format, root: &Path) -> LoginItem {
    match std::fs::read(format.entry(root)) {
        Ok(bytes) => LoginItem::from_entry(format.starts(&String::from_utf8_lossy(&bytes))),
        Err(_) => LoginItem::OFF,
    }
}

pub fn write(format: Format, root: &Path, on: bool, exe: &Path) -> std::io::Result<LoginItem> {
    let entry = format.entry(root);
    if on {
        crate::layout::replace_whole(&entry, format.render(utf8(exe)?).as_bytes())?;
    } else if let Err(err) = std::fs::remove_file(&entry) {
        if err.kind() != std::io::ErrorKind::NotFound {
            return Err(err);
        }
    }
    Ok(read(format, root))
}

fn xml_escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn xml_unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn launch_agent(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>{BACKGROUND_FLAG}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
</dict>
</plist>
"#,
        xml_escaped(exe)
    )
}

fn launch_agent_program(plist: &str) -> Option<String> {
    let (_, arguments) = plist.split_once("<key>ProgramArguments</key>")?;
    let (_, first) = arguments.trim_start().strip_prefix("<array>")?.split_once("<string>")?;
    let (program, _) = first.split_once("</string>")?;
    Some(xml_unescaped(program))
}

// Desktop Entry spec: inside double quotes `"`, `` ` ``, `$` and `\` take a
// backslash; then the string escape doubles every backslash, and `%` is a field
// code unless doubled.
fn exec_quoted(program: &str) -> String {
    let mut quoted = String::from("\"");
    for c in program.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

fn exec_first_argument(exec: &str) -> Option<String> {
    let exec = exec.replace("\\\\", "\\");
    let mut chars = exec.trim_start().chars();
    let mut program = String::new();
    if exec.trim_start().starts_with('"') {
        chars.next();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => program.push(chars.next()?),
                c => program.push(c),
            }
        }
    } else {
        program.extend(chars.take_while(|c| !c.is_whitespace()));
    }
    Some(program.replace("%%", "%")).filter(|program| !program.is_empty())
}

fn autostart(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Trawler\nExec={} {BACKGROUND_FLAG}\nX-GNOME-Autostart-enabled=true\n",
        exec_quoted(exe)
    )
}

fn autostart_program(entry: &str) -> Option<String> {
    entry.lines().find_map(|line| line.strip_prefix("Exec=")).and_then(exec_first_argument)
}

pub fn run_value(exe: &str) -> String {
    format!("\"{exe}\" {BACKGROUND_FLAG}")
}

pub fn run_value_program(query_output: &str) -> Option<String> {
    let (_, value) = query_output.lines().find_map(|line| line.split_once("REG_SZ"))?;
    let (program, _) = value.trim().strip_prefix('"')?.split_once('"')?;
    Some(program.to_owned())
}

#[cfg(windows)]
mod registry {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Output};

    use super::{run_value, run_value_program, utf8, LoginItem};

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Trawler";
    // The app has no console (windows_subsystem), so reg.exe would flash one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn reg(args: &[&str]) -> std::io::Result<Output> {
        Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
    }

    pub fn read() -> LoginItem {
        match reg(&["query", RUN_KEY, "/v", VALUE]) {
            Ok(out) if out.status.success() => LoginItem::from_entry(
                run_value_program(&String::from_utf8_lossy(&out.stdout)).map(Into::into),
            ),
            _ => LoginItem::OFF,
        }
    }

    pub fn write(on: bool, exe: &Path) -> std::io::Result<LoginItem> {
        let value = run_value(utf8(exe)?);
        let args: &[&str] = if on {
            &["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &value, "/f"]
        } else {
            &["delete", RUN_KEY, "/v", VALUE, "/f"]
        };
        let out = reg(args)?;
        let reading = read();
        if out.status.success() || reading.on == on {
            return Ok(reading);
        }
        Err(std::io::Error::other(format!(
            "reg {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[derive(Clone, Debug)]
pub enum Place {
    File {
        format: Format,
        root: PathBuf,
    },
    #[cfg(windows)]
    Registry,
}

impl Place {
    pub fn read(&self) -> LoginItem {
        match self {
            Self::File { format, root } => read(*format, root),
            #[cfg(windows)]
            Self::Registry => registry::read(),
        }
    }

    pub fn write(&self, on: bool, exe: &Path) -> std::io::Result<LoginItem> {
        match self {
            Self::File { format, root } => write(*format, root, on, exe),
            #[cfg(windows)]
            Self::Registry => registry::write(on, exe),
        }
    }
}

pub struct LoginItemPlace(pub Option<Place>);

#[cfg(target_os = "macos")]
fn this_hosts_place<R: Runtime>(app: &AppHandle<R>) -> Option<Place> {
    let root = app.path().home_dir().ok()?;
    Some(Place::File { format: Format::LaunchAgent, root })
}

#[cfg(windows)]
fn this_hosts_place<R: Runtime>(_app: &AppHandle<R>) -> Option<Place> {
    Some(Place::Registry)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn this_hosts_place<R: Runtime>(app: &AppHandle<R>) -> Option<Place> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| app.path().home_dir().ok().map(|home| home.join(".config")))?;
    Some(Place::File { format: Format::Autostart, root })
}

pub fn install<R: Runtime>(app: &AppHandle<R>) {
    app.manage(LoginItemPlace(this_hosts_place(app)));
}

fn place<'a>(place: &'a State<'_, LoginItemPlace>) -> Result<&'a Place, String> {
    place.0.as_ref().ok_or_else(|| "this host has no place for a login item".into())
}

#[tauri::command]
pub fn login_item_read(login_item: State<'_, LoginItemPlace>) -> Result<LoginItem, String> {
    Ok(place(&login_item)?.read())
}

#[tauri::command]
pub fn login_item_write(
    login_item: State<'_, LoginItemPlace>,
    on: bool,
) -> Result<LoginItem, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    place(&login_item)?.write(on, &exe).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "/Applications/Trawler.app/Contents/MacOS/trawler-ui";

    #[test]
    fn a_launch_agent_starts_the_binary_in_the_background_at_load() {
        let plist = launch_agent(PLAIN);
        for line in [
            "<string>dev.trawler.app</string>",
            "<string>/Applications/Trawler.app/Contents/MacOS/trawler-ui</string>\n\t\t<string>--background</string>",
            "<key>RunAtLoad</key>\n\t<true/>",
        ] {
            assert!(plist.contains(line), "{line} in {plist}");
        }
        assert_eq!(launch_agent_program(&plist).as_deref(), Some(PLAIN));
    }

    #[test]
    fn markup_in_a_path_is_escaped_and_reads_back() {
        let odd = "/Users/a&b/<odd> dir/trawler-ui";
        let plist = launch_agent(odd);
        assert!(plist.contains("<string>/Users/a&amp;b/&lt;odd&gt; dir/trawler-ui</string>"));
        assert_eq!(launch_agent_program(&plist).as_deref(), Some(odd));
    }

    #[test]
    fn a_path_a_login_entry_cannot_name_is_refused_before_anything_is_written() {
        let root = tempfile::tempdir().unwrap();
        let split = Path::new("/opt/trawler\nName=Other/trawler-ui");
        let refused = write(Format::Autostart, root.path(), true, split).unwrap_err();
        assert_eq!(refused.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(read(Format::Autostart, root.path()), LoginItem::OFF);
    }

    #[test]
    fn a_plist_that_names_no_program_reads_none() {
        assert_eq!(launch_agent_program("<plist><dict></dict></plist>"), None);
        assert_eq!(Format::LaunchAgent.starts("<key>ProgramArguments</key><array></array>"), None);
    }

    #[test]
    fn a_desktop_entry_quotes_its_program_per_the_spec() {
        assert_eq!(
            autostart("/opt/My Apps/trawler-ui"),
            "[Desktop Entry]\nType=Application\nName=Trawler\nExec=\"/opt/My Apps/trawler-ui\" --background\nX-GNOME-Autostart-enabled=true\n"
        );
        assert_eq!(exec_quoted(r#"/a "b"/c"#), r#""/a \\"b\\"/c""#);
        assert_eq!(exec_quoted(r"/a\b"), r#""/a\\\\b""#);
        assert_eq!(exec_quoted("/a$b`c%d"), r#""/a\\$b\\`c%%d""#);
    }

    #[test]
    fn every_quoted_program_reads_back_as_itself() {
        for program in ["/opt/trawler-ui", "/opt/My Apps/trawler-ui", r#"/a "b"\c$d`e%f"#] {
            let entry = autostart(program);
            assert_eq!(autostart_program(&entry).as_deref(), Some(program), "{entry}");
        }
        assert_eq!(
            autostart_program("Exec=/usr/bin/trawler-ui --background").as_deref(),
            Some("/usr/bin/trawler-ui")
        );
        assert_eq!(autostart_program("[Desktop Entry]\nName=Trawler\n"), None);
    }

    #[test]
    fn the_run_value_quotes_its_program_and_reads_back_from_reg_query() {
        let exe = r"C:\Program Files\Trawler\trawler-ui.exe";
        assert_eq!(run_value(exe), r#""C:\Program Files\Trawler\trawler-ui.exe" --background"#);
        let query = format!(
            "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    Trawler    REG_SZ    {}\r\n\r\n",
            run_value(exe)
        );
        assert_eq!(run_value_program(&query).as_deref(), Some(exe));
        assert_eq!(
            run_value_program(
                "ERROR: The system was unable to find the specified registry key or value."
            ),
            None
        );
    }
}
