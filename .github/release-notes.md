# Trawler {version}

Release candidate — for testing; the final follows.

## What's new

{changes}

## Install

Trawler is not signed with a paid certificate, so your OS asks before the first open. Installed copies later offer each new release from inside the app. To check your download, compare it with `SHA256SUMS` (below).

### macOS

macOS 11 or later, Apple Silicon and Intel.

1. Open the `.dmg` and drag Trawler to Applications.
2. Open Trawler. macOS says it cannot verify the app.
3. Open **System Settings** → **Privacy & Security**, scroll to the Trawler message and choose **Open Anyway**.

Open at login needs macOS 13 or later.

### Windows

1. Run the `-setup.exe` (or the `.msi`).
2. SmartScreen says Windows protected your PC: choose **More info**, then **Run anyway**.

### Linux

- AppImage: `chmod +x` the file and run it.
- `.deb`: `sudo apt install ./<file>.deb`.
- `.rpm`: `sudo dnf install ./<file>.rpm`.
- Keeping capture running with the window closed needs a tray icon. GNOME shows one only with the AppIndicator extension; where none shows, open Trawler again to bring its window back.
- Open at login from an AppImage starts that file, so keep it where it is.

## Check your download

`SHA256SUMS` lists the SHA-256 of every installer in this release. Download it beside your installer, then in that folder:

- macOS: `shasum -a 256 -c SHA256SUMS --ignore-missing`
- Linux: `sha256sum -c SHA256SUMS --ignore-missing`
- Windows (PowerShell): `(Get-FileHash .\<file> -Algorithm SHA256).Hash`, then compare it with that file's line in `SHA256SUMS`.

Each check must say `OK`, or the hashes must match. A mismatch means a damaged or altered download: delete it and download again.
