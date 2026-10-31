// How each operating system's proxy, and a shell's, is turned back off. Setup's
// "to turn it off later" reads it, and the quit dialog reads setup's, so a
// reversal is written once (W11's S4). Pure data, no I/O, no React.

export type ProxyTarget = "macos" | "windows" | "linux" | "terminal";

// The off-command where one exists, so a caller can render it as a copyable
// payload without inferring which step is a command from its punctuation.
export function proxyOffSnippet(t: ProxyTarget): string {
  switch (t) {
    case "macos":
      return "networksetup -setwebproxystate Wi-Fi off && networksetup -setsecurewebproxystate Wi-Fi off";
    case "windows":
      return 'reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 0 /f';
    case "linux":
      return "gsettings set org.gnome.system.proxy mode 'none'";
    case "terminal":
      return "unset HTTPS_PROXY HTTP_PROXY";
  }
}

export function proxyOffSteps(t: ProxyTarget): string[] {
  const snippet = proxyOffSnippet(t);
  switch (t) {
    case "macos":
      return ["In Terminal, run:", snippet];
    case "windows":
      return ["Disable the WinINET proxy (keeps the saved address):", snippet];
    case "linux":
      return ["GNOME — back to no proxy:", snippet];
    case "terminal":
      return ["Clear the environment variables:", snippet];
  }
}
