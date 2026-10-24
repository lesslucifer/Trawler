// The platform axis: the one question the setup screen asks, and everything
// that follows from the answer. A platform is what the user is holding — this
// machine, another machine, a simulator, a device — never an operating system,
// because two platforms can share one (iOS Simulator and iPhone) and one
// platform's OS is only ever an implementation detail of its instructions.
// Pure data and pure functions; no I/O, no React.

import type { CaFormat, ReachState } from "./api";
import type { ProxyTarget } from "./proxyTargets";
import { proxyOffSnippet, proxyOffSteps } from "./proxyTargets";

export type { CaFormat };

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

// The address reach is armed on, where this machine no longer holds it (R1): a
// network change leaves the listener bound to an address no device routes to,
// and the core reports the two without comparing them.
export function lostAddress(reach: ReachState): string | null {
  if (!reach.armed || reach.proxy === null) return null;
  const host = splitProxyAddr(reach.proxy).host.replace(/^\[(.*)\]$/, "$1");
  return reach.interfaces.some((i) => i.address === host) ? null : host;
}

// One file per format a platform's installer opens, each under the name its
// download suggests, so the file a step names is the file the user has.
export const CA_FILES: Record<CaFormat, string> = {
  pem: "trawler-ca.pem",
  cer: "trawler-ca.cer",
  mobileconfig: "trawler-ca.mobileconfig",
};

const PEM_FILE = CA_FILES.pem;

// A route that starts on this machine names the file first, so the screen owes
// the user a way to produce it: the save control sits at the end of this
// sentence, which is why it carries no terminal period.
export const saveCaStep = (format: CaFormat): GuideStep => [
  `Save Trawler's certificate as ${CA_FILES[format]}`,
  { save: format },
];

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

// A step is a sentence with things in it: values the user takes verbatim, the
// command they run, and the save control that produces the file the next step
// names. Modelled as parts rather than as a string the screen pattern-matches,
// so a command that drifts from its prose cannot silently stop being copyable.
//
// A value and a command are two parts, not one, because they are read in two
// places: a value is typed into a field beside the sentence that names it, and
// a command is carried to a terminal whole. The screen renders the first inline
// and the second on a surface of its own, and neither can be mistaken for the
// other by looking at its punctuation.
export interface CopyPart {
  copy: string;
  label: string;
  wire: string;
  // A value a phone opens by scanning rather than typing (DEC-57).
  scan?: boolean;
}
export interface CommandPart {
  command: string;
  label: string;
}
export interface SavePart {
  save: CaFormat;
}
export type StepPart = string | CopyPart | CommandPart | SavePart;
export type GuideStep = readonly StepPart[];

export const isCommand = (part: StepPart): part is CommandPart =>
  typeof part === "object" && "command" in part;

export const isScan = (part: StepPart): part is CopyPart =>
  typeof part === "object" && "copy" in part && part.scan === true;

export const isSave = (part: StepPart): part is SavePart =>
  typeof part === "object" && "save" in part;

// One platform's instructions for one task. `steps` is the route the reader
// takes; everything else is what a numbered step must not be. A note is true of
// this platform and is nothing to do — numbering it asks the reader to perform
// a fact. An alternative is a second route to the same end, so putting it in
// the sequence tells the reader to do the job twice.
export interface Guide {
  steps: GuideStep[];
  notes?: string[];
  alternatives?: GuideStep[];
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
  // What a user who already did the certificate steps checks next when traffic
  // arrives and does not decrypt: the one thing this platform's users miss.
  // None where a refusal is never this platform's certificate task's to say.
  notDecrypting?: string;
  // What a refusal after another device's proof most likely is, where this
  // platform knows better than every platform's answer (DEC-63 rule 7).
  refusedAfterProof?: string;
}

// The addresses an off-machine platform's instructions are built from. Null
// until the user arms reach, which is why those platforms' tasks can be
// title-only.
export interface ReachAddresses {
  proxy: string;
  caUrls: Record<CaFormat, string>;
}

// Where this machine's save put each certificate file, by the path the save
// returned. A file not saved yet is read from Downloads, where the save dialog
// opens (W7's C1).
export type SavedCa = Partial<Record<CaFormat, string>>;

export interface SetupContext {
  os: HostOs;
  proxyAddr: string;
  reach: ReachAddresses | null;
  saved?: SavedCa;
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
  // A store of its own that this machine can still read: each Simulator
  // running keeps one (W7's Q1).
  probesOwnStore: boolean;
  // It sends through this machine's own system proxy, so this machine's
  // traffic is on its wire for as long as it is routed: nothing unmarked is
  // its own, ever, and only its marked request settles anything (DEC-63).
  sharesHostWire: boolean;
  // Do this platform's instructions need an address beyond loopback?
  needsReach: boolean;
  // A device that cannot paste opens a value by scanning it (DEC-57).
  scans: boolean;
  // The one host this platform runs beside, where it cannot run on every host.
  onlyBeside?: HostOs;
}

const PLATFORMS: Record<PlatformId, PlatformFacts> = {
  this: {
    label: "This machine",
    device: null,
    openWhere: null,
    probesTrust: true,
    probesRoute: true,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: false,
    scans: false,
  },
  mac: {
    label: "Another Mac",
    device: "that Mac",
    openWhere: "on that Mac",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: true,
    scans: false,
  },
  windows: {
    label: "Another Windows PC",
    device: "that PC",
    openWhere: "on that PC",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: true,
    scans: false,
  },
  ubuntu: {
    label: "Another Ubuntu machine",
    device: "that machine",
    openWhere: "on that machine",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: true,
    scans: false,
  },
  "ios-sim": {
    label: "iOS Simulator",
    device: "the Simulator",
    openWhere: "in the Simulator",
    probesTrust: false,
    probesRoute: true,
    probesOwnStore: true,
    sharesHostWire: true,
    needsReach: false,
    scans: false,
    onlyBeside: "macos",
  },
  "android-sim": {
    label: "Android emulator",
    device: "the emulator",
    openWhere: "in the emulator",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: false,
    scans: false,
  },
  ios: {
    label: "iPhone or iPad",
    device: "your iPhone",
    openWhere: "on your iPhone",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: true,
    scans: true,
  },
  android: {
    label: "Android device",
    device: "your Android device",
    openWhere: "on your Android device",
    probesTrust: false,
    probesRoute: false,
    probesOwnStore: false,
    sharesHostWire: false,
    needsReach: true,
    scans: true,
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
  return `${deviceName(id, os)}'s first request confirms it`;
}

// The certificate is settled by a request that could decrypt, and plain HTTP
// never could, so its promise names the kind of request that will settle it.
export function confirmHttpsOn(id: PlatformId, os: HostOs): string {
  return `${deviceName(id, os)}'s first HTTPS request confirms it`;
}

// Another device's own request, for when its traffic cannot be told from this
// machine's (DEC-63): an address carrying its mark, opened on that device. The
// try task's title already says to open a website there, so this is the one.
export function checkStep(id: PlatformId, os: HostOs, url: string): GuideStep {
  return [
    `or open this one ${openWhere(id, os)}`,
    { copy: url, label: "the address to open", wire: "check url", scan: PLATFORMS[id].scans },
  ];
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
  return PLATFORM_IDS.filter((id) => (PLATFORMS[id].onlyBeside ?? os) === os).map((id) => ({
    id,
    label: platformLabel(id, os),
  }));
}

// Whether this host has a reader for its own trust store and system proxy. A
// platform that shares a store asks it; a host with no reader answers unknown,
// and a re-read of nothing is not offered. Linux reads its CA bundle and
// GNOME's proxy (D8), and says unknown wherever either is not there.
export function canProbe(os: HostOs): boolean {
  return os !== "other";
}

export function probesTrust(id: PlatformId): boolean {
  return PLATFORMS[id].probesTrust;
}

export function probesRoute(id: PlatformId): boolean {
  return PLATFORMS[id].probesRoute;
}

export function probesOwnStore(id: PlatformId): boolean {
  return PLATFORMS[id].probesOwnStore;
}

export function sharesHostWire(id: PlatformId): boolean {
  return PLATFORMS[id].sharesHostWire;
}

export function needsReach(id: PlatformId): boolean {
  return PLATFORMS[id].needsReach;
}

// The three systems the screen can give commands for. An unknown host gets the
// Linux trust route and the shell's proxy route: both are the ones that need no
// knowledge of the desktop it is running on.
type OsTarget = "macos" | "windows" | "linux";

const hostTarget = (os: HostOs): OsTarget => (os === "other" ? "linux" : os);

const NOT_DECRYPTING_SYSTEM_STORE =
  "Already did? A browser reads the system's store when it starts — restart it. Apps that pin their own certificate never decrypt.";

const NOT_DECRYPTING_LINUX =
  "Already did? The system command covers curl and the system's tools, not browsers: Firefox and Chrome each keep a store of their own, and each has its own step below. Apps that pin their own certificate never decrypt.";

const REFUSED_AFTER_PROOF_LINUX =
  "something else is connecting and not decrypting — one request already did, so it is a browser with a store of its own (Firefox, Chrome), an app that pins its certificate, or another device.";

// Linux has three stores and the browsers read neither of the other two:
// Firefox keeps its own, Chrome the NSS database, and curl and the system's
// tools the system bundle. Each is a step, browsers first, because the try task
// opens a browser (C1). Chromium moved the NSS database at M146 and keeps an
// older one where it exists (C3); a Chrome never opened has made neither, and
// certutil adds only to a database it was asked to create. certutil is not on
// a stock Ubuntu (C2).
const FIREFOX_AUTHORITIES = "In Firefox: Settings → Privacy & Security → Certificates → View Certificates → Authorities";
const NSS_DB = '"sql:$([ -d ~/.pki/nssdb ] && echo ~/.pki/nssdb || echo ~/.local/share/pki/nssdb)"';
const NSS_DB_MADE =
  "{ [ -d ~/.pki/nssdb ] || [ -d ~/.local/share/pki/nssdb ] || { mkdir -p ~/.local/share/pki/nssdb && certutil -d sql:$HOME/.local/share/pki/nssdb -N --empty-password; }; }";
const FOR_CHROME = "For Chrome, run:";
const FOR_SYSTEM_TOOLS = "For curl and the system's tools, run:";

interface OsTrust {
  install: GuideStep[];
  alternatives?: GuideStep[];
  undoTitle: string;
  undo: GuideStep[];
  notDecrypting: string;
  refusedAfterProof?: string;
}

// Windows' commands are written for PowerShell, the shell Windows opens from
// Start, Win+X and Terminal: 5.1 there rejects `&&` (C1).
const POWERSHELL_ADMIN = "In PowerShell as administrator, run:";

// The trust command reads the certificate file at `file`, which is wherever
// the step before it left it.
const OS_TRUST: Record<OsTarget, (file: string) => OsTrust> = {
  macos: (file) => ({
    install: [
      [
        "Run it in a terminal:",
        {
          command:
            `sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ${file}`,
          label: "the trust command",
        },
      ],
    ],
    alternatives: [["Or double-click the file and set the Trawler CA to Always Trust."]],
    undoTitle: "to remove it later",
    undo: [
      [
        "Run:",
        {
          command:
            'sudo security delete-certificate -c "Trawler Proxy CA" /Library/Keychains/System.keychain',
          label: "the removal command",
        },
      ],
      ["Or delete the Trawler Proxy CA entry in Keychain Access → System."],
    ],
    notDecrypting: NOT_DECRYPTING_SYSTEM_STORE,
  }),
  windows: (file) => ({
    install: [[POWERSHELL_ADMIN, { command: `certutil -addstore -f ROOT ${file}`, label: "the trust command" }]],
    undoTitle: "to remove it later",
    undo: [
      [POWERSHELL_ADMIN, { command: 'certutil -delstore ROOT "Trawler Proxy CA"', label: "the removal command" }],
    ],
    notDecrypting: NOT_DECRYPTING_SYSTEM_STORE,
  }),
  linux: (file) => ({
    install: [
      [`${FIREFOX_AUTHORITIES} → Import → ${PEM_FILE}, then tick "Trust this CA to identify websites".`],
      [
        FOR_CHROME,
        {
          command: `sudo apt install -y libnss3-tools && ${NSS_DB_MADE} && certutil -d ${NSS_DB} -A -t C,, -n trawler -i ${file}`,
          label: "the Chrome command",
        },
      ],
      [
        FOR_SYSTEM_TOOLS,
        {
          command: `sudo cp ${file} /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates`,
          label: "the trust command",
        },
      ],
    ],
    undoTitle: "to remove it later",
    undo: [
      [`${FIREFOX_AUTHORITIES} → Trawler Proxy CA → Delete or Distrust.`],
      [FOR_CHROME, { command: `certutil -d ${NSS_DB} -D -n trawler`, label: "the Chrome removal command" }],
      [
        FOR_SYSTEM_TOOLS,
        {
          command:
            "sudo rm /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates --fresh",
          label: "the removal command",
        },
      ],
    ],
    notDecrypting: NOT_DECRYPTING_LINUX,
    refusedAfterProof: REFUSED_AFTER_PROOF_LINUX,
  }),
};

// A browser on another computer saves the certificate to Downloads, and so
// does this machine's save dialog until the user points it elsewhere; a
// terminal opens elsewhere, so a command names the folder (C2, W7's C1).
// Linux names that folder in the session's language, so it asks for it (C4).
const DOWNLOADED: Record<OsTarget, (file: string) => string> = {
  macos: (file) => `~/Downloads/${file}`,
  windows: (file) => `"$HOME\\Downloads\\${file}"`,
  linux: (file) => `"$(xdg-user-dir DOWNLOAD)/${file}"`,
};

// A path the user chose may hold a space or a quote, so it is quoted for the
// shell its command runs in: PowerShell on Windows, a POSIX shell elsewhere.
const posixQuoted = (path: string) => `'${path.replace(/'/g, "'\\''")}'`;
const QUOTED: Record<OsTarget, (path: string) => string> = {
  macos: posixQuoted,
  windows: (path) => `'${path.replace(/'/g, "''")}'`,
  linux: posixQuoted,
};

function savedFile(target: OsTarget, format: CaFormat, saved: SavedCa = {}): string {
  const path = saved[format];
  return path === undefined ? DOWNLOADED[target](CA_FILES[format]) : QUOTED[target](path);
}

function routeCommand(t: ProxyTarget, addr: string): string | null {
  const { host, port } = splitProxyAddr(addr);
  switch (t) {
    case "macos":
      return `networksetup -setwebproxy Wi-Fi ${host} ${port} && networksetup -setsecurewebproxy Wi-Fi ${host} ${port}`;
    case "windows":
      return `reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyServer /t REG_SZ /d ${addr} /f; reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f`;
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
  windows: "In PowerShell, run:",
  linux: "GNOME — run:",
  terminal: "Export the proxy in your shell (curl, npm and most CLIs honour it):",
};

// proxyOffSteps owns the reversal's prose and splices its command in as a step;
// the screen needs the same sentences with the command as a value to take, so
// the command step is replaced in place rather than the prose being retyped.
function reversal(t: OsTarget | "terminal"): GuideStep[] {
  const command = proxyOffSnippet(t);
  const said = proxyOffSteps(t).map((step) =>
    step === command ? { command: step, label: "the proxy-off command" } : step,
  );
  return [said];
}

function osRoute(t: OsTarget | "terminal", addr: string): Guide {
  return {
    steps: [[ROUTE_LEAD[t], { command: routeCommand(t, addr)!, label: "the proxy command" }]],
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

const CA_NAME = "Trawler Proxy CA";

const IOS_UNVERIFIED = "iOS marks the profile Not Verified: it is yours, not signed by a company.";

const IOS_TRUST_PATH = `Settings → General → About → Certificate Trust Settings → ${CA_NAME}`;

const NOT_DECRYPTING_IOS = `Installed the profile already? Trust is a separate switch: ${IOS_TRUST_PATH}. Apple's own services never decrypt; a website in Safari is the test.`;

const NOT_DECRYPTING_ANDROID = `Already did? ${CA_NAME} should be under Settings → Security → Encryption & credentials → Trusted credentials → User. Most apps ignore a user certificate on Android 7+; a website in Chrome is the test.`;

const androidInstall = (format: CaFormat) =>
  `Install it: Settings → Security → Encryption & credentials → Install a certificate → CA certificate → ${CA_FILES[format]}.`;

// Each platform gets the one format its installer opens: an iPhone installs a
// configuration profile from Safari, Files and AirDrop alike, where a bare
// certificate installs from none of them; Android's CA installer reads DER and
// lists .cer; every desktop trust command and simctl read PEM.
export function caFormatFor(id: PlatformId): CaFormat {
  if (id === "ios") return "mobileconfig";
  if (id === "android" || id === "android-sim") return "cer";
  return "pem";
}

function downloadStep(
  where: string,
  reach: ReachAddresses,
  format: CaFormat,
  { how = "", scan = false }: { how?: string; scan?: boolean } = {},
): GuideStep {
  return [
    `${where}, open`,
    { copy: reach.caUrls[format], label: "the certificate URL", wire: "certificate url", scan },
    `${how}— it downloads ${CA_FILES[format]}.`,
  ];
}

// Why each task is worth doing, said once: the reason never changes with the
// platform, only the thing it is said about.
const PROVES_IT = "the first decrypted request is what proves setup works";

function tryTask(id: PlatformId, os: HostOs): Task {
  return { title: `Open any website ${openWhere(id, os)}`, why: PROVES_IT };
}

function localSetup(os: HostOs, proxyAddr: string, saved?: SavedCa): PlatformSetup {
  const target = hostTarget(os);
  const trust = OS_TRUST[target](savedFile(target, caFormatFor("this"), saved));
  const host = HOST_NOUN[os];
  return {
    trust: {
      title: "Trust Trawler's certificate",
      why: `so ${host}'s HTTPS can be decrypted`,
      guide: {
        steps: [saveCaStep(caFormatFor("this")), ...trust.install],
        alternatives: trust.alternatives,
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
    notDecrypting: trust.notDecrypting,
  };
}

function anotherMachineSetup(
  id: PlatformId,
  os: HostOs,
  target: OsTarget,
  reach: ReachAddresses | null,
): PlatformSetup {
  const format = caFormatFor(id);
  const trust = OS_TRUST[target](DOWNLOADED[target](CA_FILES[format]));
  const device = deviceName(id, os);
  return {
    trust: {
      title: `On ${device}, install Trawler's certificate`,
      why: `so ${device}'s HTTPS can be decrypted`,
      guide:
        reach === null
          ? undefined
          : {
              steps: [downloadStep(`On ${device}`, reach, format), ...trust.install],
              alternatives: trust.alternatives,
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
    notDecrypting: trust.notDecrypting,
    refusedAfterProof: trust.refusedAfterProof,
  };
}

function deviceSetup(id: "ios" | "android", os: HostOs, reach: ReachAddresses | null): PlatformSetup {
  const ios = id === "ios";
  const device = deviceName(id, os);
  const format = caFormatFor(id);
  const carryOver: GuideStep = ios
    ? [`Or save Trawler's certificate as ${CA_FILES[format]}`, { save: format }, "and AirDrop it over."]
    : ["No browser on it?", ...saveCaStep(format), "and carry the file over instead."];
  const trust: Guide | undefined =
    reach === null
      ? undefined
      : {
        steps: ios
          ? [
              downloadStep(`On ${device}`, reach, format, { how: "in Safari ", scan: PLATFORMS[id].scans }),
              [
                `Install it: Settings → General → VPN & Device Management → ${CA_NAME} → Install.`,
              ],
              [
                `Then turn on full trust. Installing does not: ${IOS_TRUST_PATH}. Until it is on, Safari says This Connection Is Not Private.`,
              ],
            ]
          : [downloadStep(`On ${device}`, reach, format, { scan: PLATFORMS[id].scans }), [androidInstall(format)]],
        notes: ios ? [IOS_UNVERIFIED] : [ANDROID_USER_CA],
        alternatives: [carryOver],
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
    notDecrypting: ios ? NOT_DECRYPTING_IOS : NOT_DECRYPTING_ANDROID,
  };
}

function emulatorSetup(os: HostOs, proxyAddr: string): PlatformSetup {
  const { port } = splitProxyAddr(proxyAddr);
  const format = caFormatFor("android-sim");
  const trust: Guide = {
    steps: [
      saveCaStep(format),
      [
        "Push it to the emulator:",
        { command: `adb push ${CA_FILES[format]} /sdcard/Download/`, label: "the push command" },
      ],
      [androidInstall(format)],
    ],
    notes: [ANDROID_USER_CA],
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
        { copy: EMULATOR_HOST, label: "the emulator's host", wire: "proxy host" },
        "port",
        { copy: String(port), label: "the proxy port", wire: "proxy port" },
      ],
    ],
    notes: [`${EMULATOR_HOST} is the alias every emulator resolves to the machine it runs on.`],
    alternatives: [
      [
        "Or start it with the proxy already set:",
        {
          command: `emulator -avd <name> -http-proxy http://${EMULATOR_HOST}:${port}`,
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
    notDecrypting: NOT_DECRYPTING_ANDROID,
  };
}

// Two stores, two answers. The Simulator follows this Mac's system proxy, so
// the host's own routing guide is its routing guide; its trust store is its
// own, and Apple's supported way in is simctl. `booted` names one Simulator,
// the one booted last, so the command reaches every one running (C2); and
// `keychain reset` leaves the root trusted, so the way back is the erase (C3).
const simulatorTrustCommand = (file: string) =>
  `for d in $(xcrun simctl list devices booted | grep -Eo '[0-9A-F-]{36}'); do xcrun simctl keychain $d add-root-cert ${file}; done`;

function simulatorSetup(os: HostOs, proxyAddr: string, saved?: SavedCa): PlatformSetup {
  const host = HOST_NOUN[os];
  const format = caFormatFor("ios-sim");
  const trust: Guide = {
    steps: [
      saveCaStep(format),
      [
        "With the Simulator running, run:",
        { command: simulatorTrustCommand(savedFile("macos", format, saved)), label: "the trust command" },
      ],
      ["Restart the Simulator so apps pick the new root up."],
    ],
    undoTitle: "to remove it later",
    undo: [
      [
        "Erase that Simulator: Device → Erase All Content and Settings. It wipes everything in it, Trawler's root included.",
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
      return localSetup(ctx.os, ctx.proxyAddr, ctx.saved);
    case "mac":
      return anotherMachineSetup(id, ctx.os, "macos", reach);
    case "windows":
      return anotherMachineSetup(id, ctx.os, "windows", reach);
    case "ubuntu":
      return anotherMachineSetup(id, ctx.os, "linux", reach);
    case "ios-sim":
      return simulatorSetup(ctx.os, ctx.proxyAddr, ctx.saved);
    case "android-sim":
      return emulatorSetup(ctx.os, ctx.proxyAddr);
    case "ios":
    case "android":
      return deviceSetup(id, ctx.os, reach);
  }
}
