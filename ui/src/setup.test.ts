import {
  canAutoSetup,
  defaultProxyTarget,
  defaultTarget,
  guidedSteps,
  hostOs,
  proveItSnippet,
  PROXY_TARGETS,
  proxyOffSnippet,
  proxyOffSteps,
  proxySnippet,
  proxyStatus,
  proxySteps,
  removalSteps,
  splitProxyAddr,
  TARGETS,
  trustStatus,
  type StepStatusView,
  verifyStatus,
} from "./setup";
import type { Observed, ProxyTarget, TrustTarget } from "./setup";

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

test("the proxy selector defaults to the detected host, falling back to terminal", () => {
  expect(defaultProxyTarget("macos")).toBe("macos");
  expect(defaultProxyTarget("windows")).toBe("windows");
  expect(defaultProxyTarget("linux")).toBe("linux");
  expect(defaultProxyTarget("other")).toBe("terminal");
});

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

// --- Honest per-step status derivation (W6) ---

const NONE: Observed = { decrypted: false, trafficSeen: false };
const TRAFFIC: Observed = { decrypted: false, trafficSeen: true };
const DECRYPTED: Observed = { decrypted: true, trafficSeen: true };

test("trustStatus: the local probe is the fast, green ~90% path", () => {
  const v = trustStatus("trusted", NONE, null);
  expect(v).toMatchObject({ glyph: "✓", tone: "ok" });
  expect(v.short).toBe("trusted on this machine");
});

test("trustStatus: a decrypted flow proves trust even when the probe can't (remote/manual)", () => {
  expect(trustStatus("untrusted", DECRYPTED, null)).toMatchObject({ tone: "ok", glyph: "✓" });
  expect(trustStatus("unknown", DECRYPTED, null).short).toMatch(/confirmed by traffic/);
});

test("trustStatus: traffic arriving but not decrypting points at the sending device, as a warning not a red error", () => {
  const v = trustStatus("untrusted", TRAFFIC, null);
  expect(v.tone).toBe("warn");
  expect(v.remedy).toMatch(/install the certificate on the device you're capturing from/);
});

test("trustStatus: an un-probeable OS with no traffic says capture will confirm, never 'not set up'", () => {
  const v = trustStatus("unknown", NONE, null);
  expect(v.glyph).toBe("◐");
  expect(v.remedy).toMatch(/we'll confirm when traffic arrives/);
  expect(v.short).not.toMatch(/not trusted/i);
});

test("trustStatus: probed untrusted with nothing seen is a calm 'not trusted yet', not an alarm", () => {
  const v = trustStatus("untrusted", NONE, null);
  expect(v.glyph).toBe("○");
  expect(v.tone).toBe("muted");
});

test("trustStatus: a running install and a failure surface their own transient states", () => {
  expect(trustStatus("untrusted", NONE, "installing").short).toMatch(/installing/);
  expect(trustStatus("untrusted", NONE, null, "denied")).toMatchObject({ tone: "warn" });
});

test("trustStatus: an authoritative positive wins over a stale action error", () => {
  // Probe says trusted (or a flow decrypted) despite a failed last click — the CA
  // IS trusted, so show green, not a misleading 'Install failed'.
  expect(trustStatus("trusted", NONE, null, "denied")).toMatchObject({ tone: "ok", glyph: "✓" });
  expect(trustStatus("untrusted", DECRYPTED, null, "denied")).toMatchObject({ tone: "ok" });
});

test("proxyStatus: an authoritative positive wins over a stale set error", () => {
  expect(proxyStatus("ours", NONE, null, "denied")).toMatchObject({ tone: "ok", glyph: "✓" });
  expect(proxyStatus("none", TRAFFIC, null, "denied")).toMatchObject({ tone: "ok" });
});

test("proxyStatus: the probe or ANY connection turns it green, decrypt or not", () => {
  expect(proxyStatus("ours", NONE, null)).toMatchObject({ tone: "ok", glyph: "✓" });
  const seen = proxyStatus("none", TRAFFIC, null);
  expect(seen.tone).toBe("ok");
  expect(seen.short).toMatch(/receiving traffic/);
});

test("proxyStatus: nothing yet vs can't-check are distinct, both calm", () => {
  expect(proxyStatus("none", NONE, null)).toMatchObject({ glyph: "○", tone: "muted" });
  expect(proxyStatus("unknown", NONE, null).remedy).toMatch(/we'll confirm when traffic arrives/);
});

test("proxyStatus: a foreign proxy names the conflict, not a silent 'no traffic'", () => {
  const foreign = proxyStatus("foreign", NONE, null);
  expect(foreign).toMatchObject({ glyph: "○", tone: "muted" });
  expect(foreign.short).toMatch(/another proxy/);
  expect(foreign.short).not.toBe(proxyStatus("none", NONE, null).short);
});

test("verifyStatus flips green only on a decrypted flow", () => {
  expect(verifyStatus(NONE)).toMatchObject({ glyph: "○", tone: "muted" });
  expect(verifyStatus(TRAFFIC)).toMatchObject({ glyph: "○" }); // traffic alone isn't decrypt
  expect(verifyStatus(DECRYPTED)).toMatchObject({ glyph: "✓", tone: "ok" });
});

test("proveItSnippet carries the real proxy address, never a hardcoded port", () => {
  expect(proveItSnippet("10.1.2.3:9999")).toBe("curl -x http://10.1.2.3:9999 https://example.com");
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

// The sixteen verdicts, asserted literally against design run #9's table. The
// screen may not shorten a domain message (StatusLine @avoid), so the wording
// is this module's contract — a drift here is a design defect, not a detail.
describe("the short form every verdict carries", () => {
  const nothing = { decrypted: false, trafficSeen: false };
  const traffic = { decrypted: false, trafficSeen: true };
  const decrypted = { decrypted: true, trafficSeen: true };

  const VERDICTS: [string, StepStatusView, string, string | undefined][] = [
    ["trust · installing", trustStatus("unknown", nothing, "installing"), "installing…", undefined],
    ["trust · probe trusted", trustStatus("trusted", nothing, null), "trusted on this machine", undefined],
    ["trust · decrypted", trustStatus("untrusted", decrypted, null), "trusted — confirmed by traffic", undefined],
    ["trust · error", trustStatus("untrusted", nothing, null, "boom"), "install failed", "boom"],
    [
      "trust · traffic, no decrypt",
      trustStatus("untrusted", traffic, null),
      "arriving but not decrypting",
      "install the certificate on the device you're capturing from",
    ],
    [
      "trust · probe unknown",
      trustStatus("unknown", nothing, null),
      "can't check automatically",
      "we'll confirm when traffic arrives",
    ],
    ["trust · not trusted", trustStatus("untrusted", nothing, null), "not trusted on this machine yet", undefined],
    ["proxy · setting", proxyStatus("unknown", nothing, "setting"), "setting the proxy…", undefined],
    ["proxy · probe ours", proxyStatus("ours", nothing, null), "system proxy is set to Trawler", undefined],
    ["proxy · traffic seen", proxyStatus("none", traffic, null), "receiving traffic", undefined],
    ["proxy · error", proxyStatus("none", nothing, null, "port busy"), "couldn't set the proxy", "port busy"],
    [
      "proxy · probe foreign",
      proxyStatus("foreign", nothing, null),
      "another proxy is set",
      "point it at Trawler here, or capture from a device configured to use Trawler",
    ],
    [
      "proxy · probe unknown",
      proxyStatus("unknown", nothing, null),
      "can't check automatically",
      "we'll confirm when traffic arrives",
    ],
    ["proxy · none", proxyStatus("none", nothing, null), "no traffic reaching Trawler yet", undefined],
    ["verify · decrypted", verifyStatus(decrypted), "capturing traffic", undefined],
    ["verify · waiting", verifyStatus(nothing), "waiting for your first request", undefined],
  ];

  it("covers every verdict the three resolvers can return", () => {
    expect(VERDICTS).toHaveLength(16);
  });

  it.each(VERDICTS)("%s", (_name, view, short, remedy) => {
    expect(view.short).toBe(short);
    expect(view.remedy).toBe(remedy);
  });

  it.each(VERDICTS)("%s fits StatusLine's ~40-char contract", (_name, view) => {
    expect(view.short.length).toBeLessThanOrEqual(40);
  });

  it("keeps the error text out of the short form, however long it interpolates", () => {
    const sprawling = "a".repeat(500);

    expect(trustStatus("untrusted", nothing, null, sprawling).short).toBe("install failed");
    expect(proxyStatus("none", nothing, null, sprawling).short).toBe("couldn't set the proxy");
  });

  it("carries a verdict and nothing else — the instruction is a separate field", () => {
    const foreign = proxyStatus("foreign", nothing, null);

    expect(foreign.short).toBe("another proxy is set");
    expect(foreign.short).not.toMatch(/point it at Trawler/);
    expect(foreign.remedy).toMatch(/^point it at Trawler here/);
  });
});

// withCommand replaces the step equal to the command, so the resolvers must put
// that command in exactly once — zero would drop the copy affordance, twice
// would render two CopyTexts for one command.
describe("every command is present exactly once in the steps that introduce it", () => {
  it("PROXY_TARGETS covers the whole union, so the cases below are exhaustive", () => {
    // proxyOffSteps' switch is exhaustive over ProxyTarget (tsc proves that), so
    // every member it handles must also be offered in the list the screens read.
    const listed = PROXY_TARGETS.map((t) => t.id).sort();
    const handled = listed.filter((id) => proxyOffSteps(id).length > 0).sort();
    expect(handled).toEqual(listed);
    expect(listed).toHaveLength(7);
  });

  it.each(PROXY_TARGETS.map((t) => t.id))("%s", (id) => {
    const addr = "10.0.0.1:7420";
    const pairs: [string[], string | null][] = [
      [proxySteps(id, addr), proxySnippet(id, addr)],
      [proxyOffSteps(id), proxyOffSnippet(id)],
    ];

    for (const [steps, command] of pairs) {
      if (command === null) continue;
      expect(steps.filter((s) => s === command)).toHaveLength(1);
    }
  });

  it.each(PROXY_TARGETS.map((t) => t.id))("%s repeats no step", (id) => {
    const steps = proxyOffSteps(id);
    expect(new Set(steps).size).toBe(steps.length);
  });
});
