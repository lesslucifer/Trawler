import {
  canAutoSetup,
  defaultProxyTarget,
  defaultTarget,
  guidedSteps,
  hostOs,
  NO_TRAFFIC,
  proveItSnippet,
  RECENT_MS,
  trafficStatus,
  TUNNEL_GRACE_MS,
  PROXY_TARGETS,
  proxyOffSnippet,
  proxyOffSteps,
  proxySnippet,
  proxyStatus,
  proxySteps,
  removalSteps,
  splitProxyAddr,
  TARGETS,
  removalSnippet,
  trustSnippet,
  trustStatus,
  type StepStatusView,
} from "./setup";
import type { ProxyTarget, Traffic, TrustTarget } from "./setup";

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

// --- Two layers of truth (E-REDESIGN.W10) ---
// Steps 1 and 2 are this machine, from the probes, and take no traffic input at
// all: a flow captured an hour ago cannot turn a step green, because there is no
// argument to hand it through.

test("trustStatus: the local probe is the whole verdict", () => {
  expect(trustStatus("trusted", null)).toMatchObject({ glyph: "✓", tone: "ok", short: "trusted on this machine" });
  expect(trustStatus("untrusted", null)).toMatchObject({ glyph: "○", tone: "muted" });
  expect(trustStatus("untrusted", null).short).toBe("not trusted on this machine yet");
});

test("trustStatus: an un-probeable OS says where the answer will show, never 'not set up'", () => {
  const v = trustStatus("unknown", null);
  expect(v.glyph).toBe("◐");
  expect(v.remedy).toBe("step 3 shows whether traffic decrypts");
  expect(v.short).not.toMatch(/not trusted/i);
});

test("trustStatus: a running install, a running check and a failure each surface their own state", () => {
  expect(trustStatus("untrusted", "installing").short).toBe("installing…");
  expect(trustStatus("trusted", "checking")).toMatchObject({ glyph: "⟳", short: "checking…" });
  expect(trustStatus("untrusted", null, "denied")).toMatchObject({ tone: "warn", remedy: "denied" });
});

test("trustStatus: the probe's positive wins over a stale action error", () => {
  expect(trustStatus("trusted", null, "denied")).toMatchObject({ tone: "ok", glyph: "✓" });
});

test("proxyStatus: the local probe is the whole verdict", () => {
  expect(proxyStatus("ours", null)).toMatchObject({ tone: "ok", glyph: "✓" });
  expect(proxyStatus("none", null)).toMatchObject({ glyph: "○", tone: "muted" });
  expect(proxyStatus("none", null).short).toBe("not set on this machine yet");
});

test("proxyStatus: not-set vs can't-check are distinct, both calm", () => {
  expect(proxyStatus("unknown", null).glyph).toBe("◐");
  expect(proxyStatus("unknown", null).remedy).toBe("step 3 shows whether traffic arrives");
  expect(proxyStatus("unknown", null).short).not.toBe(proxyStatus("none", null).short);
});

test("proxyStatus: a foreign proxy names the conflict and what to do", () => {
  const foreign = proxyStatus("foreign", null);
  expect(foreign).toMatchObject({ glyph: "○", tone: "muted", short: "another proxy is set" });
  expect(foreign.remedy).toMatch(/^point it at Trawler here/);
});

test("proxyStatus: the probe's positive wins over a stale set error; a check outranks both", () => {
  expect(proxyStatus("ours", null, "denied")).toMatchObject({ tone: "ok", glyph: "✓" });
  expect(proxyStatus("ours", "checking", "denied")).toMatchObject({ glyph: "⟳", short: "checking…" });
});

// Step 3 is the traffic layer, judged against a clock. The complaint that
// produced it: a flow captured earlier kept the screen green after the proxy
// had been turned off, because "seen" had no time attached.
describe("trafficStatus reads recency, never history", () => {
  const NOW = 1_000_000;
  const ago = (ms: number) => NOW - ms;
  const at = (seen: Partial<Traffic>) => trafficStatus({ ...NO_TRAFFIC, ...seen }, NOW);

  it("nothing ever: waiting for the first request", () => {
    expect(at({})).toMatchObject({ glyph: "○", tone: "muted", short: "waiting for your first request" });
  });

  it("a decrypted flow inside the window is capturing", () => {
    expect(at({ lastDecryptedAt: ago(RECENT_MS) })).toMatchObject({ glyph: "✓", tone: "ok", short: "capturing traffic" });
  });

  it("a decrypted flow past the window is idle — old evidence is not current evidence", () => {
    const v = at({ lastDecryptedAt: ago(RECENT_MS + 1) });
    expect(v).toMatchObject({ glyph: "○", tone: "muted", short: "idle — nothing arriving now" });
    expect(v.remedy).toBeUndefined();
  });

  it("a tally with no time can only ever mean idle", () => {
    expect(at({ ever: true }).short).toBe("idle — nothing arriving now");
  });

  it("a tunnel inside its grace is neither proof nor failure", () => {
    expect(at({ lastTunnelAt: ago(TUNNEL_GRACE_MS - 1) })).toMatchObject({ glyph: "⟳", short: "a connection just arrived…" });
  });

  it("a tunnel that no flow answered is arriving but not decrypting, and says whose certificate is missing", () => {
    const v = at({ lastTunnelAt: ago(TUNNEL_GRACE_MS) });
    expect(v).toMatchObject({ glyph: "⚠", tone: "warn", short: "arriving but not decrypting" });
    expect(v.remedy).toBe(
      "the client sending it doesn't trust Trawler's certificate — install it there; step 1 covers this machine",
    );
  });

  it("a tunnel answered by a decrypted flow is capturing", () => {
    expect(at({ lastTunnelAt: ago(5_000), lastDecryptedAt: ago(4_000) }).short).toBe("capturing traffic");
  });

  it("positive evidence outranks a failing tunnel while both are recent", () => {
    expect(at({ lastDecryptedAt: ago(10_000), lastTunnelAt: ago(TUNNEL_GRACE_MS) }).tone).toBe("ok");
  });

  it("an unanswered tunnel past the window is idle, not a standing alarm", () => {
    expect(at({ lastTunnelAt: ago(RECENT_MS + 1) }).short).toBe("idle — nothing arriving now");
  });

  it("plain HTTP arriving is its own verdict — the proxy works, nothing was there to decrypt", () => {
    const v = at({ lastPlainAt: ago(1_000) });
    expect(v).toMatchObject({ glyph: "◐", tone: "muted", short: "arriving — plain HTTP only" });
    expect(v.remedy).toBe("an HTTPS request is what proves decryption");
  });

  it("a failing tunnel outranks plain HTTP: the tunnel is the failure the plain flow cannot excuse", () => {
    expect(at({ lastPlainAt: ago(1_000), lastTunnelAt: ago(5_000) }).tone).toBe("warn");
  });
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

// The nineteen verdicts, asserted literally against design run #9's table (as
// amended in W10). The screen may not shorten a domain message (StatusLine
// @avoid), so the wording is this module's contract — a drift here is a design
// defect, not a detail.
describe("the short form every verdict carries", () => {
  const NOW = 1_000_000;
  const seen = (over: Partial<Traffic>): Traffic => ({ ...NO_TRAFFIC, ...over });

  const VERDICTS: [string, StepStatusView, string, string | undefined][] = [
    ["trust · checking", trustStatus("trusted", "checking"), "checking…", undefined],
    ["trust · installing", trustStatus("unknown", "installing"), "installing…", undefined],
    ["trust · probe trusted", trustStatus("trusted", null), "trusted on this machine", undefined],
    ["trust · error", trustStatus("untrusted", null, "boom"), "install failed", "boom"],
    [
      "trust · probe unknown",
      trustStatus("unknown", null),
      "can't check automatically",
      "step 3 shows whether traffic decrypts",
    ],
    ["trust · not trusted", trustStatus("untrusted", null), "not trusted on this machine yet", undefined],
    ["proxy · checking", proxyStatus("ours", "checking"), "checking…", undefined],
    ["proxy · setting", proxyStatus("unknown", "setting"), "setting the proxy…", undefined],
    ["proxy · probe ours", proxyStatus("ours", null), "system proxy is set to Trawler", undefined],
    ["proxy · error", proxyStatus("none", null, "port busy"), "couldn't set the proxy", "port busy"],
    [
      "proxy · probe foreign",
      proxyStatus("foreign", null),
      "another proxy is set",
      "point it at Trawler here, or capture from a device configured to use Trawler",
    ],
    [
      "proxy · probe unknown",
      proxyStatus("unknown", null),
      "can't check automatically",
      "step 3 shows whether traffic arrives",
    ],
    ["proxy · none", proxyStatus("none", null), "not set on this machine yet", undefined],
    ["traffic · capturing", trafficStatus(seen({ lastDecryptedAt: NOW }), NOW), "capturing traffic", undefined],
    [
      "traffic · tunnel in grace",
      trafficStatus(seen({ lastTunnelAt: NOW }), NOW),
      "a connection just arrived…",
      undefined,
    ],
    [
      "traffic · tunnel unanswered",
      trafficStatus(seen({ lastTunnelAt: NOW - TUNNEL_GRACE_MS }), NOW),
      "arriving but not decrypting",
      "the client sending it doesn't trust Trawler's certificate — install it there; step 1 covers this machine",
    ],
    [
      "traffic · plain only",
      trafficStatus(seen({ lastPlainAt: NOW }), NOW),
      "arriving — plain HTTP only",
      "an HTTPS request is what proves decryption",
    ],
    ["traffic · idle", trafficStatus(seen({ ever: true }), NOW), "idle — nothing arriving now", undefined],
    ["traffic · waiting", trafficStatus(NO_TRAFFIC, NOW), "waiting for your first request", undefined],
  ];

  it("covers every verdict the three resolvers can return", () => {
    expect(VERDICTS).toHaveLength(19);
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

    expect(trustStatus("untrusted", null, sprawling).short).toBe("install failed");
    expect(proxyStatus("none", null, sprawling).short).toBe("couldn't set the proxy");
  });

  it("carries a verdict and nothing else — the instruction is a separate field", () => {
    const foreign = proxyStatus("foreign", null);

    expect(foreign.short).toBe("another proxy is set");
    expect(foreign.short).not.toMatch(/point it at Trawler/);
    expect(foreign.remedy).toMatch(/^point it at Trawler here/);
  });
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

