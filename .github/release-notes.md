# Trawler {version}

Trawler is not signed with a paid certificate, so your OS asks before the first open. Installed copies later offer each new release from inside the app.

## macOS

1. Open the `.dmg` and drag Trawler to Applications.
2. Open Trawler. macOS says it cannot verify the app.
3. Open System Settings → Privacy & Security, scroll to the Trawler message and choose **Open Anyway**.

Open at login needs macOS 13 or later.

## Windows

1. Run the `-setup.exe` (or the `.msi`).
2. SmartScreen says Windows protected your PC: choose **More info**, then **Run anyway**.

## Linux

- AppImage: `chmod +x` the file and run it.
- `.deb`: `sudo apt install ./<file>.deb`. `.rpm`: `sudo dnf install ./<file>.rpm`.
- Keeping capture running with the window closed needs a tray icon. GNOME shows one only with the AppIndicator extension; where none shows, open Trawler again to bring its window back.
- Open at login from an AppImage starts that file, so keep it where it is.
