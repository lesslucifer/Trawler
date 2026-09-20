// The platform axis: the one question the setup screen asks, and everything
// that follows from the answer. A platform is what the user is holding — this
// machine, another machine, a simulator, a device — never an operating system,
// because two platforms can share one (iOS Simulator and iPhone) and one
// platform's OS is only ever an implementation detail of its instructions.
// Pure data and pure functions; no I/O, no React.

import type { ProxyTarget } from "./proxyTargets";
import { proxyOffSnippet, proxyOffSteps } from "./proxyTargets";

export type HostOs = "macos" | "windows" | "linux" | "other";

export function hostOs(ua: string = navigator.userAgent): HostOs {
  if (ua.includes("Windows")) return "windows";
  if (ua.includes("Mac OS X") || ua.includes("Macintosh")) return "macos";
  if (ua.includes("Linux")) return "linux";
  return "other";
}

// "This OS has a one-click local setup path" — one concept shared by the trust
// and the routing preconditions, so it is not duplicated.
export function canAutoSetup(os: HostOs): boolean {
  return os === "macos" || os === "windows";
}

export function splitProxyAddr(addr: string): { host: string; port: number } {
  const at = addr.lastIndexOf(":");
  return { host: addr.slice(0, at), port: Number(addr.slice(at + 1)) };
}

export const CA_FILENAME = "trawler-ca.pem";

// Step one of every manual route names this file, so the screen owes the user a
// way to produce it: the save control sits at the end of this sentence, which is
// why it carries no terminal period.
export const SAVE_CA_STEP = `Save Trawler's certificate as ${CA_FILENAME}`;

export const PLATFORM_IDS = [
  "this",
  "mac",
  "windows",
  "ubuntu",
  "ios-sim",
  "android-sim",
  "ios",
  "android",
] as const;

export type PlatformId = (typeof PLATFORM_IDS)[number];

// A step is a sentence with things in it: copyable values the user takes
// verbatim, and the save control that produces the file the next step names.
// Modelled as parts rather than as a string the screen pattern-matches, so a
// command that drifts from its prose cannot silently stop being copyable.
export type StepPart = string | { copy: string; label: string } | { save: true };
export type GuideStep = readonly StepPart[];

export interface Guide {
  steps: GuideStep[];
  undoTitle?: string;
  undo?: GuideStep[];
}

// What one platform's preconditions say. A missing guide is a precondition with
// nothing for the user to do — the note above it is the whole instruction.
export interface PlatformSetup {
  trust?: Guide;
  trustNote?: string;
  route?: Guide;
  routeNote?: string;
  tryIt: string;
  tryCommand?: string;
}

// The addresses an off-machine platform's instructions are built from. Null
// until the user arms reach, which is why `setupFor` can refuse to answer.
export interface ReachAddresses {
  proxy: string;
  caCertUrl: string;
}

export interface SetupContext {
  os: HostOs;
  proxyAddr: string;
  reach: ReachAddresses | null;
}

interface PlatformFacts {
  label: string;
  // The two probes read THIS machine, and a platform may share one of its
  // stores without sharing the other: the iOS Simulator follows this Mac's
  // system proxy and keeps a certificate store of its own. Two facts, never
  // one — borrowing an answer about the wrong store is worse than no answer.
  probesTrust: boolean;
  probesRoute: boolean;
  // Do this platform's instructions need an address beyond loopback?
  needsReach: boolean;
}

const PLATFORMS: Record<PlatformId, PlatformFacts> = {
  this: { label: "This machine", probesTrust: true, probesRoute: true, needsReach: false },
  mac: { label: "Another Mac", probesTrust: false, probesRoute: false, needsReach: true },
  windows: { label: "Another Windows PC", probesTrust: false, probesRoute: false, needsReach: true },
  ubuntu: { label: "Another Ubuntu machine", probesTrust: false, probesRoute: false, needsReach: true },
  "ios-sim": { label: "iOS Simulator", probesTrust: false, probesRoute: true, needsReach: false },
  "android-sim": { label: "Android emulator", probesTrust: false, probesRoute: false, needsReach: false },
  ios: { label: "iPhone or iPad", probesTrust: false, probesRoute: false, needsReach: true },
  android: { label: "Android device", probesTrust: false, probesRoute: false, needsReach: true },
};

const OS_NAME: Record<HostOs, string | null> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
  other: null,
};

export function platformLabel(id: PlatformId, os: HostOs): string {
  const { label } = PLATFORMS[id];
  if (id !== "this") return label;
  const named = OS_NAME[os];
  return named ? `${label} — ${named}` : label;
}

export function platformChoices(os: HostOs): { id: PlatformId; label: string }[] {
  return PLATFORM_IDS.map((id) => ({ id, label: platformLabel(id, os) }));
}

export function probesTrust(id: PlatformId): boolean {
  return PLATFORMS[id].probesTrust;
}

export function probesRoute(id: PlatformId): boolean {
  return PLATFORMS[id].probesRoute;
}

export function needsReach(id: PlatformId): boolean {
  return PLATFORMS[id].needsReach;
}

// The three systems the screen can give commands for. An unknown host gets the
// Linux trust route and the shell's proxy route: both are the ones that need no
// knowledge of the desktop it is running on.
type OsTarget = "macos" | "windows" | "linux";

const hostTarget = (os: HostOs): OsTarget => (os === "other" ? "linux" : os);

const OS_TRUST: Record<OsTarget, { install: GuideStep[]; undoTitle: string; undo: GuideStep[] }> = {
  macos: {
    install: [
      [
        "Run it in a terminal:",
        {
          copy: "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain trawler-ca.pem",
          label: "the trust command",
        },
      ],
      ["Or double-click the file and set the Trawler CA to Always Trust."],
      ["Firefox keeps its own store — import it there too."],
    ],
    undoTitle: "to remove it later",
    undo: [
      [
        "Run:",
        {
          copy: 'sudo security delete-certificate -c "Trawler Proxy CA" /Library/Keychains/System.keychain',
          label: "the removal command",
        },
      ],
      ["Or delete the Trawler Proxy CA entry in Keychain Access → System."],
    ],
  },
  windows: {
    install: [
      [
        "In an elevated terminal run:",
        { copy: "certutil -addstore -f ROOT trawler-ca.pem", label: "the trust command" },
      ],
      ["Firefox keeps its own store — import it there too."],
    ],
    undoTitle: "to remove it later",
    undo: [
      [
        "In an elevated terminal run:",
        { copy: 'certutil -delstore ROOT "Trawler Proxy CA"', label: "the removal command" },
      ],
    ],
  },
  linux: {
    install: [
      [
        "Run it in a terminal:",
        {
          copy: "sudo cp trawler-ca.pem /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates",
          label: "the trust command",
        },
      ],
      [
        "Chrome reads the NSS store instead:",
        {
          copy: "certutil -d sql:$HOME/.pki/nssdb -A -t C,, -n trawler -i trawler-ca.pem",
          label: "the NSS command",
        },
      ],
      ["Firefox keeps its own store — import it there too."],
    ],
    undoTitle: "to remove it later",
    undo: [
      [
        "Run:",
        {
          copy: "sudo rm /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates --fresh",
          label: "the removal command",
        },
      ],
      ["NSS store:", { copy: "certutil -d sql:$HOME/.pki/nssdb -D -n trawler", label: "the NSS removal" }],
    ],
  },
};

function routeCommand(t: ProxyTarget, addr: string): string | null {
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

const ROUTE_LEAD: Record<OsTarget | "terminal", string> = {
  macos: "Run — this sets the Wi-Fi service; change the name for Ethernet:",
  windows: "Run:",
  linux: "GNOME — run:",
  terminal: "Export the proxy in your shell (curl, npm and most CLIs honour it):",
};

// proxyOffSteps owns the reversal's prose and splices its command in as a step;
// the screen needs the same sentences with the command as a value to take, so
// the command step is replaced in place rather than the prose being retyped.
function reversal(t: OsTarget | "terminal"): GuideStep[] {
  const command = proxyOffSnippet(t);
  const parts = proxyOffSteps(t).map((step) =>
    step === command ? { copy: step, label: "the proxy-off command" } : step,
  );
  return [parts];
}

function osRoute(t: OsTarget | "terminal", addr: string): Guide {
  return {
    steps: [[ROUTE_LEAD[t], { copy: routeCommand(t, addr)!, label: "the proxy command" }]],
    undoTitle: "to turn it off later",
    undo: reversal(t),
  };
}

const DEVICE_PROXY_UNDO = "to turn it off later";

// The alias every Android emulator resolves to the machine it runs on, which is
// why the emulator needs no reach: it is already inside this host's loopback.
const EMULATOR_HOST = "10.0.2.2";

const ANDROID_USER_CA =
  "Apps targeting Android 7+ only trust user CAs when their network security config opts in.";

function downloadStep(where: string, caCertUrl: string, what: string): GuideStep {
  return [`${where}, open`, { copy: caCertUrl, label: "the certificate URL" }, `— it downloads ${what}.`];
}

function localSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  const target = hostTarget(os);
  const trust = OS_TRUST[target];
  return {
    trust: {
      steps: [[SAVE_CA_STEP, { save: true }], ...trust.install],
      undoTitle: trust.undoTitle,
      undo: trust.undo,
    },
    route: osRoute(os === "other" ? "terminal" : target, proxyAddr),
    tryIt: "make any request — or run this",
    tryCommand: `curl -x http://${proxyAddr} https://example.com`,
  };
}

function anotherMachineSetup(target: OsTarget, reach: ReachAddresses): PlatformSetup {
  const trust = OS_TRUST[target];
  return {
    trust: {
      steps: [
        [
          "On that machine, open",
          { copy: reach.caCertUrl, label: "the certificate URL" },
          `and save it as ${CA_FILENAME}.`,
        ],
        ...trust.install,
      ],
      undoTitle: trust.undoTitle,
      undo: trust.undo,
    },
    route: osRoute(target, reach.proxy),
    tryIt: "open any site on that machine — the first decrypted flow is what ends setup",
  };
}

function deviceSetup(id: "ios" | "android", reach: ReachAddresses): PlatformSetup {
  const ios = id === "ios";
  return {
    trust: {
      steps: ios
        ? [
            downloadStep("On the device", reach.caCertUrl, "the profile"),
            ["Open it: Settings → General → VPN & Device Management → install the profile."],
            ["Then turn it on: Settings → General → About → Certificate Trust Settings."],
            ["No browser on it?", SAVE_CA_STEP, { save: true }, "and carry the file over instead."],
          ]
        : [
            downloadStep("On the device", reach.caCertUrl, "the certificate"),
            ["Install it: Settings → Security → Encryption & credentials → Install a certificate → CA certificate."],
            [ANDROID_USER_CA],
            ["No browser on it?", SAVE_CA_STEP, { save: true }, "and carry the file over instead."],
          ],
    },
    route: {
      steps: ios
        ? [
            ["On the device: Settings → Wi-Fi → the network it is on → Configure Proxy → Manual."],
            ["Put the server and port above into it, and save."],
          ]
        : [
            ["On the device: Settings → Wi-Fi → the network it is on → Proxy → Manual."],
            ["Put the server and port above into it, and save."],
          ],
      undoTitle: DEVICE_PROXY_UNDO,
      undo: ios
        ? [["Settings → Wi-Fi → the network → Configure Proxy → Off."]]
        : [["Wi-Fi → the network → Proxy → None."]],
    },
    tryIt: "open any site on the device — the first decrypted flow is what ends setup",
  };
}

function emulatorSetup(proxyAddr: string): PlatformSetup {
  const { port } = splitProxyAddr(proxyAddr);
  return {
    trust: {
      steps: [
        [SAVE_CA_STEP, { save: true }],
        [
          "Push it to the emulator:",
          { copy: "adb push trawler-ca.pem /sdcard/Download/", label: "the push command" },
        ],
        ["Install it: Settings → Security → Encryption & credentials → Install a certificate → CA certificate."],
        [ANDROID_USER_CA],
      ],
      undoTitle: "to remove it later",
      undo: [
        ["Settings → Security → Encryption & credentials → Trusted credentials → User → remove Trawler Proxy CA."],
      ],
    },
    route: {
      steps: [
        ["In the emulator: Settings → Wi-Fi → the network it is on → Proxy → Manual."],
        [
          "Server",
          { copy: EMULATOR_HOST, label: "the emulator's host" },
          "port",
          { copy: String(port), label: "the proxy port" },
          "— the alias every emulator resolves to the machine it runs on.",
        ],
        [
          "Or start it with the proxy already set:",
          {
            copy: `emulator -avd <name> -http-proxy http://${EMULATOR_HOST}:${port}`,
            label: "the emulator command",
          },
        ],
      ],
      undoTitle: DEVICE_PROXY_UNDO,
      undo: [["Wi-Fi → the network → Proxy → None."]],
    },
    tryIt: "open any site in the emulator — the first decrypted flow is what ends setup",
  };
}

// Two stores, two answers. The Simulator follows this Mac's system proxy, so
// the host's own routing guide is its routing guide; its trust store is its
// own, and Apple's supported way in is simctl against the booted device.
function simulatorSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  return {
    trustNote: "the Simulator keeps its own certificate store — this Mac's does not cover it",
    trust: {
      steps: [
        [SAVE_CA_STEP, { save: true }],
        [
          "With the Simulator running, run:",
          {
            copy: `xcrun simctl keychain booted add-root-cert ${CA_FILENAME}`,
            label: "the trust command",
          },
        ],
        ["Restart the Simulator so apps pick the new root up."],
      ],
      undoTitle: "to remove it later",
      undo: [
        [
          "Reset that Simulator's keychain — this clears everything in it, not only Trawler's root:",
          { copy: "xcrun simctl keychain booted reset", label: "the keychain reset" },
        ],
      ],
    },
    routeNote: "the Simulator uses this Mac's system proxy — setting it here covers it",
    route: osRoute(os === "other" ? "terminal" : hostTarget(os), proxyAddr),
    tryIt: "open any site in the Simulator",
  };
}

// Null is the honest answer for a platform whose every instruction needs an
// address that does not exist yet: there are no steps to show, and inventing
// one is what the reach precondition exists to prevent.
export function setupFor(id: PlatformId, ctx: SetupContext): PlatformSetup | null {
  if (needsReach(id) && ctx.reach === null) return null;
  switch (id) {
    case "this":
      return localSetup(ctx.os, ctx.proxyAddr);
    case "mac":
      return anotherMachineSetup("macos", ctx.reach!);
    case "windows":
      return anotherMachineSetup("windows", ctx.reach!);
    case "ubuntu":
      return anotherMachineSetup("linux", ctx.reach!);
    case "ios-sim":
      return simulatorSetup(ctx.os, ctx.proxyAddr);
    case "android-sim":
      return emulatorSetup(ctx.proxyAddr);
    case "ios":
    case "android":
      return deviceSetup(id, ctx.reach!);
  }
}
