import { describe, expect, it, test } from "vitest";
import {
  CA_FILES,
  canAutoSetup,
  canProbe,
  caFormatFor,
  checkStep,
  confirmOn,
  deviceName,
  hostOs,
  isCommand,
  isSave,
  isScan,
  lostAddress,
  reachTaskTitle,
  needsReach,
  probesRoute,
  probesOwnStore,
  probesTrust,
  marksItsOwn,
  readsOverAdb,
  routeUndo,
  sharesHostWire,
  PLATFORM_IDS,
  platformChoices,
  setupFor,
  splitProxyAddr,
  type Guide,
  type GuideStep,
  type HostOs,
  type PlatformId,
  type PlatformSetup,
  isMedia,
  isPath,
  isRoute,
} from "./setup";

const UA = {
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  win: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)",
};

const ADDR = "127.0.0.1:9999";
const REACH = {
  proxy: "192.168.1.24:9999",
  caUrls: {
    pem: "http://192.168.1.24:7421/ca.pem",
    cer: "http://192.168.1.24:7421/ca.cer",
    mobileconfig: "http://192.168.1.24:7421/ca.mobileconfig",
  },
};
const ctx = (over: Partial<Parameters<typeof setupFor>[1]> = {}) => ({
  os: "macos" as const,
  proxyAddr: ADDR,
  reach: REACH,
  ...over,
});

const both = (s: PlatformSetup): Guide[] => [s.trust.guide, s.route.guide].filter((g) => g !== undefined);

// Every route a platform offers, whichever part of the guide holds it.
const guides = (s: PlatformSetup): GuideStep[] =>
  both(s).flatMap((g) => [...g.steps, ...(g.alternatives ?? []), ...(g.undo ?? [])]);

// The numbered sequence alone — what the reader is asked to perform, in order.
const numbered = (s: PlatformSetup): GuideStep[] => both(s).flatMap((g) => g.steps);

const notes = (s: PlatformSetup): string[] => both(s).flatMap((g) => g.notes ?? []);

const commands = (steps: GuideStep[]): string[] =>
  steps.flatMap((step) => step.flatMap((p) => (typeof p === "object" && "command" in p ? [p.command] : [])));

const values = (steps: GuideStep[]): string[] =>
  steps.flatMap((step) => step.flatMap((p) => (typeof p === "object" && "copy" in p ? [p.copy] : [])));

const titles = (s: PlatformSetup): string[] =>
  [s.trust, s.route, s.try].flatMap((t) => [t.title, t.why ?? ""]);

const prose = (s: PlatformSetup): string =>
  [...titles(s), s.confirmOn, s.tryCommand ?? "", ...notes(s)]
    .concat(guides(s).flatMap((step) => step.map((p) => (typeof p === "string" ? p : ""))))
    .join("\n");

// Every payload the user takes verbatim, whether they run it or type it in.
const copies = (s: PlatformSetup): string[] => [
  ...commands(guides(s)),
  ...values(guides(s)),
];

const saves = (s: PlatformSetup): number =>
  guides(s).flatMap((step) => step.filter((p) => typeof p === "object" && "save" in p)).length;

const everything = (s: PlatformSetup): string => [prose(s), ...copies(s)].join("\n");

const setup = (id: PlatformId, over = {}) => setupFor(id, ctx(over));

// Every tool any platform's instructions reach for. A string opening with one
// of them is a command whatever part it is written as, which is what lets the
// two questions below be asked in both directions.
const LOOKS_LIKE_COMMAND =
  /^(sudo|security|certutil|adb|networksetup|reg|gsettings|unset|export|curl|emulator|xcrun|for)\s/;

test("hostOs maps webview UA samples to the host family", () => {
  expect(hostOs(UA.mac)).toBe("macos");
  expect(hostOs(UA.win)).toBe("windows");
  expect(hostOs(UA.linux)).toBe("linux");
  expect(hostOs("curl/8")).toBe("other");
});

test("only macOS and Windows have a one-click setup path", () => {
  expect(canAutoSetup("macos")).toBe(true);
  expect(canAutoSetup("windows")).toBe(true);
  expect(canAutoSetup("linux")).toBe(false);
  expect(canAutoSetup("other")).toBe(false);
});

test("macOS, Windows and Linux have a reader for their own trust store and proxy (D8)", () => {
  expect(canProbe("macos")).toBe(true);
  expect(canProbe("windows")).toBe(true);
  expect(canProbe("linux")).toBe(true);
  expect(canProbe("other")).toBe(false);
});

test("splitProxyAddr splits host and port on the last colon", () => {
  expect(splitProxyAddr("127.0.0.1:7420")).toEqual({ host: "127.0.0.1", port: 7420 });
  expect(splitProxyAddr("192.168.1.5:8080")).toEqual({ host: "192.168.1.5", port: 8080 });
});

// DEC-63: the address another device opens so its request is known to be its.
describe("another device's own request is one address, opened where it is", () => {
  const ADDRESS = "https://example.com/?trawler-check=k3m9";
  const OFF = ["mac", "windows", "ubuntu", "ios", "android"] as const;

  it.each(OFF)("%s: the address is a value, said where it is opened", (id) => {
    const [said, value] = checkStep(id, "macos", ADDRESS);

    expect(said).toMatch(/^or open this one (on|in) /);
    expect(value).toMatchObject({ copy: ADDRESS, label: "the address to open" });
  });

  // W9's S3: once proved, opening it again is the re-check.
  it("once proved, it is the address to check again", () => {
    const [said] = checkStep("ios", "macos", ADDRESS, true);

    expect(said).toBe("to check again, open this one on your iPhone");
  });

  it("a phone scans it, and a computer does not (DEC-57)", () => {
    const scans = OFF.filter((id) => checkStep(id, "macos", ADDRESS).some(isScan));

    expect(scans).toEqual(["ios", "android"]);
  });

  it("a platform scans its certificate URL exactly where it scans this address", () => {
    const scansCertificate = (id: PlatformId) => guides(setup(id)).some((step) => step.some(isScan));

    expect(OFF.filter(scansCertificate)).toEqual(OFF.filter((id) => checkStep(id, "macos", ADDRESS).some(isScan)));
  });
});

// R1: the core lists the addresses this machine holds on every read, and keeps
// the one it armed whether or not it still does.
describe("an armed address this machine no longer holds is lost", () => {
  const reach = (proxy: string | null, addresses: string[]) => ({
    armed: proxy !== null,
    proxy,
    api: null,
    ca_urls: null,
    interfaces: addresses.map((address) => ({ name: "Wi-Fi", address })),
  });

  it("is the armed host, once the list no longer holds it", () => {
    expect(lostAddress(reach("192.168.1.24:7420", ["10.0.0.7"]))).toBe("192.168.1.24");
    expect(lostAddress(reach("192.168.1.24:7420", []))).toBe("192.168.1.24");
  });

  it("is nothing while the list holds it, or while nothing is armed", () => {
    expect(lostAddress(reach("192.168.1.24:7420", ["10.0.0.7", "192.168.1.24"]))).toBeNull();
    expect(lostAddress(reach(null, []))).toBeNull();
  });

  it("matches an IPv6 address the way the list writes it, without brackets", () => {
    expect(lostAddress(reach("[2001:db8::7]:7420", ["2001:db8::7"]))).toBeNull();
    expect(lostAddress(reach("[2001:db8::7]:7420", ["2001:db8::8"]))).toBe("2001:db8::7");
  });
});

// The screen's one axis. The order is what the user reads in the picker, and
// this machine leads it because it is the only member the app can act on itself.
test("the axis offers the eight platforms, this machine first", () => {
  expect(PLATFORM_IDS).toEqual([
    "this",
    "mac",
    "windows",
    "ubuntu",
    "ios-sim",
    "android-sim",
    "ios",
    "android",
  ]);
});

describe("a host is offered only the platforms that run beside it", () => {
  const offered = (os: HostOs) => platformChoices(os).map((p) => p.id);

  test("macOS offers all eight, the iOS Simulator among them", () => {
    expect(offered("macos")).toEqual([...PLATFORM_IDS]);
  });

  test.each<HostOs>(["windows", "linux", "other"])("%s offers no iOS Simulator", (os) => {
    expect(offered(os)).toEqual(PLATFORM_IDS.filter((id) => id !== "ios-sim"));
  });

  test.each<HostOs>(["macos", "windows", "linux", "other"])(
    "%s offers the Android emulator",
    (os) => {
      expect(offered(os)).toContain("android-sim");
    },
  );
});

test("this machine names the detected host, and names nothing it did not detect", () => {
  expect(platformChoices("macos")[0]).toEqual({ id: "this", label: "This machine — macOS" });
  expect(platformChoices("windows")[0].label).toBe("This machine — Windows");
  expect(platformChoices("linux")[0].label).toBe("This machine — Linux");
  expect(platformChoices("other")[0].label).toBe("This machine");
});

test("every other platform is named for what the user is holding, not for an OS", () => {
  const labels = Object.fromEntries(platformChoices("macos").map((p) => [p.id, p.label]));
  expect(labels).toMatchObject({
    mac: "Another Mac",
    windows: "Another Windows PC",
    ubuntu: "Another Ubuntu machine",
    "ios-sim": "iOS Simulator",
    "android-sim": "Android emulator",
    ios: "iPhone or iPad",
    android: "Android device",
  });
});

// DEC-33's layer rule applied to the axis: a probe reads this machine, so it may
// only answer for a platform that reads the same store. The two probes read two
// stores, and the Simulator shares exactly one of them.
test("this machine's certificate store answers for this machine alone", () => {
  expect(PLATFORM_IDS.filter(probesTrust)).toEqual(["this"]);
});

test("this machine's system proxy answers for it and for the Simulator that follows it", () => {
  expect(PLATFORM_IDS.filter(probesRoute)).toEqual(["this", "ios-sim"]);
});

// W7: the Simulator keeps a store this machine can read, and sends through this
// machine's own proxy, so this machine's traffic is always on its wire.
test("the Simulator alone has a store of its own read here, and shares this machine's wire", () => {
  expect(PLATFORM_IDS.filter(probesOwnStore)).toEqual(["ios-sim"]);
  expect(PLATFORM_IDS.filter(sharesHostWire)).toEqual(["ios-sim"]);
});

test("the five platforms that are somewhere else are the ones that need reach", () => {
  expect(PLATFORM_IDS.filter(needsReach)).toEqual(["mac", "windows", "ubuntu", "ios", "android"]);
});

// A platform whose instructions need an address keeps its tasks and loses its
// guides: the shape of the job is readable before any of it can be done, and
// nothing is invented to fill the gap.
describe("a platform that needs reach has no instructions until it exists", () => {
  it.each(PLATFORM_IDS.filter(needsReach))("%s offers no steps while reach is unarmed", (id) => {
    const s = setupFor(id, ctx({ reach: null }));

    expect(s.trust.guide).toBeUndefined();
    expect(s.route.guide).toBeUndefined();
    expect(s.trust.title).not.toBe("");
    expect(s.try.title).not.toBe("");
  });

  it.each(PLATFORM_IDS.filter((id) => !needsReach(id)))("%s answers without reach", (id) => {
    expect(setupFor(id, ctx({ reach: null })).trust.guide).toBeDefined();
  });
});

describe("every platform's instructions are its own", () => {
  it.each(PLATFORM_IDS.filter((id) => id !== "this"))("%s says where to open a website, and why it settles setup", (id) => {
    expect(setup(id).try.title).toMatch(/^Open any website /);
    expect(setup(id).try.why).toBe("the first decrypted request is what proves setup works");
  });

  it("this names what its request does, and keeps only the why its lead does not say (W12's Q2, Q3)", () => {
    for (const os of ["macos", "windows", "linux"] as const) {
      const s = setup("this", { os });
      expect(s.try.title).toBe(`Make a request from ${deviceName("this", os)}`);
      expect(s.try.why).toBeUndefined();
      expect(s.trust.why).toBeUndefined();
      expect(s.route.why).toBe("its system proxy points at Trawler while Trawler runs");
    }
  });

  it("only this machine offers a command to run here", () => {
    expect(setup("this").tryCommand).toBe("curl -x http://127.0.0.1:9999 https://example.com");
    for (const id of PLATFORM_IDS.filter((p) => p !== "this")) {
      expect(setup(id).tryCommand).toBeUndefined();
    }
  });

  it.each(PLATFORM_IDS)("%s names no address it was not given", (id) => {
    const text = everything(setup(id));
    expect(text).not.toMatch(/7420/);
    const withoutCa = Object.values(REACH.caUrls).reduce((t, url) => t.split(url).join(""), text);
    expect(withoutCa).not.toMatch(/7421/);
  });
});

// The blocked-on row the run handed the build: the emulator's host note and the
// device's blocker used to sit in one array, so a user setting up either read
// the other's answer.
describe("the Android emulator and the Android device stop sharing one answer", () => {
  const emulator = () => setup("android-sim");
  const device = () => setup("android");

  it("differ in whether reach is needed at all", () => {
    expect(needsReach("android-sim")).toBe(false);
    expect(needsReach("android")).toBe(true);
  });

  it("send the certificate by different routes", () => {
    expect(commands(emulator().trust.guide!.steps)).toEqual([
      "for d in $(adb devices | grep -Eo '^emulator-[0-9]+'); do adb -s $d push ~/Downloads/trawler-ca.cer /sdcard/Download/; done",
    ]);
    expect(copies(device())).toContain(REACH.caUrls.cer);
    expect(everything(device())).not.toMatch(/\badb\b/);
  });

  it("point at different hosts — the emulator's alias, the device's reach address", () => {
    expect(copies(emulator())).toContain("10.0.2.2");
    expect(everything(emulator())).not.toContain(REACH.proxy);
    expect(everything(device())).toContain("192.168.1.24");
    expect(everything(device())).not.toContain("10.0.2.2");
  });

  it("carry the real port into the emulator's host, never a hardcoded one", () => {
    const text = everything(setupFor("android-sim", ctx({ proxyAddr: "127.0.0.1:8123" }))!);
    expect(text).toContain("10.0.2.2:8123");
    expect(text).not.toMatch(/9999|7420/);
  });

  it("both state the Android 7+ user-CA caveat, because it is true of both", () => {
    expect(prose(emulator())).toMatch(/network security config/);
    expect(prose(device())).toMatch(/network security config/);
  });
});

// The punt the old android line shipped: "Host = your machine's LAN address".
// An instruction that asks the user to know an address Trawler could have
// printed is the failure this whole epic exists to end.
test("no instruction punts an address in prose", () => {
  for (const id of PLATFORM_IDS) {
    const text = everything(setup(id));
    expect(text).not.toMatch(/LAN address/i);
    expect(text).not.toMatch(/your machine's address/i);
    expect(text).not.toMatch(/see setup notes/i);
  }
});

describe("an off-machine platform is pointed at the address reach reported", () => {
  it.each(PLATFORM_IDS.filter(needsReach))("%s fetches the certificate over it", (id) => {
    expect(copies(setup(id))).toContain(REACH.caUrls[caFormatFor(id)]);
  });

  it.each(["mac", "windows", "ubuntu"] as const)("%s routes through it, never loopback", (id) => {
    const text = everything(setup(id));
    expect(text).toContain("192.168.1.24");
    expect(text).not.toContain("127.0.0.1");
  });

  it("the devices take the server and port from the zone above rather than restating them", () => {
    for (const id of ["ios", "android"] as const) {
      expect(prose(setup(id))).toMatch(/server and port above/);
    }
  });
});

describe("this machine is instructed for the host it is running on", () => {
  it("macOS gets the keychain route and the networksetup command", () => {
    const text = everything(setup("this", { os: "macos" }));
    expect(text).toMatch(/security add-trusted-cert/);
    expect(text).toMatch(/networksetup -setwebproxy Wi-Fi 127\.0\.0\.1 9999/);
  });

  it("Windows gets certutil and the registry command", () => {
    const text = everything(setup("this", { os: "windows" }));
    expect(text).toMatch(/certutil -addstore/);
    expect(text).toMatch(/ProxyServer .*127\.0\.0\.1:9999/);
  });

  // W7's C1: a terminal opens in the home folder, so a bare file name finds a
  // file only the user who saved it there has.
  it("its trust command reads Downloads, where the save dialog opens, then the path a save returned, quoted for its shell", () => {
    const before = {
      macos: "~/Downloads/trawler-ca.pem",
      windows: '"$HOME\\Downloads\\trawler-ca.pem"',
      linux: '"$(xdg-user-dir DOWNLOAD)/trawler-ca.pem"',
    };
    const saved = {
      macos: "/Users/me/My Certs/it's trawler-ca.pem",
      windows: "C:\\Users\\me\\it's\\trawler-ca.pem",
      linux: "/home/me/certs/trawler-ca.pem",
    };
    const quoted = {
      macos: "'/Users/me/My Certs/it'\\''s trawler-ca.pem'",
      windows: "'C:\\Users\\me\\it''s\\trawler-ca.pem'",
      linux: "'/home/me/certs/trawler-ca.pem'",
    };
    for (const os of ["macos", "windows", "linux"] as const) {
      const trust = (over = {}) => {
        const guide = setup("this", { os, ...over }).trust.guide!;
        return commands([...guide.steps, ...(guide.alternatives ?? [])]).join("\n");
      };
      expect(trust()).toContain(` ${before[os]}`);
      expect(trust()).not.toMatch(/ trawler-ca\.pem( |$)/);
      expect(trust({ saved: { pem: saved[os] } })).toContain(` ${quoted[os]}`);
      expect(trust({ saved: { cer: saved[os] } })).toContain(` ${before[os]}`);
    }
  });

  it("an unknown host gets the two routes that assume no desktop", () => {
    const text = everything(setup("this", { os: "other" }));
    expect(text).toMatch(/update-ca-certificates/);
    expect(text).toMatch(/export HTTPS_PROXY=http:\/\/127\.0\.0\.1:9999/);
  });

  it("every route offers its reversal", () => {
    for (const os of ["macos", "windows", "linux", "other"] as const) {
      const s = setup("this", { os });
      expect(s.trust.guide?.undo?.length).toBeGreaterThan(0);
      expect(s.route.guide?.undo?.length).toBeGreaterThan(0);
    }
  });
});

// The Simulator keeps its own trust store and follows this Mac's system proxy.
// Saying the host's certificate store covers it is the one sentence on this
// screen that sent a user to a step that cannot work.
describe("the Simulator is trusted through its own keychain and routed through this Mac's", () => {
  it("says the store is its own, and gives Apple's route into it", () => {
    const s = setup("ios-sim");
    expect(s.trust.why).toMatch(/keeps its own certificate store/);
    expect(everything(s)).not.toMatch(/this Mac's certificate store/);
    expect(commands(s.trust.guide!.steps)).toEqual([
      "for d in $(xcrun simctl list devices booted | grep -Eo '[0-9A-F-]{36}'); do xcrun simctl keychain $d add-root-cert ~/Downloads/trawler-ca.pem; done",
    ]);
    expect(saves(s)).toBe(1);
  });

  // W7's C2: with two running, `booted` named the one booted last, silently.
  it("trusts every Simulator running, never the one `booted` picks", () => {
    const [trust] = commands(setup("ios-sim").trust.guide!.steps);
    expect(trust).toMatch(/^for d in \$\(xcrun simctl list devices booted \| .+\); do xcrun simctl keychain \$d add-root-cert .+; done$/);
    expect(everything(setup("ios-sim"))).not.toMatch(/simctl keychain booted/);
  });

  it("reads the file where the save put it", () => {
    const s = setup("ios-sim", { saved: { pem: "/Users/me/Desktop/trawler-ca.pem" } });
    expect(commands(s.trust.guide!.steps)[0]).toContain("add-root-cert '/Users/me/Desktop/trawler-ca.pem'; done");
  });

  // W7's C3: `keychain reset` exits 0 and leaves the root trusted.
  it("reverses that trust by the erase, and says what the erase costs", () => {
    const s = setup("ios-sim");
    expect(s.trust.guide!.undo).toEqual([
      ["Erase that Simulator: Device → Erase All Content and Settings. It wipes everything in it, Trawler's root included."],
    ]);
    expect(everything(s)).not.toMatch(/keychain .*reset/);
  });

  it("routes through the host's own guide, because the proxy it follows is the host's", () => {
    const s = setup("ios-sim");
    expect(s.route.why).toMatch(/this Mac's system proxy/);
    expect(copies(s)).toContain(copies(setup("this")).find((c) => c.startsWith("networksetup")));
  });

  it("carries no trust step of this machine's, whose store it does not read", () => {
    expect(everything(setup("ios-sim"))).not.toMatch(/security add-trusted-cert/);
  });
});

// A copyable value that appears twice renders two controls for one fact; one
// that appears zero times is prose the user has to retype.
describe("every copyable value is offered exactly once in the guide that introduces it", () => {
  it.each(PLATFORM_IDS)("%s", (id) => {
    const taken = copies(setup(id));
    expect(new Set(taken).size).toBe(taken.length);
  });

  it.each(PLATFORM_IDS)("%s writes no command as prose", (id) => {
    for (const step of guides(setup(id))) {
      for (const part of step) {
        if (typeof part === "string") expect(LOOKS_LIKE_COMMAND.test(part)).toBe(false);
      }
    }
  });
});

// A numbered step is something the reader performs. A caveat is not, and a
// second route to the same end is the same job twice — so each is its own kind
// of thing, and the count under a task's summary counts only the first.
describe("a guide's steps are the route, and everything else says what it is", () => {
  it.each(PLATFORM_IDS)("%s carries every command as a command, never as a value", (id) => {
    for (const value of values(guides(setup(id)))) {
      expect(LOOKS_LIKE_COMMAND.test(value)).toBe(false);
    }
  });

  it.each(PLATFORM_IDS)("%s writes every value it offers as a value, never as a command", (id) => {
    for (const command of commands(guides(setup(id)))) {
      expect(LOOKS_LIKE_COMMAND.test(command)).toBe(true);
    }
  });

  it.each(PLATFORM_IDS)("%s's notes are nothing to do, so none of them is a step", (id) => {
    const s = setup(id);
    const said = numbered(s).flatMap((step) => step.filter((p) => typeof p === "string"));

    for (const note of notes(s)) expect(said).not.toContain(note);
  });

  it("the browser caveats are notes — a reader cannot perform 'apps ignore user CAs'", () => {
    expect(notes(setup("android"))).toContain(
      "Apps targeting Android 7+ only trust user CAs when their network security config opts in.",
    );
    expect(notes(setup("android-sim")).join("\n")).toMatch(/10\.0\.2\.2 is the alias/);
  });

  it("a second way in is an alternative, not a step the reader also takes", () => {
    const alternatives = (id: PlatformId) =>
      [setup(id).trust.guide, setup(id).route.guide]
        .filter((g) => g !== undefined)
        .flatMap((g) => g.alternatives ?? []);

    expect(commands(alternatives("android-sim"))).toContain(
      "emulator -avd <name> -http-proxy http://127.0.0.1:9999",
    );
    expect(alternatives("ubuntu")).toEqual([]);
    expect(
      alternatives("android").flatMap((step) => step.filter((p) => typeof p === "string")),
    ).toContain("No browser on it?");
    expect(
      alternatives("ios").flatMap((step) => step.filter((p) => typeof p === "string")),
    ).toContain("and AirDrop it over.");
    expect(saves(setup("ios"))).toBe(1);
    expect(saves(setup("android"))).toBe(1);
  });

  it("what is left in the sequence is short enough to read before starting", () => {
    expect(setup("this").trust.guide?.steps).toHaveLength(3);
    expect(setup("this").route.guide?.steps).toHaveLength(4);
    expect(setup("this", { os: "windows" }).trust.guide?.steps).toHaveLength(3);
    expect(setup("this", { os: "windows" }).route.guide?.steps).toHaveLength(4);
    expect(setup("ios").trust.guide?.steps).toHaveLength(3);
    expect(setup("android-sim").trust.guide?.steps).toHaveLength(3);
    expect(setup("android-sim").route.guide?.steps).toHaveLength(1);
  });
});

// The save control produces the file the next step names, so it belongs only
// where a file is the route — never where the certificate is fetched over the
// network instead.
test("the save control appears where the file is carried, and once", () => {
  expect(saves(setup("this"))).toBe(1);
  expect(saves(setup("android-sim"))).toBe(1);
  expect(saves(setup("ios"))).toBe(1);
  expect(saves(setup("android"))).toBe(1);
  expect(saves(setup("ios-sim"))).toBe(1);
  for (const id of ["mac", "windows", "ubuntu"] as const) {
    expect(saves(setup(id))).toBe(0);
  }
});


// F2 / F3 / F4. Every line below the tiles names the thing the user picked, in
// the words they picked it with — and a task nobody can read the state of says
// when it WILL be settled rather than what the app cannot do.
describe("the words each platform is spoken about in", () => {
  const NAMES: [PlatformId, string][] = [
    ["mac", "that Mac"],
    ["windows", "that PC"],
    ["ubuntu", "that machine"],
    ["ios-sim", "the Simulator"],
    ["android-sim", "the emulator"],
    ["ios", "your iPhone"],
    ["android", "your Android device"],
  ];

  it.each(NAMES)("%s is called %s", (id, device) => {
    expect(deviceName(id, "macos")).toBe(device);
  });

  it("this machine is called what its host calls it", () => {
    expect(deviceName("this", "macos")).toBe("this Mac");
    expect(deviceName("this", "windows")).toBe("this PC");
    expect(deviceName("this", "linux")).toBe("this machine");
    expect(deviceName("this", "other")).toBe("this machine");
  });

  it.each(PLATFORM_IDS)("%s promises when an unreadable state will be settled", (id) => {
    expect(confirmOn(id, "macos")).toBe(
      `${deviceName(id, "macos")}'s first request confirms it`,
    );
  });

  it.each(PLATFORM_IDS.filter(needsReach))("%s's network task is titled for the device", (id) => {
    expect(reachTaskTitle(id, "macos")).toBe(`Allow ${deviceName(id, "macos")} to connect`);
  });

  it("a website is opened ON a machine and IN a simulator", () => {
    expect(setup("this").try.title).toBe("Make a request from this Mac");
    expect(setup("ios").try.title).toBe("Open any website on your iPhone");
    expect(setup("ios-sim").try.title).toBe("Open any website in the Simulator");
    expect(setup("android-sim").try.title).toBe("Open any website in the emulator");
  });

  it.each(PLATFORM_IDS)("%s titles its tasks by what the user does, never by a zone", (id) => {
    const s = setup(id);

    for (const title of [s.trust.title, s.route.title, s.try.title]) {
      expect(title).toMatch(/^[A-Z]/);
      expect(title).not.toMatch(/what has to be true|precondition|target|platform/i);
    }
  });

  // The list is where "flow" is taught; this screen is read before the word
  // means anything, and the same holds for the machine's other words for
  // itself. The screen asserts the rendering; this asserts the source.
  it.each(PLATFORM_IDS)("%s's own strings carry none of the machine's words", (id) => {
    const text = [everything(setup(id)), confirmOn(id, "macos"), reachTaskTitle(id, "macos")].join(
      "\n",
    );

    for (const word of [/\bflows?\b/i, /\bverdict\b/i, /\breach\b/i, /\bplatform\b/i, /listen on/i]) {
      expect(text).not.toMatch(word);
    }
  });
});

// The owner's iPhone was sent a PEM under a step that promised a profile, and
// told to carry a .pem over when iOS installs none. Each platform's certificate
// task names one file, the one its URL or save control produces, and every
// command in it reads that name.
describe("each platform names the one certificate file its installer opens", () => {
  const HOSTS = ["macos", "windows", "linux", "other"] as const;
  const FILES = Object.values(CA_FILES);
  const cases = PLATFORM_IDS.flatMap((id) =>
    HOSTS.flatMap((os) => [REACH, null].map((reach) => [id, os, reach] as const)),
  );

  it.each(cases)("%s on %s, reach %o", (id, os, reach) => {
    const trust = setupFor(id, ctx({ os, reach })).trust.guide;
    if (trust === undefined) return;
    const format = caFormatFor(id);
    const steps = [...trust.steps, ...(trust.alternatives ?? [])];
    const said = steps
      .flatMap((step) => step.map((p) => (typeof p === "string" ? p : isCommand(p) ? p.command : "")))
      .concat(trust.notes ?? [])
      .join("\n");

    expect(FILES.filter((file) => said.includes(file))).toEqual([CA_FILES[format]]);
    for (const part of steps.flat()) {
      if (isSave(part)) expect(part.save).toBe(format);
      if (typeof part === "object" && "wire" in part && part.wire === "certificate url") {
        expect(part.copy).toBe(REACH.caUrls[format]);
      }
    }
    for (const command of commands(steps).filter((c) => /trawler-ca\./.test(c))) {
      expect(command).toContain(CA_FILES[format]);
    }
  });

  it("gives an iPhone the profile, Android DER, and every other platform PEM", () => {
    expect(caFormatFor("ios")).toBe("mobileconfig");
    expect(caFormatFor("android")).toBe("cer");
    expect(caFormatFor("android-sim")).toBe("cer");
    for (const id of ["this", "mac", "windows", "ubuntu", "ios-sim"] as const) {
      expect(caFormatFor(id)).toBe("pem");
    }
  });

  it("sends an iPhone to Safari for the profile, says why iOS calls it Not Verified, and never names a .pem", () => {
    const text = everything(setup("ios"));
    expect(text).toMatch(/in Safari/);
    expect(text).toMatch(/Not Verified/);
    expect(text).not.toMatch(/\.pem\b/);
  });

  it("names the file to pick in each phone installer", () => {
    expect(prose(setup("ios"))).toMatch(/VPN & Device Management → Trawler Proxy CA → Install/);
    expect(prose(setup("android"))).toMatch(/CA certificate → Install anyway → the trawler-ca file it downloaded\./);
    expect(prose(setup("android-sim"))).toMatch(/CA certificate → Install anyway → ☰ → Downloads → trawler-ca\.cer\./);
  });
});

// The owner's second iPhone report: the profile installed, trust off, and
// nothing on the screen said installing is not trusting. Each platform names
// what its users check next when traffic arrives and does not decrypt.
describe("each platform says what to check when its traffic does not decrypt", () => {
  it.each(PLATFORM_IDS.filter((id) => !sharesHostWire(id) && !readsOverAdb(id) && id !== "ios" && id !== "android"))(
    "%s has a check to make",
    (id) => {
      expect(setup(id).notDecrypting).toMatch(/\S/);
    },
  );

  // W7's S2: the Simulator's wire is this Mac's too, so a refusal is never its
  // certificate task's to explain.
  it("the Simulator has none: a refusal is never its certificate task's", () => {
    expect(setup("ios-sim").notDecrypting).toBeUndefined();
  });

  // W9's Q1: Apple's own services refuse whatever its certificate does, so its
  // address is its test, and only that refusal is its certificate's to say.
  it("the iPhone has none: its test is the address it is offered", () => {
    expect(setup("ios").notDecrypting).toBeUndefined();
  });

  // W10's Q1: Android's own services ignore a user certificate, so the Android
  // device's test is its address too.
  it("the Android device has none: its test is the address it is offered, as the iPhone's is", () => {
    expect(setup("android").notDecrypting).toBeUndefined();
    expect(PLATFORM_IDS.filter((id) => setup(id).addressTest !== undefined)).toEqual(["ios", "android"]);
  });

  // W10's S2: a user certificate has no second switch, and the wrong type is
  // refused outright, so the address refused says the user store lacks it.
  it("the Android device's address refused says its user store lacks the certificate, and how to see it there", () => {
    const { refused, maybeRefused, elsewhereRefused } = setup("android").addressTest!;

    expect(refused).toBe(
      "example.com did not decrypt on your Android device — Trawler Proxy CA is not in its user store. The steps below install it. Installed it already? Settings → search Trusted credentials → User lists it; if it does, open the address in Chrome.",
    );
    expect(maybeRefused).toBe(
      "example.com did not decrypt — if you opened it on your Android device, Trawler Proxy CA is not in its user store: the certificate step installs it.",
    );
    expect(elsewhereRefused).toBe(
      "something is connecting and none of it is decrypting — Android's own services ignore a user certificate, so they never decrypt. The address below is the test.",
    );
  });

  // W9's S2: the profile and the switch refuse alike, and the switch leads.
  it("the iPhone's address refused names the trust switch before the profile", () => {
    const { refused, maybeRefused } = setup("ios").addressTest!;
    const SWITCH = "turn on full trust: Settings → General → About → Certificate Trust Settings → Trawler Proxy CA";

    expect(refused).toBe(
      `example.com did not decrypt on your iPhone — ${SWITCH}. Not listed there? The profile is not installed — the steps below install it.`,
    );
    expect(maybeRefused).toBe(
      `example.com did not decrypt — if you opened it on your iPhone, ${SWITCH}. Not listed there? The profile is not installed — the certificate step installs it.`,
    );
  });

  it("the iPhone's other refusals are Apple's own services, and the address is the test", () => {
    expect(setup("ios").addressTest!.elsewhereRefused).toBe(
      "something is connecting and none of it is decrypting — Apple's own services pin their certificates, so they never decrypt. The address below is the test.",
    );
  });

  // W8's S2: the emulator's own services refuse a certificate it holds, so a
  // refusal is never its certificate task's either.
  it("the emulator has none: a refusal is never its certificate task's", () => {
    expect(setup("android-sim").notDecrypting).toBeUndefined();
  });

  // S2: Firefox reads the system's store on macOS and Windows, and keeps its
  // own on Linux.
  it.each([
    ["this", "macos"],
    ["this", "windows"],
    ["mac", "macos"],
    ["windows", "macos"],
  ] as const)("%s on %s: a browser restart and pinning, and no Firefox store of its own", (id, os) => {
    const check = setup(id, { os }).notDecrypting;
    expect(check).toMatch(/reads the system's store when it starts — restart it/);
    expect(check).toMatch(/pin their own certificate/);
    expect(check).not.toMatch(/Firefox/);
    expect(notes(setup(id, { os })).join("\n")).not.toMatch(/Firefox/);
  });

  // S1: on Linux the system command reaches no browser, so a restart fixes
  // nothing and the advice names the stores the browsers keep instead.
  it.each([
    ["this", "linux"],
    ["this", "other"],
    ["ubuntu", "macos"],
  ] as const)("%s on %s: the system command reaches no browser, each has its own step", (id, os) => {
    const check = setup(id, { os }).notDecrypting;
    expect(check).toMatch(/The system command covers curl and the system's tools, not browsers/);
    expect(check).toMatch(/Firefox and Chrome each keep a store of their own, and each has its own step below/);
    expect(check).toMatch(/pin their own certificate/);
    expect(check).not.toMatch(/restart/);
    expect(notes(setup(id, { os }))).toEqual([]);
  });

  it("the iPhone's trust step says installing does not trust, and what Safari says until it does", () => {
    const steps = setup("ios").trust.guide!.steps.map((step) => step.join(" "));
    expect(steps[2]).toMatch(/^Then turn on full trust\. Installing does not: Settings → General → About → Certificate Trust Settings → Trawler Proxy CA\./);
    expect(steps[2]).toMatch(/Until it is on, Safari says This Connection Is Not Private\./);
  });
});

// C1: Windows PowerShell 5.1 - what Windows opens from Start, Win+X and Terminal -
// rejects `&&`, so a command that joins with it sets nothing.
describe("Windows' commands run in the shell their sentence names", () => {
  const windowsSteps = (id: PlatformId) => numbered(setup(id, { os: "windows" }));
  const leadOf = (step: GuideStep) => step.find((p) => typeof p === "string");

  it.each(["this", "windows"] as const)("%s: every command is led by PowerShell, and none joins with &&", (id) => {
    for (const step of windowsSteps(id).filter((st) => commands([st]).length > 0)) {
      expect(leadOf(step)).toMatch(/^In PowerShell( as administrator)?, run:$/);
    }
    for (const command of commands(guides(setup(id, { os: "windows" })))) expect(command).not.toMatch(/&&/);
  });

  it("the route writes the address, then enables it, joined as PowerShell runs them", () => {
    const [route] = commands(setup("windows").route.guide!.steps);
    expect(route).toMatch(/\/v ProxyServer \/t REG_SZ \/d 192\.168\.1\.24:9999 \/f; reg add .* \/v ProxyEnable \/t REG_DWORD \/d 1 \/f$/);
    expect(leadOf(setup("windows").route.guide!.steps[0])).toBe("In PowerShell, run:");
  });

  it("the certificate is installed as administrator, whose terminal opens elsewhere", () => {
    expect(commands(setup("windows").trust.guide!.steps)).toEqual([
      'certutil -addstore -f ROOT "$HOME\\Downloads\\trawler-ca.pem"',
    ]);
  });
});

// C2: a browser saves the certificate to Downloads, and a terminal does not
// open there, so another computer's trust command names the folder.
describe("another computer's trust command reads the file its download step names, from Downloads", () => {
  const downloaded = (id: PlatformId) =>
    setup(id).trust.guide!.steps[0].find((p): p is string => typeof p === "string" && /it downloads/.test(p))!;

  it.each([
    ["mac", "~/Downloads/trawler-ca.pem"],
    ["windows", '"$HOME\\Downloads\\trawler-ca.pem"'],
    ["ubuntu", '"$(xdg-user-dir DOWNLOAD)/trawler-ca.pem"'],
  ] as const)("%s", (id, path) => {
    const file = downloaded(id).match(/it downloads (\S+)\.$/)![1];
    const installs = commands(setup(id).trust.guide!.steps);

    expect(path).toContain(file);
    expect(installs.length).toBeGreaterThan(0);
    for (const install of installs) expect(install.endsWith(path) || install.includes(`${path} `)).toBe(true);
  });
});

// C1-C4: Linux keeps three certificate stores, and the browsers read neither
// of the other two, so a step that writes one cannot make another decrypt.
describe("Linux's certificate task installs into every store, each a step of its own", () => {
  const LINUX = [
    ["this", "linux"],
    ["this", "other"],
    ["ubuntu", "macos"],
  ] as const;
  const trustOf = (id: PlatformId, os: HostOs) => setup(id, { os }).trust.guide!;
  const leads = (steps: GuideStep[]) => steps.map((step) => step.find((p) => typeof p === "string"));

  it.each(LINUX)("%s on %s: Firefox, then Chrome, then the system's tools", (id, os) => {
    const [, ...stores] = trustOf(id, os).steps;

    expect(leads(stores)).toEqual([
      'In Firefox: Settings → Privacy & Security → Certificates → View Certificates → Authorities → Import → trawler-ca.pem, then tick "Trust this CA to identify websites".',
      "For Chrome, run:",
      "For curl and the system's tools, run:",
    ]);
    expect(trustOf(id, os).alternatives).toBeUndefined();
  });

  it.each(LINUX)("%s on %s: the way back undoes each store, in the same order", (id, os) => {
    const { undo } = trustOf(id, os);

    expect(leads(undo!)).toEqual([
      "In Firefox: Settings → Privacy & Security → Certificates → View Certificates → Authorities → Trawler Proxy CA → Delete or Distrust.",
      "For Chrome, run:",
      "For curl and the system's tools, run:",
    ]);
  });

  it.each(LINUX)("%s on %s: Chrome's command installs certutil, then finds whichever database Chrome made", (id, os) => {
    const NSS_DB = '"sql:$([ -d ~/.pki/nssdb ] && echo ~/.pki/nssdb || echo ~/.local/share/pki/nssdb)"';
    const [chrome] = commands([trustOf(id, os).steps[2]]);
    const [chromeRemoval] = commands([trustOf(id, os).undo![1]]);

    expect(chrome).toMatch(/^sudo apt install -y libnss3-tools && /);
    expect(chrome).toContain(`-d ${NSS_DB} -A -t C,, -n trawler -i `);
    expect(chromeRemoval).toBe(`certutil -d ${NSS_DB} -D -n trawler`);
  });

  it("this machine's commands and another Ubuntu machine's read the Downloads folder Ubuntu names", () => {
    const [, ...thisMachine] = commands(trustOf("this", "linux").steps);
    const [, ...ubuntu] = commands(trustOf("ubuntu", "macos").steps);

    expect(thisMachine).toEqual(ubuntu);
    expect(ubuntu).toEqual([
      'sudo cp "$(xdg-user-dir DOWNLOAD)/trawler-ca.pem" /usr/local/share/ca-certificates/trawler-ca.crt && sudo update-ca-certificates',
    ]);
  });

  // S2: another Ubuntu machine's browsers keep stores of their own, so after a
  // proof a refusal is first one of them; the emulator's, its own services
  // (W8's S2); elsewhere the platform says nothing.
  it("only another Ubuntu machine, the two Androids and the iPhone say what a refusal after a proof is", () => {
    expect(setup("ubuntu").refusedAfterProof).toBe(
      "something else is connecting and not decrypting — one request already did, so it is a browser with a store of its own (Firefox, Chrome), an app that pins its certificate, or another device.",
    );
    // W10's S4: so does an Android device, through its own services.
    for (const id of ["android-sim", "android"] as const) {
      expect(setup(id).refusedAfterProof).toBe(
        "something else is connecting and not decrypting — one request already did, so it is Android's own services, which ignore a user certificate, an app that pins its certificate, or another device.",
      );
    }
    // W9's S4: an iPhone refuses most through Apple's own services.
    expect(setup("ios").refusedAfterProof).toBe(
      "something else is connecting and not decrypting — one request already did, so it is Apple's own services, which pin their certificates, an app that pins its certificate, or another device.",
    );
    for (const id of PLATFORM_IDS.filter((p) => !["ubuntu", "android-sim", "android", "ios"].includes(p))) {
      for (const os of ["macos", "windows", "linux", "other"] as const) {
        expect(setup(id, { os }).refusedAfterProof).toBeUndefined();
      }
    }
  });
});

// Review round 1: a Chrome installed and never opened has made neither NSS
// database, and certutil adds only to a database it was asked to create.
test("Linux's Chrome command creates the database a Chrome since M146 reads where none exists, before adding to it", () => {
  const chrome = commands([setup("ubuntu").trust.guide!.steps[2]])[0];
  const create =
    "{ [ -d ~/.pki/nssdb ] || [ -d ~/.local/share/pki/nssdb ] || { mkdir -p ~/.local/share/pki/nssdb && certutil -d sql:$HOME/.local/share/pki/nssdb -N --empty-password; }; }";

  expect(chrome).toContain(`libnss3-tools && ${create} && certutil -d `);
  expect(chrome.indexOf(" -N ")).toBeLessThan(chrome.indexOf(" -A "));
});

// W8: the emulator's steps, as written, reach every emulator running and send
// its traffic to Trawler on whichever network it uses.
describe("the emulator's commands reach every emulator running, and route it on every network", () => {
  const emulator = (over: Partial<Parameters<typeof setupFor>[1]> = {}) => setup("android-sim", over);

  // C1: the push named a bare file, and a terminal does not open in Downloads.
  it("pushes the file where the save put it, quoted, and from Downloads before a save", () => {
    const [push] = commands(emulator({ saved: { cer: "/Users/me/My Files/trawler-ca.cer" } }).trust.guide!.steps);
    expect(push).toContain("push '/Users/me/My Files/trawler-ca.cer' /sdcard/Download/");
    expect(commands(emulator({ os: "windows" }).trust.guide!.steps)[0]).toContain(
      'push "$HOME\\Downloads\\trawler-ca.cer" /sdcard/Download/',
    );
    expect(commands(emulator({ os: "linux" }).trust.guide!.steps)[0]).toContain(
      'push "$(xdg-user-dir DOWNLOAD)/trawler-ca.cer" /sdcard/Download/',
    );
    expect(everything(emulator())).not.toMatch(/adb push trawler-ca\.cer/);
  });

  // C2: a bare adb refuses with two of anything attached.
  it.each(["macos", "linux", "windows"] as const)("on %s, every adb command runs once per emulator running", (os) => {
    const all = commands(guides(emulator({ os }))).filter((c) => /\badb\b/.test(c));
    expect(all.length).toBe(3);
    for (const command of all) {
      expect(command).toMatch(
        os === "windows"
          ? /^adb devices \| Select-String '\^emulator-\\d\+' \| ForEach-Object \{ adb -s \$_\.Matches\[0\]\.Value .+ \}$/
          : /^for d in \$\(adb devices \| grep -Eo '\^emulator-\[0-9\]\+'\); do adb -s \$d .+; done$/,
      );
      if (os === "windows") expect(command).not.toMatch(/&&/);
    }
  });

  it("leads each command with the shell it runs in", () => {
    const leads = (os: HostOs) =>
      both(emulator({ os })).flatMap((g) => [...g.steps, ...(g.undo ?? [])])
        .filter((step) => commands([step]).some((c) => /\badb\b/.test(c)))
        .map((step) => step.find((p): p is string => typeof p === "string"));
    expect(leads("windows")).toEqual(["With the emulator running, in PowerShell, run:", "In PowerShell, run:", "In PowerShell, run:"]);
    expect(leads("macos")).toEqual(["With the emulator running, in a terminal, run:", "In a terminal, run:", "In a terminal, run:"]);
  });

  // C4 · Q3: a Wi-Fi proxy routes nothing where the emulator sends over its
  // mobile network; the global proxy covers both.
  it("routes through the global proxy, and turns it off the same way", () => {
    const route = emulator({ proxyAddr: "127.0.0.1:8123" }).route.guide!;
    expect(commands(route.steps)).toEqual([
      "for d in $(adb devices | grep -Eo '^emulator-[0-9]+'); do adb -s $d shell settings put global http_proxy 10.0.2.2:8123; done",
    ]);
    expect(commands(route.undo!)).toEqual([
      "for d in $(adb devices | grep -Eo '^emulator-[0-9]+'); do adb -s $d shell settings put global http_proxy :0; done",
    ]);
  });

  it("offers the Settings route only with mobile data off, and the start-up flag on this machine's own address", () => {
    const route = emulator({ proxyAddr: "127.0.0.1:8123" }).route.guide!;
    const [settings, flag] = route.alternatives!;
    expect(settings[0]).toMatch(/turn Mobile data off, then AndroidWifi ⚙ → ✎ → Advanced options → Proxy → Manual/);
    expect(values([settings])).toEqual(["10.0.2.2", "8123"]);
    // C3: the flag is read by the emulator process on this machine, where 10.0.2.2 is nothing.
    expect(commands([flag])).toEqual(["emulator -avd <name> -http-proxy http://127.0.0.1:8123"]);
    expect(everything(emulator())).not.toMatch(/-http-proxy http:\/\/10\.0\.2\.2/);
  });

  // C5: Android 15 has no Settings → Security, and a pushed file is not under Recent.
  it("names Android 15's own paths, to install and to remove", () => {
    const s = emulator();
    expect(prose(s)).toContain(
      "Install it: Settings → Security & privacy → More security & privacy → Encryption & credentials → Install a certificate → CA certificate → Install anyway → ☰ → Downloads → trawler-ca.cer.",
    );
    expect(s.trust.guide!.undo).toEqual([
      [
        "Settings → Security & privacy → More security & privacy → Encryption & credentials → Trusted credentials → User → Trawler Proxy CA → Remove.",
      ],
    ]);
    expect(prose(s)).not.toMatch(/Settings → Security →|Settings → Wi-Fi →/);
  });

  it("is the one platform adb reads, and every platform but This machine marks its own request", () => {
    expect(PLATFORM_IDS.filter(readsOverAdb)).toEqual(["android-sim"]);
    expect(PLATFORM_IDS.filter(marksItsOwn)).toEqual(PLATFORM_IDS.filter((id) => id !== "this"));
  });
});

// W10's C1-C3, Q3: Chrome saves the download under a name of its own, a maker
// moves the installer where Settings' search still finds it, and search finds
// no proxy, so the proxy is the stock path, said to differ by maker.
describe("the Android device's steps name what a stock Android shows", () => {
  const device = () => setup("android");

  it("names no file the download may not leave", () => {
    expect(device().trust.guide!.steps[0]).toContain("— it downloads Trawler's certificate.");
    const said = device().trust.guide!.steps.flat().filter((p) => typeof p === "string");
    expect(said.join("\n")).not.toMatch(/trawler-ca\.cer\b/);
  });

  it("reaches the installer by Settings' search, past its warning", () => {
    expect(device().trust.guide!.steps[1]).toEqual([
      "Install it: in Settings, search CA certificate → CA certificate → Install anyway → the trawler-ca file it downloaded.",
    ]);
    expect(prose(device())).not.toMatch(/Settings → Security →/);
  });

  it("routes by the stock path, led by the maker's difference, and turns it off by the same", () => {
    const path = "Settings → Network & internet → Internet → ⚙ by the network it is on → ✎ → Advanced options → Proxy";
    expect(device().route.guide!.steps[0]).toEqual([`Menus differ by maker; on stock Android: ${path} → Manual.`]);
    expect(device().route.guide!.undo).toEqual([[`${path} → None.`]]);
    expect(prose(device())).not.toMatch(/Settings → Wi-Fi/);
  });
});

// W11's S4: the quit dialog shows routeUndo, so it must be what setup's own
// "to turn it off later" says, on every platform and every host.
describe("each platform's way to stop routing is one source", () => {
  const HOSTS: HostOs[] = ["macos", "windows", "linux", "other"];
  it.each(HOSTS)("on %s, setup's undo is routeUndo for every platform", (os) => {
    for (const id of PLATFORM_IDS) {
      expect(setupFor(id, ctx({ os })).route.guide?.undo, `${id}`).toEqual(routeUndo(id, os));
    }
  });

  it("needs no address: an off-machine platform has its way back before reach is armed", () => {
    for (const id of ["mac", "windows", "ubuntu", "ios", "android"] as PlatformId[]) {
      expect(setupFor(id, ctx({ reach: null })).route.guide).toBeUndefined();
      expect(routeUndo(id, "macos").length, id).toBeGreaterThan(0);
    }
  });

  it("names the stock Android path W10 found, never the one stock Android lacks", () => {
    const android = JSON.stringify(routeUndo("android", "macos"));
    expect(android).toContain("Network & internet");
    expect(android).not.toContain("Wi-Fi → your network");
  });
});

// W12: each by-hand step on This Mac offers the easiest route there is - a
// button that opens the screen, a recording where the user must still find the
// control - and the command is the other way.
describe("This Mac's by-hand routes", () => {
  const parts = (steps: GuideStep[]) => steps.flat();
  const routes = (steps: GuideStep[]) => parts(steps).filter(isRoute).map((p) => p.open);
  const paths = (steps: GuideStep[]) => parts(steps).filter(isPath).map((p) => p.path.join(" → "));
  const media = (steps: GuideStep[]) => parts(steps).filter(isMedia);

  it("the certificate opens in Keychain Access, then the one control no route reaches is shown", () => {
    const trust = setup("this").trust.guide!;

    expect(routes(trust.steps)).toEqual(["keychain"]);
    expect(paths(trust.steps)).toContain("Trust → When using this certificate → Always Trust");
    expect(media(trust.steps)).toEqual([
      expect.objectContaining({ media: "macos-always-trust", shows: "macOS 26" }),
    ]);
    expect(media(trust.steps)[0].alt).toContain("Trust → When using this certificate → Always Trust");
  });

  it("the certificate's command is the other way, after the save it reads", () => {
    const trust = setup("this").trust.guide!;

    expect(trust.alternatives?.[0].some(isSave)).toBe(true);
    expect(commands(trust.alternatives!)).toEqual([
      "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/Downloads/trawler-ca.pem",
    ]);
    expect(commands(trust.steps)).toEqual([]);
  });

  it("removing it reaches the login keychain the route adds to, and the System keychain the command does", () => {
    const removal = commands(setup("this").trust.guide!.undo!).join("\n");

    expect(removal).toContain('security delete-certificate -c "Trawler Proxy CA" login.keychain');
    expect(removal).toContain('sudo security delete-certificate -c "Trawler Proxy CA" /Library/Keychains/System.keychain');
  });

  it("the proxy opens its settings pane, shows the path to the sheet, and gives the two values to type", () => {
    const route = setup("this").route.guide!;

    expect(routes(route.steps)).toEqual(["proxy-settings"]);
    expect(paths(route.steps)).toContain("Wi-Fi → Details… → Proxies");
    expect(media(route.steps).map((m) => m.media)).toEqual(["macos-proxies"]);
    expect(values(route.steps)).toEqual(["127.0.0.1", "9999"]);
    expect(commands(route.alternatives!)).toEqual([
      "networksetup -setwebproxy Wi-Fi 127.0.0.1 9999 && networksetup -setsecurewebproxy Wi-Fi 127.0.0.1 9999",
    ]);
    expect(route.notes).toEqual([
      "On Ethernet, click Ethernet in place of Wi-Fi, and change Wi-Fi to Ethernet in the command.",
    ]);
  });

  it("every sentence that leads a command names where it runs (W12's W3)", () => {
    const leads = guides(setup("this"))
      .filter((step) => step.some(isCommand))
      .map((step) => step.find((p): p is string => typeof p === "string"));
    expect(leads.every((lead) => lead === "In Terminal, run:" || lead === "Or in Terminal, run:")).toBe(true);
  });

  it("the request's other ways are the browser, by route, then Terminal", () => {
    const s = setup("this");
    expect(s.tryRoutes).toEqual([{ open: "browser", label: "open example.com in your browser" }]);
    expect(s.tryLead).toBe("In Terminal, run:");
  });

  it("the Simulator follows this Mac's proxy, so it takes this Mac's route", () => {
    expect(setup("ios-sim").route.guide).toEqual(setup("this").route.guide);
  });

  it("no route or recording is offered on linux yet", () => {
    const s = setup("this", { os: "linux" });
    expect(routes(guides(s))).toEqual([]);
    expect(media(guides(s))).toEqual([]);
    expect(s.tryRoutes).toBeUndefined();
  });
});

describe("This PC's by-hand routes", () => {
  const pc = () => setup("this", { os: "windows" });
  const routes = (steps: GuideStep[]) => steps.flat().filter(isRoute).map((p) => p.open);
  const media = (steps: GuideStep[]) => steps.flat().filter(isMedia).map((p) => p.media);

  it("the certificate opens in Windows' own dialog, its store page recorded", () => {
    const trust = pc().trust.guide!;
    expect(routes(trust.steps)).toEqual(["certificate"]);
    expect(media(trust.steps)).toEqual(["windows-install-cert"]);
    expect(commands(trust.alternatives!)).toEqual(["certutil -addstore -f ROOT \"$HOME\\Downloads\\trawler-ca.pem\""]);
  });

  it("the user's own import comes off without an administrator, set up this PC's with one", () => {
    expect(commands(pc().trust.guide!.undo!)).toEqual([
      'certutil -user -delstore ROOT "Trawler Proxy CA"',
      'certutil -delstore ROOT "Trawler Proxy CA"',
    ]);
  });

  it("the proxy opens on its Settings page, the values to type copyable, the command another way", () => {
    const route = pc().route.guide!;
    expect(routes(route.steps)).toEqual(["proxy-settings"]);
    expect(media(route.steps)).toEqual(["windows-proxy"]);
    expect(values(route.steps)).toEqual(["127.0.0.1", "9999"]);
    expect(commands(route.alternatives!)).toHaveLength(1);
  });

  it("the request's other ways are the browser, by route, then PowerShell", () => {
    expect(pc().tryRoutes).toEqual([{ open: "browser", label: "open example.com in your browser" }]);
    expect(pc().tryLead).toBe("In PowerShell, run:");
  });
});
