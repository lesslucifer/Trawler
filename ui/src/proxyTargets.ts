// The proxy-client vocabulary: which clients a user can be told to point at
// Trawler, and how each one is turned back off. It is keyed by client rather
// than by platform because that is what the quit dialog asks — "a proxy is set
// on this machine, which client do you need the steps for" — and the onboarding
// screen's platform axis reads the reversals from here rather than restating
// them. Pure data, no I/O, no React.

import type { HostOs } from "./setup";

export type ProxyTarget =
  | "macos"
  | "windows"
  | "linux"
  | "firefox"
  | "terminal"
  | "ios-sim"
  | "android";

export function defaultProxyTarget(os: HostOs): ProxyTarget {
  return os === "other" ? "terminal" : os;
}

// Keyed by the union so tsc proves the list offers every target: a member added
// to ProxyTarget without a label here fails to compile. A runtime check over the
// list could only ever compare it with itself.
const PROXY_TARGET_LABELS: Record<ProxyTarget, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux (GNOME)",
  firefox: "Firefox",
  terminal: "Terminal / CLI",
  "ios-sim": "iOS Simulator",
  android: "Android",
};

export const PROXY_TARGETS: readonly { id: ProxyTarget; label: string }[] = (
  Object.keys(PROXY_TARGET_LABELS) as ProxyTarget[]
).map((id) => ({ id, label: PROXY_TARGET_LABELS[id] }));

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
      return ['Settings → Network Settings → Settings… → select "No proxy".'];
    case "terminal":
      return ["Clear the environment variables:", snippet!];
    case "ios-sim":
      return ["Turn the host Mac's proxy off (the macOS steps above), then restart the Simulator."];
    case "android":
      return ["Wi-Fi → your network → Proxy → None."];
  }
}
