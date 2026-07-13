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

export function canAutoInstall(os: HostOs): boolean {
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

const DOWNLOAD = "Save Trawler's CA certificate — GET /ca.crt on the control API — as trawler-ca.pem.";

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
