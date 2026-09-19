import {
  canAutoSetup,
  defaultTarget,
  guidedSteps,
  hostOs,
  proveItSnippet,
  proxySnippet,
  proxySteps,
  removalSnippet,
  removalSteps,
  splitProxyAddr,
  TARGETS,
  trustSnippet,
} from "./setup";
import type { TrustTarget } from "./setup";
import { PROXY_TARGETS, type ProxyTarget } from "./proxyTargets";

const UA = {
  mac: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  win: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)",
  linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko)",
};

test("hostOs maps webview UA samples to the host family", () => {
  expect(hostOs(UA.mac)).toBe("macos");
  expect(hostOs(UA.win)).toBe("windows");
  expect(hostOs(UA.linux)).toBe("linux");
  expect(hostOs("Mozilla/5.0 (darwin) jsdom/25.0.1")).toBe("other");
});

test("only macOS and Windows have a one-click setup path", () => {
  expect(canAutoSetup("macos")).toBe(true);
  expect(canAutoSetup("windows")).toBe(true);
  expect(canAutoSetup("linux")).toBe(false);
  expect(canAutoSetup("other")).toBe(false);
});

test("the selector defaults to the detected host, falling back to linux", () => {
  expect(defaultTarget("macos")).toBe("macos");
  expect(defaultTarget("windows")).toBe("windows");
  expect(defaultTarget("linux")).toBe("linux");
  expect(defaultTarget("other")).toBe("linux");
});

test("guidedSteps covers every selectable target with plain-text instructions", () => {
  for (const { id } of TARGETS) {
    const steps = guidedSteps(id);
    expect(steps.length).toBeGreaterThan(1);
    for (const step of steps) expect(typeof step).toBe("string");
  }
});

const expects: [TrustTarget, RegExp][] = [
  ["macos", /security add-trusted-cert .* \/Library\/Keychains\/System\.keychain/],
  ["windows", /certutil -addstore -f ROOT/],
  ["linux", /update-ca-certificates/],
  ["firefox", /own certificate store/],
  ["ios-sim", /host Mac's trust store/],
  ["android", /adb push/],
];

test.each(expects)("guidedSteps(%s) carries the system-specific instruction", (target, rx) => {
  expect(guidedSteps(target).join("\n")).toMatch(rx);
});

test("removalSteps covers every target with plain-text instructions", () => {
  for (const { id } of TARGETS) {
    const steps = removalSteps(id);
    expect(steps.length).toBeGreaterThan(0);
    for (const step of steps) expect(typeof step).toBe("string");
  }
});

const removals: [TrustTarget, RegExp][] = [
  ["macos", /security delete-certificate -c "Trawler Proxy CA"/],
  ["windows", /certutil -delstore ROOT "Trawler Proxy CA"/],
  ["linux", /update-ca-certificates --fresh/],
  ["firefox", /Delete or Distrust/],
  ["ios-sim", /restart the Simulator/],
  ["android", /Trusted credentials/],
];

test.each(removals)("removalSteps(%s) carries the system-specific removal", (target, rx) => {
  expect(removalSteps(target).join("\n")).toMatch(rx);
});

// --- Set the proxy (W3) ---

test("splitProxyAddr splits host and port on the last colon", () => {
  expect(splitProxyAddr("127.0.0.1:7420")).toEqual({ host: "127.0.0.1", port: 7420 });
  expect(splitProxyAddr("192.168.1.5:8080")).toEqual({ host: "192.168.1.5", port: 8080 });
});

const ADDR = "10.1.2.3:9999";

test("every proxy snippet and its steps carry the passed address, never a hardcoded port", () => {
  for (const { id } of PROXY_TARGETS) {
    const text = [proxySnippet(id, ADDR) ?? "", ...proxySteps(id, ADDR)].join("\n");
    expect(text).not.toMatch(/7420/);
    // GUI-only clients (firefox, ios-sim) name no numeric port at all.
    if (proxySnippet(id, ADDR)) expect(text).toContain("9999");
  }
});

const snippets: [ProxyTarget, RegExp][] = [
  ["macos", /networksetup -setsecurewebproxy .*10\.1\.2\.3 9999/],
  ["windows", /reg add .*ProxyServer .*10\.1\.2\.3:9999/],
  ["linux", /gsettings .*9999/],
  ["terminal", /HTTPS_PROXY=http:\/\/10\.1\.2\.3:9999/],
];

test.each(snippets)("proxySnippet(%s) is the copyable command with the address", (t, rx) => {
  expect(proxySnippet(t, ADDR)).toMatch(rx);
});

test("GUI-only clients have no copyable command", () => {
  expect(proxySnippet("firefox", ADDR)).toBeNull();
  expect(proxySnippet("ios-sim", ADDR)).toBeNull();
  expect(proxySnippet("android", ADDR)).toBeNull();
});

test("the Android steps say to use the machine's LAN address, not loopback", () => {
  const text = proxySteps("android", ADDR).join("\n");
  expect(text).toMatch(/LAN address/);
  expect(text).not.toMatch(/127\.0\.0\.1/);
});

test("proveItSnippet carries the real proxy address, never a hardcoded port", () => {
  expect(proveItSnippet("10.1.2.3:9999")).toBe("curl -x http://10.1.2.3:9999 https://example.com");
});

// withCommand replaces the step equal to the command, so the resolvers must put
// that command in exactly once — zero would drop the copy affordance, twice
// would render two CopyTexts for one command.
describe("every command is present exactly once in the steps that introduce it", () => {
  // Coverage of the union is tsc's job, not this file's: PROXY_TARGETS is built
  // from a Record keyed by ProxyTarget, so a member added without a label fails to
  // compile. A runtime check here could only compare the list with itself.
  it.each(TARGETS.map((t) => t.id))("%s: its trust and removal commands each appear once", (id) => {
    for (const [steps, command] of [
      [guidedSteps(id), trustSnippet(id)],
      [removalSteps(id), removalSnippet(id)],
    ] as [string[], string | null][]) {
      if (command === null) continue;
      expect(steps.filter((s) => s === command)).toHaveLength(1);
    }
  });

  // A command named but absent from its steps would render no CopyText at all;
  // one present but unnamed would stay uncopyable prose. Both are silent, so the
  // pairing is asserted in both directions.
  it.each(TARGETS.map((t) => t.id))("%s: no step is an unnamed command", (id) => {
    const looksLikeCommand = (step: string) =>
      /^(sudo|certutil|adb|networksetup|reg|gsettings|unset|export|curl) /.test(step);

    for (const [steps, command] of [
      [guidedSteps(id), trustSnippet(id)],
      [removalSteps(id), removalSnippet(id)],
    ] as [string[], string | null][]) {
      for (const step of steps.filter(looksLikeCommand)) {
        expect(step).toBe(command);
      }
    }
  });

  it.each(PROXY_TARGETS.map((t) => t.id))("%s", (id) => {
    const addr = "10.0.0.1:7420";
    const pairs: [string[], string | null][] = [[proxySteps(id, addr), proxySnippet(id, addr)]];

    for (const [steps, command] of pairs) {
      if (command === null) continue;
      expect(steps.filter((s) => s === command)).toHaveLength(1);
    }
  });
});
