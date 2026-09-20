import { describe, expect, it, test } from "vitest";
import {
  canAutoSetup,
  confirmOn,
  deviceName,
  hostOs,
  reachTaskTitle,
  needsReach,
  probesRoute,
  probesTrust,
  PLATFORM_IDS,
  platformChoices,
  setupFor,
  splitProxyAddr,
  type GuideStep,
  type PlatformId,
  type PlatformSetup,
} from "./setup";

const UA = {
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  win: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)",
};

const ADDR = "127.0.0.1:9999";
const REACH = { proxy: "192.168.1.24:9999", caCertUrl: "http://192.168.1.24:7421/ca.crt" };
const ctx = (over: Partial<Parameters<typeof setupFor>[1]> = {}) => ({
  os: "macos" as const,
  proxyAddr: ADDR,
  reach: REACH,
  ...over,
});

const guides = (s: PlatformSetup): GuideStep[] => [
  ...(s.trust.guide?.steps ?? []),
  ...(s.trust.guide?.undo ?? []),
  ...(s.route.guide?.steps ?? []),
  ...(s.route.guide?.undo ?? []),
];

const titles = (s: PlatformSetup): string[] =>
  [s.trust, s.route, s.try].flatMap((t) => [t.title, t.why ?? ""]);

const prose = (s: PlatformSetup): string =>
  [...titles(s), s.confirmOn, s.tryCommand ?? ""]
    .concat(guides(s).flatMap((step) => step.map((p) => (typeof p === "string" ? p : ""))))
    .join("\n");

const copies = (s: PlatformSetup): string[] =>
  guides(s).flatMap((step) =>
    step.flatMap((p) => (typeof p === "object" && "copy" in p ? [p.copy] : [])),
  );

const saves = (s: PlatformSetup): number =>
  guides(s).flatMap((step) => step.filter((p) => typeof p === "object" && "save" in p)).length;

const everything = (s: PlatformSetup): string => [prose(s), ...copies(s)].join("\n");

const setup = (id: PlatformId, over = {}) => setupFor(id, ctx(over));

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

test("splitProxyAddr splits host and port on the last colon", () => {
  expect(splitProxyAddr("127.0.0.1:7420")).toEqual({ host: "127.0.0.1", port: 7420 });
  expect(splitProxyAddr("192.168.1.5:8080")).toEqual({ host: "192.168.1.5", port: 8080 });
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
  it.each(PLATFORM_IDS)("%s says where to open a website, and why it settles setup", (id) => {
    expect(setup(id).try.title).toMatch(/^Open any website /);
    expect(setup(id).try.why).toBe("the first decrypted request is what proves setup works");
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
    expect(text.replace(REACH.caCertUrl, "")).not.toMatch(/7421/);
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
    expect(copies(emulator())).toContain("adb push trawler-ca.pem /sdcard/Download/");
    expect(copies(device())).toContain(REACH.caCertUrl);
    expect(copies(device())).not.toContain("adb push trawler-ca.pem /sdcard/Download/");
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
    expect(copies(setup(id))).toContain(REACH.caCertUrl);
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
    expect(copies(s)).toContain("xcrun simctl keychain booted add-root-cert trawler-ca.pem");
    expect(saves(s)).toBe(1);
  });

  it("reverses that trust, and says what the reversal costs", () => {
    const s = setup("ios-sim");
    expect(copies(s)).toContain("xcrun simctl keychain booted reset");
    expect(prose(s)).toMatch(/clears everything in it/);
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
    const looksLikeCommand = (text: string) =>
      /^(sudo|certutil|adb|networksetup|reg |gsettings|unset|export|curl|emulator|xcrun) /.test(text);
    for (const step of guides(setup(id))) {
      for (const part of step) {
        if (typeof part === "string") expect(looksLikeCommand(part)).toBe(false);
      }
    }
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
      `we'll confirm this on ${deviceName(id, "macos")}'s first request`,
    );
  });

  it.each(PLATFORM_IDS.filter(needsReach))("%s's network task is titled for the device", (id) => {
    expect(reachTaskTitle(id, "macos")).toBe(`Allow ${deviceName(id, "macos")} to connect`);
  });

  it("a website is opened ON a machine and IN a simulator", () => {
    expect(setup("this").try.title).toBe("Open any website on this Mac");
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
