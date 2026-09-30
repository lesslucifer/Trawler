// The walkthrough (E-ONBREV.W1): every step of every platform the setup screen
// offers, on each host it runs on, as chapters of one reel per host × platform.
//
// A chapter films one moment. `anchor` is what proves that moment painted — it
// holds before and after the epic's fixes alike, so both recordings stop at the
// same place. `expect` is what UC-ONBOARD says the moment should show, named by
// the finding it checks; it is the owner's tick, not the spec's assertion.

export type Host = "macos" | "windows" | "linux";
export const HOSTS: readonly Host[] = ["macos", "windows", "linux"];

export type Platform =
  | "this"
  | "mac"
  | "windows"
  | "ubuntu"
  | "ios-sim"
  | "android-sim"
  | "ios"
  | "android";

export type TrustOs = "trusted" | "installed" | "absent" | "unreadable";
export type ProxyOs = "unset" | "unreadable" | { set: { host: string; port: number } };
export type Outcome = "ok" | { fails: { stderr: string } };

export interface Patch {
  trust?: TrustOs;
  proxy?: ProxyOs;
  set_up?: Outcome;
  set_proxy?: Outcome;
  unset_proxy?: Outcome;
  allow_reach?: boolean;
  allow_replace?: boolean;
  interface_names?: Record<string, string>;
}

export interface Interface {
  name: string;
  address: string;
}

export type FrontMode = "up" | "down" | { slow: number };

// What a chapter can do to the window and the world around it. Implemented by
// the spec; the script only names acts.
export interface Walk {
  script(patch: Patch): Promise<void>;
  recheck(): Promise<void>;
  openPicker(): Promise<void>;
  closePicker(): Promise<void>;
  choose(label: string): Promise<void>;
  click(prefix: string): Promise<void>;
  closeGuide(task: number): Promise<void>;
  dot(): Promise<void>;
  filter(text: string): Promise<void>;
  hit(opts?: { untrusted?: boolean }): Promise<void>;
  front(mode: FrontMode): Promise<void>;
  interfaces(list: Interface[]): Promise<void>;
  pause(ms: number): Promise<void>;
}

export interface Chapter {
  id: string;
  title: string;
  expect: string;
  findings: string[];
  anchor: string[];
  act?: (walk: Walk) => Promise<void>;
  // A chapter about a platform some hosts do not offer: where the picker holds
  // no such option, this chapter and the rest of its reel are skipped for that
  // reason instead of failing on a missing answer.
  offeredAs?: string;
}

export interface Prelaunch {
  front?: FrontMode;
  hits?: number;
  seed?: "imported" | "sent";
  interfaces?: Interface[];
  scenario?: Patch;
}

export interface Launch {
  id: string;
  prelaunch: Prelaunch;
  chapters: Chapter[];
}

export interface Reel {
  id: string;
  host: Host;
  platform: Platform;
  launches: Launch[];
}

const OS_NAME: Record<Host, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };
const DEVICE: Record<Host, string> = { macos: "this Mac", windows: "this PC", linux: "this machine" };

const PLATFORM_LABEL: Record<Exclude<Platform, "this">, string> = {
  mac: "Another Mac",
  windows: "Another Windows PC",
  ubuntu: "Another Ubuntu machine",
  "ios-sim": "iOS Simulator",
  "android-sim": "Android emulator",
  ios: "iPhone or iPad",
  android: "Android device",
};

const thisLabel = (host: Host) => `This machine — ${OS_NAME[host]}`;
const platformLabel = (host: Host, platform: Platform) =>
  platform === "this" ? thisLabel(host) : PLATFORM_LABEL[platform];

const OURS: ProxyOs = { set: { host: "127.0.0.1", port: 7420 } };
const FOREIGN: ProxyOs = { set: { host: "10.0.0.9", port: 3128 } };

const ETHERNET: Interface = { name: "en0", address: "192.168.1.23" };
const WIFI: Interface = { name: "en1", address: "192.168.1.40" };
const PORT_NAMES = { en0: "Ethernet", en1: "Wi-Fi" };

const CANCELLED: Record<Host, string> = {
  macos: "User cancelled. (-128)",
  windows: "The operation was canceled by the user.",
  linux: "User cancelled. (-128)",
};
const INSTALL_ERROR: Record<Host, string> = {
  macos: "SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (1)",
  windows: "CertUtil: -addstore command FAILED: 0x80070005 (WIN32: 5 ERROR_ACCESS_DENIED)",
  linux: "update-ca-certificates: permission denied",
};
const PROXY_ERROR: Record<Host, string> = {
  macos: "** Error: The parameters were not valid.",
  windows: "ERROR: Access is denied.",
  linux: "gsettings: permission denied",
};

const task = (n: number) => `[data-testid="task-${n}"]`;
const button = (prefix: string) => `//button[starts-with(normalize-space(.), "${prefix}")]`;
const DECIDED = ['[data-testid="lead"]', '[data-testid="flow-row"]'];
const LISTBOX = ['[role="listbox"]'];
const BANNER = ['[data-testid="set-up-banner"]'];

function firstPaint(host: Host): Chapter {
  return {
    id: "first-paint",
    title: "First paint",
    expect: `setup, answered "${thisLabel(host)}", with its numbered tasks`,
    findings: [],
    anchor: DECIDED,
  };
}

function picker(host: Host): Chapter {
  return {
    id: "picker",
    title: "The platform question",
    act: (w) => w.openPicker(),
    expect:
      host === "macos"
        ? "every platform, the iOS Simulator among them"
        : `no "iOS Simulator" option — it cannot run beside ${OS_NAME[host]}`,
    findings: host === "macos" ? [] : ["R8"],
    anchor: LISTBOX,
  };
}

function autoThis(host: Host): Chapter[] {
  const device = DEVICE[host];
  const setUp = `Set up ${device}`;
  const reread = (patch: Patch) => async (w: Walk) => {
    await w.script(patch);
    await w.recheck();
  };
  return [
    firstPaint(host),
    picker(host),
    {
      id: "both-undone",
      title: "Nothing done yet",
      act: async (w) => {
        await w.closePicker();
        await reread({ trust: "absent", proxy: "unset" })(w);
      },
      expect: `one "${setUp}" button under the tasks with "one password prompt"; its manual steps collapsed; task 3 waiting for the first request`,
      findings: [],
      anchor: [button(setUp)],
    },
    {
      id: "cert-done",
      title: "The certificate trusted, the proxy off",
      act: reread({ trust: "trusted", proxy: "unset" }),
      expect: `no "${setUp}" button and no certificate steps under task 2. Task 2 carries "Set the system proxy" and its own steps; task 1's steps are collapsed and keep their undo`,
      findings: ["R12"],
      anchor: [task(2)],
    },
    {
      id: "proxy-done",
      title: "The proxy ours, the certificate not trusted",
      act: reread({ trust: "absent", proxy: OURS }),
      expect: `no combined button; task 1's certificate steps are open; no proxy steps are offered as work. The reversal sits inside task 2 and names only the proxy, not a certificate that is not installed`,
      findings: ["R12", "R14"],
      anchor: [task(2)],
    },
    {
      id: "both-done",
      title: "Both done",
      act: reread({ trust: "trusted", proxy: OURS }),
      expect: `no action for either task and no install route offered as work under task 2 — only each task's undo, and "stop routing through Trawler" inside task 2`,
      findings: ["R13"],
      anchor: [task(2)],
    },
    {
      id: "prompt-cancelled",
      title: "The password prompt cancelled",
      act: async (w) => {
        await reread({ trust: "absent", proxy: "unset", set_up: { fails: { stderr: CANCELLED[host] } } })(w);
        await w.click(setUp);
      },
      expect: `"The password prompt was cancelled. Try again, or follow the manual steps."; nothing changed and "${setUp}" is still offered`,
      findings: [],
      anchor: ['[data-testid="set-up-status"]'],
    },
    {
      id: "set-up-failed",
      title: "Setting up fails",
      act: async (w) => {
        await w.script({ set_up: { fails: { stderr: INSTALL_ERROR[host] } } });
        await w.click(setUp);
      },
      expect: `"setting up failed — the steps below do it by hand", and those manual steps are open`,
      findings: ["R16"],
      anchor: ['[data-testid="set-up-status"]'],
    },
    {
      id: "foreign-declined",
      title: "Another proxy set, the replace declined",
      act: async (w) => {
        await reread({ proxy: FOREIGN, set_up: "ok", allow_replace: false })(w);
        await w.click(setUp);
      },
      expect: `task 2 names the other proxy, 10.0.0.9:3128. Declining the replace says "nothing changed — the other proxy is still the one in use, and the certificate was not installed."`,
      findings: [],
      anchor: ['[data-testid="set-up-status"]'],
    },
    {
      id: "set-up-done",
      title: "Setting up succeeds",
      act: async (w) => {
        await w.script({ allow_replace: true });
        await w.click(setUp);
        await w.recheck();
      },
      expect: `both tasks read done; nothing offered but "stop routing through Trawler", inside task 2`,
      findings: ["R13"],
      anchor: [button("stop routing through Trawler")],
    },
    {
      id: "unset-failed",
      title: "Turning the proxy off fails",
      act: async (w) => {
        await w.script({ unset_proxy: { fails: { stderr: PROXY_ERROR[host] } } });
        await w.click("stop routing through Trawler");
      },
      expect: `"couldn't turn the proxy off — the steps below do it by hand", and those steps are open`,
      findings: ["R16"],
      anchor: ['[data-testid="unset-proxy-status"]'],
    },
    {
      id: "not-decrypting",
      title: "Arriving but not decrypting, the certificate green",
      act: async (w) => {
        await reread({ trust: "trusted", proxy: "unset", unset_proxy: "ok" })(w);
        await w.hit({ untrusted: true });
        await w.pause(3_500);
      },
      expect: `task 3 reads "arriving but not decrypting". With task 1 reading done on ${device}, the remedy does not tell the user to finish the certificate step`,
      findings: ["R18"],
      anchor: ['[data-testid="traffic-remedy"]'],
    },
    {
      id: "first-request",
      title: "The first captured request",
      act: (w) => w.hit(),
      expect: `the list, and "You're set up — Trawler is capturing traffic" with "set up another device"`,
      findings: [],
      anchor: BANNER,
    },
    {
      id: "another-device",
      title: "Set up another device",
      act: async (w) => {
        await w.script({ trust: "absent", proxy: "unset" });
        await w.click("set up another device");
      },
      expect: `the dot stays connected; "${thisLabel(host)}" is not preselected — the question reads "choose a device" and no task is drawn; no "You're set up" line before a new captured request`,
      findings: ["R4", "R5", "R6"],
      anchor: ['[data-testid="lead"]'],
    },
    anotherChosen(),
    {
      id: "another-this",
      title: "This machine chosen again, neither half done",
      act: async (w) => {
        await w.choose(thisLabel(host));
        await w.hit();
      },
      expect: `task 3 reads "capturing traffic", and the success line names no device: task 1 and task 2 read not done, so the request cannot be ${device}'s`,
      findings: ["R17"],
      anchor: ['[data-testid="proven"]'],
    },
    {
      id: "filtered-reentry",
      title: "Re-entry with the list filtered",
      act: async (w) => {
        await w.click("‹ show traffic");
        await w.filter("nomatch");
        await w.dot();
        await w.hit();
      },
      expect: `the dot opens setup on "${thisLabel(host)}", the last device chosen. With the list filtered to a query nothing matches, the new captured request still reads in setup: task 3 "capturing traffic"`,
      findings: ["R7"],
      anchor: ['[data-testid="lead"]'],
    },
  ];
}

function linuxThis(): Chapter[] {
  return [
    {
      ...firstPaint("linux"),
      expect: `setup, answered "${thisLabel("linux")}": each task carries its own steps and the first is open`,
    },
    picker("linux"),
    {
      id: "after-gsettings",
      title: "After running the given gsettings command",
      act: async (w) => {
        await w.closePicker();
        await w.script({ trust: "unreadable", proxy: "unreadable" });
        await w.recheck();
      },
      expect: `task 2 reads "can't check here — this machine's first request confirms it", not "the system proxy is off"; no "check again" row, since nothing on this host can be re-read`,
      findings: ["R19", "R20"],
      anchor: [task(2)],
    },
    {
      id: "first-request",
      title: "The first captured request",
      act: (w) => w.hit(),
      expect: `the list, and "You're set up — Trawler is capturing traffic"`,
      findings: [],
      anchor: BANNER,
    },
    {
      id: "another-device",
      title: "Set up another device",
      act: (w) => w.click("set up another device"),
      expect: `the dot stays connected; "${thisLabel("linux")}" is not preselected — the question reads "choose a device" and no task is drawn; no "You're set up" line before a new captured request`,
      findings: ["R4", "R5", "R6"],
      anchor: ['[data-testid="lead"]'],
    },
    anotherChosen(),
  ];
}

// The second device's own session: the question answered, its first guide opens
// although the first device's request is still in the list.
function anotherChosen(): Chapter {
  return {
    id: "another-chosen",
    title: "The second device chosen",
    act: (w) => w.choose("Android emulator"),
    expect: `the Android emulator's tasks, with task 1's steps open; task 3 reads "waiting for the first request" and no "You're set up" line shows before a new captured request`,
    findings: ["R4"],
    anchor: [task(1)],
  };
}

// The Simulator runs beside macOS only, so its proxy chapters are filmed there;
// elsewhere the one moment worth filming is what choosing it offers at all.
function iosSim(host: Host): Chapter[] {
  const chosen: Chapter = {
    id: "chosen",
    title: "The iOS Simulator chosen",
    offeredAs: "iOS Simulator",
    act: async (w) => {
      await w.closePicker();
      await w.choose("iOS Simulator");
    },
    expect: `task 1 carries the Simulator's own certificate steps; task 2 offers "Set the system proxy", which the Simulator follows`,
    findings: [],
    anchor: [button("Set the system proxy")],
  };
  if (host !== "macos")
    return [
      firstPaint(host),
      picker(host),
      {
        ...chosen,
        expect: `nothing to choose — were it offered, it would give xcrun simctl steps${host === "windows" ? ` and a "Set the system proxy" button that says the Simulator follows ${DEVICE[host]}'s proxy` : ""}, none of which can apply on ${OS_NAME[host]}`,
        findings: ["R8"],
        anchor: [task(2)],
      },
    ];
  return [
    firstPaint(host),
    picker(host),
    chosen,
    {
      id: "set-proxy-failed",
      title: "Setting the proxy fails",
      act: async (w) => {
        await w.script({ set_proxy: { fails: { stderr: PROXY_ERROR[host] } } });
        await w.click("Set the system proxy");
      },
      expect: `"setting the proxy failed — the steps below do it by hand", and task 2's steps are open`,
      findings: ["R16"],
      anchor: ['[data-testid="set-proxy-status"]'],
    },
    {
      id: "proxy-ours",
      title: "The host proxy ours",
      act: async (w) => {
        await w.script({ set_proxy: "ok" });
        await w.click("Set the system proxy");
        await w.recheck();
      },
      expect: `task 2 reads done; no "Set the system proxy" button; the reversal sits inside task 2 and says it only turns the proxy off`,
      findings: ["R15"],
      anchor: [button("stop routing through Trawler")],
    },
    {
      id: "proxy-off",
      title: "Routing stopped",
      act: async (w) => {
        await w.click("stop routing through Trawler");
        await w.recheck();
      },
      expect: `task 2 reads not done again and offers "Set the system proxy"`,
      findings: [],
      anchor: [button("Set the system proxy")],
    },
  ];
}

function androidSim(host: Host): Chapter[] {
  const chapters: Chapter[] = [
    firstPaint(host),
    picker(host),
    {
      id: "chosen",
      title: "The Android emulator chosen",
      act: async (w) => {
        await w.closePicker();
        await w.choose("Android emulator");
      },
      expect: `each task carries its own steps and the first guide is open. Nothing observes the emulator, so the guide does not advance by itself (the rule Q3 keeps). The certificate task saves trawler-ca.cer, pushes it with adb and picks trawler-ca.cer in the installer, and names no .pem`,
      findings: ["R10", "R26", "R28"],
      anchor: [task(1)],
    },
  ];
  if (host === "linux")
    chapters.push({
      id: "closed-then-switched",
      title: "A guide closed, then the platform switched",
      act: async (w) => {
        await w.closeGuide(1);
        await w.choose(thisLabel(host));
      },
      expect: `"${thisLabel(host)}" opens the guide its own rule opens, whatever was closed on the emulator`,
      findings: ["R11"],
      anchor: [task(1)],
    });
  return chapters;
}

// What each off-machine certificate task has to name once armed: the one file
// that platform's installer opens, from the URL that downloads it.
const CA_SHOWN: Record<"mac" | "windows" | "ubuntu" | "ios" | "android", { says: string; findings: string[] }> = {
  mac: { says: "opens /ca.pem, which downloads the trawler-ca.pem its commands read", findings: ["R27"] },
  windows: { says: "opens /ca.pem, which downloads the trawler-ca.pem its commands read", findings: ["R27"] },
  ubuntu: { says: "opens /ca.pem, which downloads the trawler-ca.pem its commands read", findings: ["R27"] },
  ios: {
    says: "opens /ca.mobileconfig in Safari, which downloads trawler-ca.mobileconfig, installs Trawler Proxy CA from VPN & Device Management, and says why iOS calls it Not Verified; its other way saves the same profile to AirDrop over, and no .pem is named",
    findings: ["R23", "R24", "R27", "R28"],
  },
  android: {
    says: "opens /ca.cer, which downloads trawler-ca.cer, and the install step picks trawler-ca.cer; its other way saves the same file",
    findings: ["R25", "R27", "R28"],
  },
};

function offMachine(host: Host, platform: Platform): Chapter[] {
  const allow = "Allow devices on";
  const ca = CA_SHOWN[platform as keyof typeof CA_SHOWN];
  const chapters: Chapter[] = [
    firstPaint(host),
    picker(host),
    {
      id: "chosen",
      title: `${PLATFORM_LABEL[platform as Exclude<Platform, "this">]} chosen, one address`,
      act: async (w) => {
        await w.closePicker();
        await w.choose(platformLabel(host, platform));
      },
      expect: `task 1 names the one address it will arm, Ethernet · 192.168.1.23, and its button and copy name Ethernet, not Wi-Fi. Tasks 2 onward are dimmed, numbered, and say "after step 1"`,
      findings: ["R9", "R21"],
      anchor: [task(1)],
    },
    {
      id: "arm-declined",
      title: "Allowing declined",
      act: async (w) => {
        await w.script({ allow_reach: false });
        await w.click(allow);
      },
      expect: `"not allowed — Trawler still answers on this machine only"; nothing is armed`,
      findings: [],
      anchor: ['[data-testid="arm-reach-status"]'],
    },
    {
      id: "armed",
      title: "Allowed",
      act: async (w) => {
        await w.script({ allow_reach: true });
        await w.click(allow);
      },
      expect: `task 1 reads allowed, listening on 192.168.1.23; the tasks below are no longer dimmed, the certificate task's guide is open, and the device's server and port are shown. The certificate task ${ca.says}`,
      findings: ["R10", ...ca.findings],
      anchor: ['[data-testid="device-address"]', '[data-wire="certificate url"]'],
    },
  ];
  if (platform === "ios" || platform === "android")
    chapters.push({
      id: "not-decrypting",
      title: "Arriving but not decrypting",
      act: async (w) => {
        await w.hit({ untrusted: true });
        await w.pause(3_500);
      },
      expect:
        platform === "ios"
          ? `task 2 reads "not working — nothing decrypted" and its steps are open; the remedy says the profile's trust is a separate switch, names Settings → General → About → Certificate Trust Settings → Trawler Proxy CA, and makes a website in Safari the test. Step 3 says installing does not trust`
          : `task 2 reads "not working — nothing decrypted" and its steps are open; the remedy names where Trawler Proxy CA sits in the User store, that most apps ignore a user certificate, and makes a website in Chrome the test`,
      findings: platform === "ios" ? ["R29", "R30"] : ["R30"],
      anchor: ['[data-testid="traffic-remedy"]'],
    });
  if (platform === "ios")
    chapters.push(
      {
        id: "two-addresses",
        title: "Two addresses",
        act: async (w) => {
          await w.interfaces([ETHERNET, WIFI]);
          await w.click("stop listening");
        },
        expect: `"address to use" names one address and "other addresses" holds the other; the button names the chosen interface`,
        findings: ["R21"],
        anchor: ['[data-testid="listen-on"]'],
      },
      {
        id: "no-address",
        title: "No reachable address",
        act: async (w) => {
          await w.click(allow);
          await w.pause(500);
          await w.interfaces([]);
          await w.click("stop listening");
        },
        expect: `task 1 says no address a device could reach and gives a remedy (join the network the device is on) and a way to read again. The tasks below do not wait on a step that cannot be done`,
        findings: ["R22"],
        anchor: [task(1)],
      },
    );
  return chapters;
}

function launchesOfThis(): Launch[] {
  return [
    {
      id: "slow-core",
      prelaunch: { hits: 1, front: { slow: 12_000 } },
      chapters: [
        {
          id: "checking",
          title: "Launch, the core still answering",
          expect: `the shell and the checking dot only — no setup title, question or task paints while the launch read is in flight`,
          findings: ["R1"],
          anchor: ['[data-testid="status-dot"][data-status="checking"]'],
        },
        {
          id: "answered",
          title: "The core answers with a captured row",
          expect: `the list, with no setup paint before it and no "You're set up" banner`,
          findings: ["R1"],
          anchor: ['[data-testid="flow-row"]'],
        },
      ],
    },
    {
      id: "core-down",
      prelaunch: { hits: 1, front: "down" },
      chapters: [
        {
          id: "down",
          title: "Launch, the core not answering",
          act: (w) => w.pause(3_000),
          expect: `"Trawler's capture service isn't answering" — the core is not running — and no setup steps`,
          findings: ["R2"],
          anchor: ["header"],
        },
        {
          id: "back",
          title: "The core comes back, holding old captured rows",
          act: (w) => w.front("up"),
          expect: `the list with the old row, and no "You're set up" banner — that success did not happen in this session`,
          findings: ["R2"],
          anchor: ['[data-testid="flow-row"]'],
        },
      ],
    },
    {
      id: "imported-only",
      prelaunch: { seed: "imported" },
      chapters: [
        {
          id: "imported",
          title: "Launch with only an imported row",
          expect: `setup shows; task 3 reads "waiting for the first request" — an imported row proves nothing`,
          findings: ["R3"],
          anchor: DECIDED,
        },
      ],
    },
    {
      id: "sent-only",
      prelaunch: { seed: "sent" },
      chapters: [
        {
          id: "sent",
          title: "Launch with only a request Trawler sent",
          expect: `setup shows; task 3 reads "waiting for the first request" — a sent row proves nothing`,
          findings: ["R3"],
          anchor: DECIDED,
        },
      ],
    },
  ];
}

const PLATFORMS: readonly Platform[] = [
  "this",
  "mac",
  "windows",
  "ubuntu",
  "ios-sim",
  "android-sim",
  "ios",
  "android",
];

function chaptersFor(host: Host, platform: Platform): Chapter[] {
  switch (platform) {
    case "this":
      return host === "linux" ? linuxThis() : autoThis(host);
    case "ios-sim":
      return iosSim(host);
    case "android-sim":
      return androidSim(host);
    default:
      return offMachine(host, platform);
  }
}

const WALK_SCENARIO: Patch = { interface_names: PORT_NAMES };

export const REELS: Reel[] = HOSTS.flatMap((host) =>
  PLATFORMS.map((platform) => ({
    id: `${host}-${platform}`,
    host,
    platform,
    launches: [
      ...(host === "macos" && platform === "this" ? launchesOfThis() : []),
      {
        id: "walk",
        prelaunch: { interfaces: [ETHERNET], scenario: WALK_SCENARIO },
        chapters: chaptersFor(host, platform),
      },
    ],
  })),
);

export function findRun(id: string): { reel: Reel; launch: Launch } {
  const [reelId, launchId] = id.split("/");
  const reel = REELS.find((r) => r.id === reelId);
  const launch = reel?.launches.find((l) => l.id === launchId);
  if (!reel || !launch) throw new Error(`no walkthrough run ${JSON.stringify(id)}`);
  return { reel, launch };
}
