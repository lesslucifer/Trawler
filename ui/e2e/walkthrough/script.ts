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
export type ProxyOs =
  "unset" | "unreadable" | { set: { host: string; port: number } };
export type Outcome = "ok" | { fails: { stderr: string } };

export type EmulatorProxyOs = "ours" | "wifi-only" | "foreign" | "none" | "unreadable";

export interface Patch {
  trust?: TrustOs;
  simulators?: { name: string; trust: TrustOs }[];
  emulators?: { name: string; trust: TrustOs; proxy: EmulatorProxyOs }[];
  android?: {
    adb: boolean;
    sdk: string | null;
    avd: { id: string; name: string; playStore?: boolean } | null;
    starting?: boolean;
  };
  install_emulators?: Outcome;
  route_emulators?: Outcome;
  unroute_emulators?: Outcome;
  proxy?: ProxyOs;
  set_up?: Outcome;
  set_proxy?: Outcome;
  unset_proxy?: Outcome;
  remove_ca?: Outcome;
  trust_simulators?: Outcome;
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
  openGuide(task: number): Promise<void>;
  openHelp(task: number): Promise<void>;
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

const OS_NAME: Record<Host, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};
const DEVICE: Record<Host, string> = {
  macos: "this Mac",
  windows: "this PC",
  linux: "this machine",
};

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
  macos:
    "SecTrustSettingsSetTrustSettings: The authorization was denied since no user interaction was possible. (1)",
  windows:
    "CertUtil: -addstore command FAILED: 0x80070005 (WIN32: 5 ERROR_ACCESS_DENIED)",
  linux: "update-ca-certificates: permission denied",
};
const PROXY_ERROR: Record<Host, string> = {
  macos: "** Error: The parameters were not valid.",
  windows: "ERROR: Access is denied.",
  linux: "gsettings: permission denied",
};

const task = (n: number) => `[data-testid="task-${n}"]`;
const button = (prefix: string) =>
  `//button[starts-with(normalize-space(.), "${prefix}")]`;
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
  const setUp = `set up ${device}`;
  const trustRoute = {
    macos: `"open in Keychain Access"`,
    windows: `"open the certificate"`,
    linux: `"save…"`,
  }[host];
  const trustBack =
    host === "macos"
      ? `"remove the certificate", bare, and "do it by hand"`
      : `only "to remove it later"`;
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
      expect: `"${setUp}", the one framed control, above the tasks with "it does tasks 1 and 2"; each task's "do it by hand" collapsed, with no way back inside it; task 3 "Make a request from ${device}" waiting, its request bare`,
      findings: ["W12.E1", "W12.E4"],
      anchor: [button(setUp)],
    },
    {
      id: "cert-done",
      title: "The certificate trusted, the proxy off",
      act: reread({ trust: "trusted", proxy: "unset" }),
      expect: `no "${setUp}" button and no certificate steps under task 2. Task 2 carries "set the system proxy", the one framed control, and "do it by hand"; task 1 keeps ${trustBack}`,
      findings: ["R12", "W12.E1"],
      anchor: [task(2)],
    },
    {
      id: "proxy-done",
      title: "The proxy ours, the certificate not trusted",
      act: reread({ trust: "absent", proxy: OURS }),
      expect: `no combined button; task 1's certificate steps are open, ${trustRoute} the one framed control; no proxy steps are offered as work. Task 2 offers "stop routing through Trawler" with only "do it by hand" under it — no line about a certificate that is not installed`,
      findings: ["R12", "R14", "W12.E3"],
      anchor: [task(2)],
    },
    {
      id: "both-done",
      title: "Both done",
      act: reread({ trust: "trusted", proxy: OURS }),
      expect: `no install route offered as work: task 1 keeps ${trustBack}${host === "macos" ? `, with "the proxy stays set" behind its ?` : ""}; task 2 "stop routing through Trawler", "the certificate stays trusted" behind its ?, and "do it by hand"; task 3's request is the one framed control, its browser and terminal under "other ways"`,
      findings: ["R13", "W12.E2", "W12.E3"],
      anchor: [task(2)],
    },
    ...(host === "macos"
      ? [
          {
            id: "ca-removed",
            title: "The certificate removed",
            act: async (w: Walk) => {
              await w.script({ remove_ca: "ok" });
              await w.click("remove the certificate");
              await w.recheck();
            },
            expect: `task 1 reads not done again, its certificate steps open with ${trustRoute} the one framed control; task 2 stays done, "stop routing through Trawler" with no ? saying "the certificate stays trusted"`,
            findings: [],
            anchor: [task(1)],
          },
        ]
      : []),
    {
      id: "prompt-cancelled",
      title: "The password prompt cancelled",
      act: async (w) => {
        await reread({
          trust: "absent",
          proxy: "unset",
          set_up: { fails: { stderr: CANCELLED[host] } },
        })(w);
        await w.click(setUp);
      },
      expect: `"the password prompt was cancelled. Try again, or do it by hand."; nothing changed and "${setUp}" is still offered`,
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
      expect: `"setting up failed — do it by hand below", and task 1's steps under it are open`,
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
        await w.script({
          unset_proxy: { fails: { stderr: PROXY_ERROR[host] } },
        });
        await w.click("stop routing through Trawler");
      },
      expect: `"couldn't turn the proxy off — do it by hand below", and task 2's "do it by hand" is open`,
      findings: ["R16"],
      anchor: ['[data-testid="unset-proxy-status"]'],
    },
    {
      id: "not-decrypting",
      title: "Arriving but not decrypting, the certificate green",
      act: async (w) => {
        await reread({ trust: "trusted", proxy: "unset", unset_proxy: "ok" })(
          w,
        );
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
      expect: `a desktop gsettings does not answer for: task 2 reads "can't check here — the same request confirms it", not "the system proxy is off", and the request in task 3 is what settles it; no "check again" row, since the CA bundle and GNOME's proxy are re-read on their own`,
      findings: ["R19", "W3.D6", "W3.D8"],
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
    expect: `task 1 reads "not done — no Simulator running", its steps open and "open the Simulator" the one framed control; no "Restart" step; task 2's "set the system proxy" is bare below it`,
    findings: ["W16.W4", "W16.I3", "W16.E2"],
    anchor: [button("open the Simulator")],
  };
  if (host !== "macos")
    return [
      firstPaint(host),
      picker(host),
      {
        ...chosen,
        expect: `nothing to choose — were it offered, it would give xcrun simctl steps${host === "windows" ? ` and a "set the system proxy" button that says the Simulator follows ${DEVICE[host]}'s proxy` : ""}, none of which can apply on ${OS_NAME[host]}`,
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
        await w.click("set the system proxy");
      },
      expect: `"setting the proxy failed — do it by hand below", and task 2's steps are open`,
      findings: ["R16"],
      anchor: ['[data-testid="set-proxy-status"]'],
    },
    {
      id: "proxy-ours",
      title: "The host proxy ours",
      act: async (w) => {
        await w.script({ set_proxy: "ok" });
        await w.click("set the system proxy");
        await w.recheck();
      },
      expect: `task 2 reads done; no "set the system proxy" button; the reversal sits inside task 2 with only "do it by hand" under it`,
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
      expect: `task 2 reads not done again and offers "set the system proxy"`,
      findings: [],
      anchor: [button("set the system proxy")],
    },
    {
      id: "running",
      title: "A Simulator running",
      act: async (w) => {
        await w.script({ simulators: [{ name: "iPhone 17 Pro", trust: "absent" }] });
        await w.recheck();
      },
      expect: `"set up the Simulator", the one framed control, above the tasks; task 1 reads "not done — not in iPhone 17 Pro" with its save and command shut under "do it by hand"; task 2 has no button of its own`,
      findings: ["W16.E1"],
      anchor: [button("set up the Simulator")],
    },
    {
      id: "trusted",
      title: "The Simulator set up",
      act: async (w) => {
        await w.click("set up the Simulator");
        await w.recheck();
      },
      expect: `task 1 reads "done — trusted in iPhone 17 Pro" and task 2 done; task 3's "open it in the Simulator" is the one framed control, the marked address beside it`,
      findings: ["W16.I1", "W16.E3"],
      anchor: [button("open it in the Simulator")],
    },
  ];
}

// The SDK where Android Studio puts it on each host, as its shell writes it.
const ANDROID_SDK: Record<Host, string> = {
  macos: "$HOME/Library/Android/sdk",
  windows: "$env:LOCALAPPDATA\\Android\\Sdk",
  linux: "$HOME/Android/Sdk",
};
const AVD = { id: "Pixel_7_API_35", name: "Pixel 7 API 35" };
const EMULATOR = AVD.name;

function androidSim(host: Host): Chapter[] {
  const SDK = { adb: true, sdk: ANDROID_SDK[host], avd: AVD };
  const chapters: Chapter[] = [
    firstPaint(host),
    picker(host),
    {
      id: "chosen",
      title: "The Android emulator chosen, none running",
      act: async (w) => {
        await w.script({ android: SDK, emulators: [] });
        await w.closePicker();
        await w.choose("Android emulator");
      },
      expect: `"open ${EMULATOR}" the one framed control above the tasks, where "set up the emulator" will stand; task 1 reads "not done — no emulator running" with its save, its push (the SDK's own adb, ${ANDROID_SDK[host]}) and its Settings path shut under "do it by hand", as task 2's steps are; task 3's "open it in the emulator" stands disabled`,
      findings: ["R10", "R26", "R28", "W17.W4", "W17.I4", "W17.N1", "W17.S1"],
      anchor: [button(`open ${EMULATOR}`)],
    },
    {
      id: "starting",
      title: "The emulator starting",
      act: async (w) => {
        await w.script({ android: { ...SDK, starting: true } });
        await w.recheck();
      },
      expect: `"open ${EMULATOR}" disabled in its place; task 1 reads "not done — the emulator is starting", and nothing else changed`,
      findings: ["W17.S2"],
      anchor: [task(1)],
    },
    {
      id: "running",
      title: "An emulator running",
      act: async (w) => {
        await w.script({ android: SDK, emulators: [{ name: EMULATOR, trust: "absent", proxy: "none" }] });
        await w.recheck();
      },
      expect: `"set up the emulator", the one framed control, where "open ${EMULATOR}" stood; task 1 reads "not done — not in ${EMULATOR}", its steps still shut under "do it by hand"; task 3's "open it in the emulator" enabled; nothing else moved`,
      findings: ["W17.E1", "W17.E2", "W17.S1"],
      anchor: [button("set up the emulator")],
    },
    {
      id: "set-up",
      title: "The emulator set up",
      act: async (w) => {
        await w.click("set up the emulator");
        await w.recheck();
      },
      expect: `task 1 reads "done — trusted in ${EMULATOR}" and task 2 "done — ${EMULATOR} routes through Trawler", with "stop routing the emulator" bare inside it; task 3's "open it in the emulator" is the one framed control, the marked address beside it`,
      findings: ["W17.I1", "W17.I2", "W17.E3", "W17.I3", "W17.I6"],
      anchor: [button("open it in the emulator")],
    },
    {
      id: "play-chosen",
      title: "A Google Play image used last, none running",
      act: async (w) => {
        await w.script({ android: { ...SDK, avd: { ...AVD, playStore: true } }, emulators: [] });
        await w.recheck();
      },
      expect: `"open ${EMULATOR}" framed above the tasks, no one act to come; task 1's steps open on "send it to the emulator", disabled, then the path from Encryption & credentials; task 2's "route the emulator" and task 3's "open it in the emulator" disabled`,
      findings: ["W17.S3"],
      anchor: [button(`open ${EMULATOR}`)],
    },
    {
      id: "no-su",
      title: "An emulator whose image has no su",
      act: async (w) => {
        await w.script({ emulators: [{ name: EMULATOR, trust: "unreadable", proxy: "none" }] });
        await w.recheck();
      },
      expect: `the layout before it ran, its buttons enabled and the opener gone: task 1's steps open on "send it to the emulator", the one framed control, then the path from Encryption & credentials, its ? after trawler-ca.cer; task 2 offers "route the emulator", bare`,
      findings: ["W17.I5", "W17.S3", "W17.D1"],
      anchor: [button("send it to the emulator")],
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
const CA_SHOWN: Record<
  "mac" | "windows" | "ubuntu" | "ios" | "android",
  { says: string; findings: string[] }
> = {
  mac: {
    says: "opens /ca.pem, which downloads trawler-ca.pem; then trawler-ca.pem from Downloads into login, Trawler Proxy CA set to Always Trust with its recording, and the password — the System keychain command under other ways",
    findings: ["R27", "W13.I1"],
  },
  windows: {
    says: "opens /ca.cer, which downloads trawler-ca.cer; then trawler-ca.cer from Downloads, Install Certificate → Trusted Root Certification Authorities with its recording, Finish and Yes — the certutil -user command under other ways",
    findings: ["R27", "W14.I1"],
  },
  ubuntu: {
    says: "opens /ca.pem, which downloads trawler-ca.pem; then Firefox's Import… with its recording, choose, tick and OK, then Chrome or Chromium's Trusted Certificates → Import — the Chrome and system commands under other ways, no sudo on the path",
    findings: ["R27", "W15.E1", "W15.E2"],
  },
  ios: {
    says: "opens /ca.mobileconfig in Safari, which downloads trawler-ca.mobileconfig, installs Trawler Proxy CA from VPN & Device Management, and says why iOS calls it Not Verified; its other way saves the same profile to AirDrop over, and no .pem is named",
    findings: ["R23", "R24", "R27", "R28"],
  },
  android: {
    says: "opens /ca.cer, which downloads Trawler's certificate, and the install step reaches the CA installer by Settings' search and picks the trawler-ca file it downloaded; its other way saves trawler-ca.cer",
    findings: ["R25", "R27", "R28"],
  },
};

function offMachine(host: Host, platform: Platform): Chapter[] {
  const allow = "allow devices on";
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
      expect: `task 1 names the one address it will arm, Ethernet · 192.168.1.23, and its button and copy name Ethernet, not Wi-Fi. Tasks 2 onward are dimmed, numbered, and say "after task 1"`,
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
      expect: `"not allowed" alone, under the state line that says Trawler answers on ${DEVICE[host]} only; nothing is armed`,
      findings: ["W14.E3"],
      anchor: ['[data-testid="arm-reach-status"]'],
    },
    {
      id: "armed",
      title: "Allowed",
      act: async (w) => {
        await w.script({ allow_reach: true });
        await w.click(allow);
      },
      expect: `task 1 reads allowed, listening on 192.168.1.23; the tasks below are no longer dimmed, the certificate task's guide is open, and the device's server and port are shown. The certificate task ${ca.says}. No task has a why line; the last is a request from the device, its address at the weight of a step, with no "or"`,
      findings: ["R10", "W13.E1", ...ca.findings],
      anchor: [
        '[data-testid="device-address"]',
        '[data-wire="certificate url"]',
      ],
    },
  ];
  if (platform === "mac" || platform === "windows" || platform === "ubuntu")
    chapters.push({
      id: "download-help",
      title: "What to check when the download fails",
      act: (w) => w.openHelp(2),
      expect: `under the certificate URL, its ? open: "Doesn't load? ${{ mac: "That Mac", windows: "That PC", ubuntu: "That machine" }[platform]} can't reach this one — check both are on the same network, and that ${DEVICE[host]}'s firewall lets Trawler accept incoming connections."`,
      findings: ["W13.R1"],
      anchor: [task(2)],
    });
  if (platform === "windows")
    chapters.push({
      id: "route-open",
      title: "That PC's proxy, by hand",
      act: async (w) => {
        await w.openGuide(3);
        await w.openHelp(3);
      },
      expect: `task 3's four steps: Settings → Network & internet → Proxy on that PC, Manual proxy setup → Use a proxy server → Set up with its recording, Use a proxy server on with the server and port above, Save; under Save's ? "Nothing arrives? On the same page, turn off Automatically detect settings and Use setup script — either one outranks this proxy."; the reg command under other ways`,
      findings: ["W14.I2", "W14.N2"],
      anchor: [task(3)],
    });
  if (platform === "ubuntu")
    chapters.push({
      id: "route-open",
      title: "That machine's proxy, by hand",
      act: async (w) => {
        await w.openGuide(3);
        await w.openHelp(3);
      },
      expect: `task 3's four steps: Settings → Network on that machine, Proxy → Network Proxy → Configuration → Manual with its recording, HTTP and HTTPS Proxy with the server and port above, Save; under Save's ? "Firefox and Chrome follow this proxy. Terminal tools such as curl and apt do not — they read HTTPS_PROXY."; the gsettings command under other ways, and no GNOME anywhere`,
      findings: ["W15.I2", "W15.N2"],
      anchor: [task(3)],
    });
  if (platform === "mac")
    chapters.push(
      {
        id: "route-open",
        title: "That Mac's proxy, by hand",
        act: async (w) => {
          await w.openGuide(3);
          await w.openHelp(3);
        },
        expect: `task 3's four steps: System Settings → Network on that Mac, Wi-Fi → Details… → Proxies with its recording, both proxies on with the server and port above, OK; the Ethernet hint under its ?; the networksetup command under other ways`,
        findings: ["W13.I2", "W13.R1"],
        anchor: [task(3)],
      },
    );
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
  if (!reel || !launch)
    throw new Error(`no walkthrough run ${JSON.stringify(id)}`);
  return { reel, launch };
}
