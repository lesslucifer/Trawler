// Pure CA-trust helpers (E-UISETUP.W2) — no I/O, no React. The frontend picks
// which step content to render from the webview UA (== host OS); Rust builds
// the actual privileged command from std::env::consts::OS at call time.

import type { ProxyTarget } from "./proxyTargets";

export type HostOs = "macos" | "windows" | "linux" | "other";
export type TrustTarget = "macos" | "windows" | "linux" | "firefox" | "ios-sim" | "android";

export function hostOs(ua: string = navigator.userAgent): HostOs {
  if (ua.includes("Windows")) return "windows";
  if (ua.includes("Mac OS X") || ua.includes("Macintosh")) return "macos";
  if (ua.includes("Linux")) return "linux";
  return "other";
}

// "This OS has a one-click local setup path" — one concept shared by both the
// trust-ca and set-proxy steps (macOS + Windows), so it is not duplicated.
export function canAutoSetup(os: HostOs): boolean {
  return os === "macos" || os === "windows";
}

export function defaultTarget(os: HostOs): TrustTarget {
  return os === "other" ? "linux" : os;
}

export const TARGETS: readonly { id: TrustTarget; label: string }[] = [
  { id: "macos", label: "macOS" },
  { id: "windows", label: "Windows" },
  { id: "linux", label: "Linux" },
  { id: "firefox", label: "Firefox" },
  { id: "ios-sim", label: "iOS Simulator" },
  { id: "android", label: "Android" },
];

export const CA_FILENAME = "trawler-ca.pem";

// Step one of every manual route names this file, so the screen owes the user
// a way to produce it: the save control sits at the end of this sentence, which
// is why it carries no terminal period.
export const SAVE_CA_STEP = `Save Trawler's certificate as ${CA_FILENAME}`;
const DOWNLOAD = SAVE_CA_STEP;

// The one copyable command per target, or null where the steps are point-and-click.
// Named here rather than inferred at the call site: the module builds the arrays,
// so it is the module that knows which element is a command. The arrays below
// splice these values in rather than retyping them — manualSteps renders the copy
// affordance on `step === command` by string equality, so a drift between the two
// would silently drop it.
const TRUST_COMMANDS: Record<TrustTarget, string | null> = {
  macos: "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain trawler-ca.pem",
  windows: "certutil -addstore -f ROOT trawler-ca.pem",
  linux: "sudo cp trawler-ca.pem /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates",
  firefox: null,
  "ios-sim": null,
  android: "adb push trawler-ca.pem /sdcard/Download/",
};

const STEPS: Record<TrustTarget, string[]> = {
  macos: [
    DOWNLOAD,
    "Run it in a terminal:",
    TRUST_COMMANDS.macos!,
    "Or double-click the file to add it in Keychain Access, then set the Trawler CA to Always Trust.",
  ],
  windows: [
    DOWNLOAD,
    "In an elevated terminal run:",
    TRUST_COMMANDS.windows!,
  ],
  linux: [
    DOWNLOAD,
    "Run it in a terminal:",
    TRUST_COMMANDS.linux!,
    "Chrome reads the NSS store instead: certutil -d sql:$HOME/.pki/nssdb -A -t C,, -n trawler -i trawler-ca.pem",
  ],
  firefox: [
    "Firefox keeps its own certificate store, separate from the operating system's.",
    DOWNLOAD,
    "Settings → Privacy & Security → Certificates → View Certificates → Authorities → Import.",
    "Select trawler-ca.pem and check \"Trust this CA to identify websites\".",
  ],
  "ios-sim": [
    "The iOS Simulator uses the host Mac's trust store — the macOS install already covers it.",
    "If the Simulator was running during the install, restart it to pick up the new trust.",
  ],
  android: [
    DOWNLOAD,
    "Push it to the device:",
    TRUST_COMMANDS.android!,
    "Settings → Security → Encryption & credentials → Install a certificate → CA certificate.",
    "Apps targeting Android 7+ only trust user CAs when their network security config opts in.",
  ],
};

export function trustSnippet(target: TrustTarget): string | null {
  return TRUST_COMMANDS[target];
}

export function guidedSteps(target: TrustTarget): string[] {
  return STEPS[target];
}

// Trawler's CA is issued with CN "Trawler Proxy CA" — removal keys off that name.
const REMOVE_COMMANDS: Record<TrustTarget, string | null> = {
  macos: 'sudo security delete-certificate -c "Trawler Proxy CA" /Library/Keychains/System.keychain',
  windows: 'certutil -delstore ROOT "Trawler Proxy CA"',
  linux: "sudo rm /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates --fresh",
  firefox: null,
  "ios-sim": null,
  android: null,
};

const REMOVE: Record<TrustTarget, string[]> = {
  macos: [
    "Run it in a terminal:",
    REMOVE_COMMANDS.macos!,
    "Or delete the Trawler Proxy CA entry in Keychain Access → System.",
  ],
  windows: [
    "In an elevated terminal run:",
    REMOVE_COMMANDS.windows!,
  ],
  linux: [
    "Run it in a terminal:",
    REMOVE_COMMANDS.linux!,
    "NSS store: certutil -d sql:$HOME/.pki/nssdb -D -n trawler",
  ],
  firefox: [
    "Settings → Privacy & Security → Certificates → View Certificates → Authorities.",
    "Select Trawler Proxy CA → Delete or Distrust.",
  ],
  "ios-sim": [
    "Remove it from the host Mac (the macOS removal above), then restart the Simulator.",
    "Or reset the Simulator: Device → Erase All Content and Settings.",
  ],
  android: [
    "Settings → Security → Encryption & credentials → Trusted credentials → User → remove Trawler Proxy CA.",
  ],
};

export function removalSnippet(target: TrustTarget): string | null {
  return REMOVE_COMMANDS[target];
}

export function removalSteps(target: TrustTarget): string[] {
  return REMOVE[target];
}

// --- Set the proxy (E-UISETUP.W3) ---------------------------------------------
// Same shape as the trust-ca helpers: pure, no I/O, instruction text as data.
// Every snippet carries the real configured address — never a hardcoded port.

export function splitProxyAddr(addr: string): { host: string; port: number } {
  const at = addr.lastIndexOf(":");
  return { host: addr.slice(0, at), port: Number(addr.slice(at + 1)) };
}

// The one copyable command for the target, or null for GUI-only clients whose
// steps are point-and-click (Firefox, iOS Simulator, Android).
export function proxySnippet(t: ProxyTarget, addr: string): string | null {
  const { host, port } = splitProxyAddr(addr);
  switch (t) {
    case "macos":
      return `networksetup -setwebproxy Wi-Fi ${host} ${port} && networksetup -setsecurewebproxy Wi-Fi ${host} ${port}`;
    case "windows":
      return `reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyServer /t REG_SZ /d ${addr} /f && reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f`;
    case "linux":
      return `gsettings set org.gnome.system.proxy mode 'manual' && gsettings set org.gnome.system.proxy.http host '${host}' && gsettings set org.gnome.system.proxy.http port ${port} && gsettings set org.gnome.system.proxy.https host '${host}' && gsettings set org.gnome.system.proxy.https port ${port}`;
    case "terminal":
      return `export HTTPS_PROXY=http://${addr} HTTP_PROXY=http://${addr}`;
    default:
      return null;
  }
}

export function proxySteps(t: ProxyTarget, addr: string): string[] {
  const { host, port } = splitProxyAddr(addr);
  const snippet = proxySnippet(t, addr);
  switch (t) {
    case "macos":
      return ["Run — this sets the Wi-Fi service; change the name for Ethernet etc.:", snippet!];
    case "windows":
      return ["Run:", snippet!];
    case "linux":
      return ["GNOME — run:", snippet!, "Chrome/Electron inherit this system proxy."];
    case "firefox":
      return [
        "Firefox keeps its own proxy settings, separate from the system.",
        "Settings → Network Settings → Settings… → Manual proxy configuration.",
        `HTTP Proxy ${host}, Port ${port}, and check "Also use this proxy for HTTPS".`,
      ];
    case "terminal":
      return ["Export the proxy in your shell (curl, npm, and most CLIs honour it):", snippet!];
    case "ios-sim":
      return [
        "The iOS Simulator uses the host Mac's system proxy — the macOS setup already covers it.",
        "Restart the Simulator if it was running when you set the proxy.",
      ];
    case "android":
      return [
        `Wi-Fi → your network → Proxy → Manual. Host = your machine's LAN address, Port ${port}.`,
        "Android emulator: use host 10.0.2.2 to reach the machine it runs on.",
        "Reaching a device needs Trawler bound beyond loopback — see setup notes.",
      ];
  }
}

// P1 prove-it: the concrete, copyable command that generates the first request.
// Text only — the app never fires it (epic non-goal); it guides the user.
export function proveItSnippet(addr: string): string {
  return `curl -x http://${addr} https://example.com`;
}
