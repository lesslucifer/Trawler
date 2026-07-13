import { canAutoInstall, defaultTarget, guidedSteps, hostOs, TARGETS } from "./setup";
import type { TrustTarget } from "./setup";

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

test("only macOS and Windows can auto-install", () => {
  expect(canAutoInstall("macos")).toBe(true);
  expect(canAutoInstall("windows")).toBe(true);
  expect(canAutoInstall("linux")).toBe(false);
  expect(canAutoInstall("other")).toBe(false);
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
