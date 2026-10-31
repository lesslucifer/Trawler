import { describe, expect, it } from "vitest";
import { proxyOffSnippet, proxyOffSteps, type ProxyTarget } from "./proxyTargets";

const TARGETS: ProxyTarget[] = ["macos", "windows", "linux", "terminal"];

describe("the reversal every target carries", () => {
  it("turns each proxy back off", () => {
    expect(proxyOffSnippet("macos")).toMatch(/-setwebproxystate .*off/);
    expect(proxyOffSnippet("windows")).toMatch(/ProxyEnable .*\/d 0/);
    expect(proxyOffSnippet("linux")).toMatch(/mode 'none'/);
    expect(proxyOffSnippet("terminal")).toMatch(/unset HTTPS_PROXY/);
  });

  // The off-command is replaced in place in the steps that introduce it, so it
  // must be there exactly once: zero drops the copy affordance, two render twice.
  it.each(TARGETS)("%s carries its command once, after its lead", (t) => {
    const steps = proxyOffSteps(t);
    expect(steps.filter((step) => step === proxyOffSnippet(t))).toHaveLength(1);
    expect(steps[steps.length - 1]).toBe(proxyOffSnippet(t));
    expect(new Set(steps).size).toBe(steps.length);
  });

  // W14's W1: the lead names the shell the command runs in, as every Windows
  // command's does, never the API it writes.
  it("leads Windows' command with PowerShell", () => {
    expect(proxyOffSteps("windows")).toEqual(["In PowerShell, run:", proxyOffSnippet("windows")]);
  });
});
