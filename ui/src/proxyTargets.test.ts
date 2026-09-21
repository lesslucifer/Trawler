import { describe, expect, it, test } from "vitest";
import {
  defaultProxyTarget,
  PROXY_TARGETS,
  proxyOffSteps,
} from "./proxyTargets";

test("the proxy selector defaults to the detected host, falling back to terminal", () => {
  expect(defaultProxyTarget("macos")).toBe("macos");
  expect(defaultProxyTarget("windows")).toBe("windows");
  expect(defaultProxyTarget("linux")).toBe("linux");
  expect(defaultProxyTarget("other")).toBe("terminal");
});

test("proxyOffSteps covers every target with the inverse action", () => {
  for (const { id } of PROXY_TARGETS) {
    const steps = proxyOffSteps(id);
    expect(steps.length).toBeGreaterThan(0);
    for (const step of steps) expect(typeof step).toBe("string");
  }
  expect(proxyOffSteps("macos").join("\n")).toMatch(/-setwebproxystate .*off/);
  expect(proxyOffSteps("windows").join("\n")).toMatch(/ProxyEnable .*\/d 0/);
  expect(proxyOffSteps("terminal").join("\n")).toMatch(/unset HTTPS_PROXY/);
});

// The off-command is replaced in place in the steps that introduce it, so the
// resolver must put it there exactly once — zero would drop the copy affordance,
// twice would render two CopyTexts for one command.
describe("the reversal every target carries", () => {
  // The list is derived from a Record now, so its order is insertion order rather
  // than something the reader can see in an array literal — and the order is what
  // the user sees in the picker.
  it("offers the targets in the order the picker has always shown", () => {
    expect(PROXY_TARGETS.map((t) => t.id)).toEqual([
      "macos",
      "windows",
      "linux",
      "firefox",
      "terminal",
      "ios-sim",
      "android",
    ]);
  });

  it("offers every target it can render steps for", () => {
    for (const { id, label } of PROXY_TARGETS) {
      expect(label).not.toBe("");
      expect(proxyOffSteps(id).length).toBeGreaterThan(0);
    }
  });

  it.each(PROXY_TARGETS.map((t) => t.id))("%s repeats no step", (id) => {
    const steps = proxyOffSteps(id);
    expect(new Set(steps).size).toBe(steps.length);
  });
});
