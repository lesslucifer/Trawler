// The app has no console (windows_subsystem), so Windows gives each console
// program it starts a console of its own: a window flashing up on every read
// the setup screen repeats, and where that console cannot start, the child
// dies at load (0xc0000142) in a system error dialog every poll.

use std::ffi::OsStr;
use std::process::Command;

#[cfg(windows)]
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

#[cfg(test)]
mod tests {
    // bundle.rs runs in the build script, never in the app.
    const SPAWNS_ELSEWHERE: [&str; 2] = ["spawn.rs", "bundle.rs"];

    fn shipped_code(source: &str) -> &str {
        source.split("#[cfg(test)]\nmod tests").next().unwrap_or(source)
    }

    #[test]
    fn every_program_the_app_starts_is_started_without_a_console() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut bare = Vec::new();
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.extension().is_some_and(|ext| ext == "rs")
                && !SPAWNS_ELSEWHERE.contains(&name.as_str())
                && shipped_code(&std::fs::read_to_string(&path).unwrap()).contains("Command::new(")
            {
                bare.push(name);
            }
        }
        assert!(bare.is_empty(), "start these through spawn::command: {bare:?}");
    }
}
