# Security policy

## Supported versions

Only the latest release gets security fixes. Installed copies offer each new release from inside the app.

## Reporting a vulnerability

Report it privately through GitHub: **Security → [Report a vulnerability](https://github.com/lesslucifer/Trawler/security/advisories/new)**. Please never open a public issue for a vulnerability.

Include:

- your Trawler version and your system with its version,
- how you installed it,
- the steps that show the problem, and what an attacker gains.

Reports are answered on a best-effort basis by one maintainer.

## In scope

- How Trawler stores or handles its certificate authority (CA) key.
- The proxy or the local API answering beyond this machine without the user allowing another device.
- The updater accepting an unsigned, altered or older release.
- Captured data leaving the machine other than where the user's own traffic was going.

## Out of scope

- Slowing or crashing the local proxy with traffic from this machine: please report that as an ordinary bug.
- The documented risk of a trusted certificate, described below.

## About Trawler's certificate

Each install makes its own certificate authority. Its private key is stored unencrypted in the app data folder (`~/Library/Application Support/dev.trawler.app/ca` on macOS, `%APPDATA%\dev.trawler.app\ca` on Windows, `~/.local/share/dev.trawler.app/ca` on Linux) and is never sent anywhere. While the certificate is trusted, anyone who can read your user files could use that key to read your HTTPS. Remove the certificate when you stop using Trawler ([the guide](GUIDE.md#uninstall)).
