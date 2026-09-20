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

// One numbered task of a platform's setup: what the user does, why it is worth
// doing, anything true about this platform they need before doing it, and the
// instructions. A task with no guide is one whose instructions do not exist yet
// — the device has nothing to point at until the network task is done — and it
// renders dimmed rather than explaining what cannot happen.
export interface Task {
  title: string;
  why?: string;
  note?: string;
  guide?: Guide;
}

// What one platform's tasks say. The words are the platform's, not the
// screen's: a user who answered "iPhone or iPad" is told about their iPhone in
// every line below, and never about a target, a platform or a route.
export interface PlatformSetup {
  trust: Task;
  route: Task;
  try: Task;
  tryCommand?: string;
  // The un-probeable state, worded as the promise it is: nothing here can read
  // that device's stores, and the first request it makes is what settles it.
  confirmOn: string;
}

// The addresses an off-machine platform's instructions are built from. Null
// until the user arms reach, which is why those platforms' tasks can be
// title-only.
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
  // What every sentence below the tiles calls this thing. "This machine" is the
  // one member whose name the host decides, because a Mac user reading "this
  // machine" is reading about a machine and a Mac user reading "this Mac" is
  // reading about theirs.
  device: string | null;
  // The phrase after "Open any website": you open one ON a phone and IN a
  // simulator, and the difference is the whole reason this is data.
  openWhere: string | null;
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
  this: {
    label: "This machine",
    device: null,
    openWhere: null,
    probesTrust: true,
    probesRoute: true,
    needsReach: false,
  },
  mac: {
    label: "Another Mac",
    device: "that Mac",
    openWhere: "on that Mac",
    probesTrust: false,
    probesRoute: false,
    needsReach: true,
  },
  windows: {
    label: "Another Windows PC",
    device: "that PC",
    openWhere: "on that PC",
    probesTrust: false,
    probesRoute: false,
    needsReach: true,
  },
  ubuntu: {
    label: "Another Ubuntu machine",
    device: "that machine",
    openWhere: "on that machine",
    probesTrust: false,
    probesRoute: false,
    needsReach: true,
  },
  "ios-sim": {
    label: "iOS Simulator",
    device: "the Simulator",
    openWhere: "in the Simulator",
    probesTrust: false,
    probesRoute: true,
    needsReach: false,
  },
  "android-sim": {
    label: "Android emulator",
    device: "the emulator",
    openWhere: "in the emulator",
    probesTrust: false,
    probesRoute: false,
    needsReach: false,
  },
  ios: {
    label: "iPhone or iPad",
    device: "your iPhone",
    openWhere: "on your iPhone",
    probesTrust: false,
    probesRoute: false,
    needsReach: true,
  },
  android: {
    label: "Android device",
    device: "your Android device",
    openWhere: "on your Android device",
    probesTrust: false,
    probesRoute: false,
    needsReach: true,
  },
};

// The host names itself, so "Set up this Mac" and "Open any website on this
// Mac" say what the user is looking at rather than what the code calls it.
const HOST_NOUN: Record<HostOs, string> = {
  macos: "this Mac",
  windows: "this PC",
  linux: "this machine",
  other: "this machine",
};

export function deviceName(id: PlatformId, os: HostOs): string {
  return PLATFORMS[id].device ?? HOST_NOUN[os];
}

function openWhere(id: PlatformId, os: HostOs): string {
  return PLATFORMS[id].openWhere ?? `on ${HOST_NOUN[os]}`;
}

// A task nobody can read the state of says when it WILL be settled, in the
// words it will be settled by (F4). The screen never renders a verdict about a
// machine it did not read.
export function confirmOn(id: PlatformId, os: HostOs): string {
  return `we'll confirm this on ${deviceName(id, os)}'s first request`;
}

// The network task's title, needed before any instruction exists: it is the
// task that makes the rest possible, so it is titled whether or not the rest
// can be written yet.
export function reachTaskTitle(id: PlatformId, os: HostOs): string {
  return `Allow ${deviceName(id, os)} to connect`;
}

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

// Why each task is worth doing, said once: the reason never changes with the
// platform, only the thing it is said about.
const PROVES_IT = "the first decrypted request is what proves setup works";

function tryTask(id: PlatformId, os: HostOs): Task {
  return { title: `Open any website ${openWhere(id, os)}`, why: PROVES_IT };
}

function localSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  const target = hostTarget(os);
  const trust = OS_TRUST[target];
  const host = HOST_NOUN[os];
  return {
    trust: {
      title: "Trust Trawler's certificate",
      why: `so ${host}'s HTTPS can be decrypted`,
      guide: {
        steps: [[SAVE_CA_STEP, { save: true }], ...trust.install],
        undoTitle: trust.undoTitle,
        undo: trust.undo,
      },
    },
    route: {
      title: `Send ${host}'s traffic to Trawler`,
      why: "its system proxy points at Trawler while Trawler runs",
      guide: osRoute(os === "other" ? "terminal" : target, proxyAddr),
    },
    try: tryTask("this", os),
    tryCommand: `curl -x http://${proxyAddr} https://example.com`,
    confirmOn: confirmOn("this", os),
  };
}

function anotherMachineSetup(
  id: PlatformId,
  os: HostOs,
  target: OsTarget,
  reach: ReachAddresses | null,
): PlatformSetup {
  const trust = OS_TRUST[target];
  const device = deviceName(id, os);
  return {
    trust: {
      title: `On ${device}, install Trawler's certificate`,
      why: `so ${device}'s HTTPS can be decrypted`,
      guide:
        reach === null
          ? undefined
          : {
              steps: [
                [
                  `On ${device}, open`,
                  { copy: reach.caCertUrl, label: "the certificate URL" },
                  `and save it as ${CA_FILENAME}.`,
                ],
                ...trust.install,
              ],
              undoTitle: trust.undoTitle,
              undo: trust.undo,
            },
    },
    route: {
      title: `On ${device}, send its traffic to Trawler`,
      guide: reach === null ? undefined : osRoute(target, reach.proxy),
    },
    try: tryTask(id, os),
    confirmOn: confirmOn(id, os),
  };
}

function deviceSetup(id: "ios" | "android", os: HostOs, reach: ReachAddresses | null): PlatformSetup {
  const ios = id === "ios";
  const device = deviceName(id, os);
  const trust: Guide | undefined =
    reach === null
      ? undefined
      : {
        steps: ios
          ? [
              downloadStep(`On ${device}`, reach.caCertUrl, "the profile"),
              ["Open it: Settings → General → VPN & Device Management → install the profile."],
              ["Then turn it on: Settings → General → About → Certificate Trust Settings."],
              ["No browser on it?", SAVE_CA_STEP, { save: true }, "and carry the file over instead."],
            ]
          : [
              downloadStep(`On ${device}`, reach.caCertUrl, "the certificate"),
              [
                "Install it: Settings → Security → Encryption & credentials → Install a certificate → CA certificate.",
              ],
              [ANDROID_USER_CA],
              ["No browser on it?", SAVE_CA_STEP, { save: true }, "and carry the file over instead."],
            ],
      };
  const route: Guide | undefined =
    reach === null
      ? undefined
      : {
        steps: ios
          ? [
              ["Settings → Wi-Fi → ⓘ on the network it is on → Configure Proxy → Manual."],
              ["Put the server and port above into it, and save."],
            ]
          : [
              ["Settings → Wi-Fi → the network it is on → Proxy → Manual."],
              ["Put the server and port above into it, and save."],
            ],
        undoTitle: DEVICE_PROXY_UNDO,
        undo: ios
          ? [["Settings → Wi-Fi → the network → Configure Proxy → Off."]]
          : [["Wi-Fi → the network → Proxy → None."]],
      };
  return {
    trust: { title: `On ${device}, install the certificate`, guide: trust },
    route: { title: `On ${device}, set the Wi-Fi proxy`, guide: route },
    try: tryTask(id, os),
    confirmOn: confirmOn(id, os),
  };
}

function emulatorSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  const { port } = splitProxyAddr(proxyAddr);
  const trust: Guide = {
    steps: [
      [SAVE_CA_STEP, { save: true }],
      [
        "Push it to the emulator:",
        { copy: "adb push trawler-ca.pem /sdcard/Download/", label: "the push command" },
      ],
      [
        "Install it: Settings → Security → Encryption & credentials → Install a certificate → CA certificate.",
      ],
      [ANDROID_USER_CA],
    ],
    undoTitle: "to remove it later",
    undo: [
      [
        "Settings → Security → Encryption & credentials → Trusted credentials → User → remove Trawler Proxy CA.",
      ],
    ],
  };
  const route: Guide = {
    steps: [
      ["Settings → Wi-Fi → the network it is on → Proxy → Manual."],
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
  };
  return {
    trust: { title: "In the emulator, install the certificate", guide: trust },
    route: { title: "In the emulator, set the Wi-Fi proxy", guide: route },
    try: tryTask("android-sim", os),
    confirmOn: confirmOn("android-sim", os),
  };
}

// Two stores, two answers. The Simulator follows this Mac's system proxy, so
// the host's own routing guide is its routing guide; its trust store is its
// own, and Apple's supported way in is simctl against the booted device.
function simulatorSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  const host = HOST_NOUN[os];
  const trust: Guide = {
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
  };
  return {
    trust: {
      title: "Trust Trawler's certificate in the Simulator",
      why: `the Simulator keeps its own certificate store — ${host}'s does not cover it`,
      guide: trust,
    },
    route: {
      title: `Send ${host}'s traffic to Trawler`,
      why: `the Simulator follows ${host}'s system proxy`,
      guide: osRoute(os === "other" ? "terminal" : hostTarget(os), proxyAddr),
    },
    try: tryTask("ios-sim", os),
    confirmOn: confirmOn("ios-sim", os),
  };
}

// Every platform always has its tasks, because the shape of the whole job is
// what a first-time user reads before doing any of it. What a platform without
// a reachable address does NOT have is instructions: those need an address
// that does not exist yet, and inventing one is what the network task exists to
// prevent. A task with no guide renders dimmed under its number.
export function setupFor(id: PlatformId, ctx: SetupContext): PlatformSetup {
  const reach = needsReach(id) ? ctx.reach : null;
  switch (id) {
    case "this":
      return localSetup(ctx.os, ctx.proxyAddr);
    case "mac":
      return anotherMachineSetup(id, ctx.os, "macos", reach);
    case "windows":
      return anotherMachineSetup(id, ctx.os, "windows", reach);
    case "ubuntu":
      return anotherMachineSetup(id, ctx.os, "linux", reach);
    case "ios-sim":
      return simulatorSetup(ctx.os, ctx.proxyAddr);
    case "android-sim":
      return emulatorSetup(ctx.os, ctx.proxyAddr);
    case "ios":
    case "android":
      return deviceSetup(id, ctx.os, reach);
  }
}
