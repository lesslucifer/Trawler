// Pure CA-trust helpers (E-UISETUP.W2) — no I/O, no React. The frontend picks
// which step content to render from the webview UA (== host OS); Rust builds
// the actual privileged command from std::env::consts::OS at call time.

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

const DOWNLOAD = "Save Trawler's certificate as trawler-ca.pem.";

const STEPS: Record<TrustTarget, string[]> = {
  macos: [
    DOWNLOAD,
    "Run: sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain trawler-ca.pem",
    "Or double-click the file to add it in Keychain Access, then set the Trawler CA to Always Trust.",
  ],
  windows: [
    DOWNLOAD,
    "In an elevated terminal run: certutil -addstore -f ROOT trawler-ca.pem",
  ],
  linux: [
    DOWNLOAD,
    "Run: sudo cp trawler-ca.pem /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates",
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
    "Push it to the device: adb push trawler-ca.pem /sdcard/Download/",
    "Settings → Security → Encryption & credentials → Install a certificate → CA certificate.",
    "Apps targeting Android 7+ only trust user CAs when their network security config opts in.",
  ],
};

export function guidedSteps(target: TrustTarget): string[] {
  return STEPS[target];
}

// Trawler's CA is issued with CN "Trawler Proxy CA" — removal keys off that name.
const REMOVE: Record<TrustTarget, string[]> = {
  macos: [
    'Run: sudo security delete-certificate -c "Trawler Proxy CA" /Library/Keychains/System.keychain',
    "Or delete the Trawler Proxy CA entry in Keychain Access → System.",
  ],
  windows: [
    'In an elevated terminal run: certutil -delstore ROOT "Trawler Proxy CA"',
  ],
  linux: [
    "Run: sudo rm /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates --fresh",
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

export function removalSteps(target: TrustTarget): string[] {
  return REMOVE[target];
}

// --- Set the proxy (E-UISETUP.W3) ---------------------------------------------
// Same shape as the trust-ca helpers: pure, no I/O, instruction text as data.
// Every snippet carries the real configured address — never a hardcoded port.

export type ProxyTarget = "macos" | "windows" | "linux" | "firefox" | "terminal" | "ios-sim" | "android";

export function defaultProxyTarget(os: HostOs): ProxyTarget {
  return os === "other" ? "terminal" : os;
}

export const PROXY_TARGETS: readonly { id: ProxyTarget; label: string }[] = [
  { id: "macos", label: "macOS" },
  { id: "windows", label: "Windows" },
  { id: "linux", label: "Linux (GNOME)" },
  { id: "firefox", label: "Firefox" },
  { id: "terminal", label: "Terminal / CLI" },
  { id: "ios-sim", label: "iOS Simulator" },
  { id: "android", label: "Android" },
];

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

// --- Honest per-step status (E-UISETUP.W6) -----------------------------------
// Each step has two independent truths: a fast LOCAL PROBE (does *this machine*
// trust the CA / route its proxy through Trawler? — the ~90% case) and OBSERVED
// TRAFFIC (has any request/connection actually reached Trawler, from any
// device?). The derivation below turns those into one honest line per step —
// green when a probe or capture confirms, a clarifying (never alarming) message
// when neither can, so the remote/manual/un-probeable cases guide instead of
// mislead. Pure data → unit-tested; the probes themselves live in Rust.

export type TrustProbe = "trusted" | "untrusted" | "unknown";
export type ProxyProbe = "ours" | "foreign" | "none" | "unknown";
export type StepTone = "ok" | "warn" | "muted";

// `short` is the ≤40-char fragment StatusLine renders — a verdict, not a
// sentence. Where the verdict also implies an instruction, `remedy` carries it
// out to a Notice: what is true and what to do about it are two elements.
export interface StepStatusView {
  glyph: string;
  tone: StepTone;
  short: string;
  remedy?: string;
}

// What the app has observed reaching Trawler this session, plus the running
// action's transient state. `decrypted` ⇒ a real https:// flow (trust proven);
// `trafficSeen` ⇒ any connection or flow at all (proxy proven, decrypt or not).
export interface Observed {
  decrypted: boolean;
  trafficSeen: boolean;
}
export type Running = "installing" | "setting" | null;

export function trustStatus(
  probe: TrustProbe,
  seen: Observed,
  running: Running,
  error?: string,
): StepStatusView {
  if (running === "installing")
    return { glyph: "⟳", tone: "muted", short: "installing…" };
  // An authoritative positive — the machine trusts the CA, proven by the probe or
  // a decrypted flow — wins over a stale action error: the CA is trusted whatever
  // the last click did.
  if (probe === "trusted")
    return {
      glyph: "✓",
      tone: "ok",
      short: "trusted on this machine",
    };
  if (seen.decrypted)
    return {
      glyph: "✓",
      tone: "ok",
      short: "trusted — confirmed by traffic",
    };
  if (error)
    return {
      glyph: "⚠",
      tone: "warn",
      short: "install failed",
      remedy: error,
    };
  if (seen.trafficSeen)
    return {
      glyph: "⚠",
      tone: "warn",
      short: "arriving but not decrypting",
      remedy: "install the certificate on the device you're capturing from",
    };
  if (probe === "unknown")
    return {
      glyph: "◐",
      tone: "muted",
      short: "can't check automatically",
      remedy: "we'll confirm when traffic arrives",
    };
  return {
    glyph: "○",
    tone: "muted",
    short: "not trusted on this machine yet",
  };
}

export function proxyStatus(
  probe: ProxyProbe,
  seen: Observed,
  running: Running,
  error?: string,
): StepStatusView {
  if (running === "setting")
    return { glyph: "⟳", tone: "muted", short: "setting the proxy…" };
  // Same rule as trustStatus: an authoritative positive (probe says the system
  // proxy is Trawler, or traffic is actually arriving) wins over a stale set error.
  if (probe === "ours")
    return {
      glyph: "✓",
      tone: "ok",
      short: "system proxy is set to Trawler",
    };
  if (seen.trafficSeen)
    return { glyph: "✓", tone: "ok", short: "receiving traffic" };
  if (error)
    return {
      glyph: "⚠",
      tone: "warn",
      short: "couldn't set the proxy",
      remedy: error,
    };
  if (probe === "foreign")
    return {
      glyph: "○",
      tone: "muted",
      short: "another proxy is set",
      remedy: "point it at Trawler here, or capture from a device configured to use Trawler",
    };
  if (probe === "unknown")
    return {
      glyph: "◐",
      tone: "muted",
      short: "can't check automatically",
      remedy: "we'll confirm when traffic arrives",
    };
  return {
    glyph: "○",
    tone: "muted",
    short: "no traffic reaching Trawler yet",
  };
}

export function verifyStatus(seen: Observed): StepStatusView {
  return seen.decrypted
    ? {
        glyph: "✓",
        tone: "ok",
        short: "capturing traffic",
      }
    : {
        glyph: "○",
        tone: "muted",
        short: "waiting for your first request",
      };
}

// P1 prove-it: the concrete, copyable command that generates the first request.
// Text only — the app never fires it (epic non-goal); it guides the user.
export function proveItSnippet(addr: string): string {
  return `curl -x http://${addr} https://example.com`;
}

// The off-command where one exists, so a caller can render it as a copyable
// payload without inferring which step is a command from its punctuation.
export function proxyOffSnippet(t: ProxyTarget): string | null {
  switch (t) {
    case "macos":
      return "networksetup -setwebproxystate Wi-Fi off && networksetup -setsecurewebproxystate Wi-Fi off";
    case "windows":
      return 'reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 0 /f';
    case "linux":
      return "gsettings set org.gnome.system.proxy mode 'none'";
    case "terminal":
      return "unset HTTPS_PROXY HTTP_PROXY";
    default:
      return null;
  }
}

export function proxyOffSteps(t: ProxyTarget): string[] {
  const snippet = proxyOffSnippet(t);
  switch (t) {
    case "macos":
      return ["Turn it off per enabled service (keeps the saved address):", snippet!];
    case "windows":
      return ["Disable the WinINET proxy (keeps the saved address):", snippet!];
    case "linux":
      return ["GNOME — back to no proxy:", snippet!];
    case "firefox":
      return ["Settings → Network Settings → Settings… → select \"No proxy\"."];
    case "terminal":
      return ["Clear the environment variables:", snippet!];
    case "ios-sim":
      return ["Turn the host Mac's proxy off (the macOS steps above), then restart the Simulator."];
    case "android":
      return ["Wi-Fi → your network → Proxy → None."];
  }
}
