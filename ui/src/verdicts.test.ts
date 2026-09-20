import { describe, expect, test } from "vitest";
import {
  NO_TRAFFIC,
  proxyStatus,
  RECENT_MS,
  stepsLeftHere,
  trafficStatus,
  trustStatus,
  TUNNEL_GRACE_MS,
  type StepStatusView,
  type Traffic,
} from "./verdicts";

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
  expect(v.remedy).toBe("the traffic verdict is what proves it");
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
  expect(proxyStatus("unknown", null).remedy).toBe("the traffic verdict is what proves it");
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
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
    );
    // F21: the app cannot attribute a tunnel to a platform - the design run's
    // own blocked-on row says so - and the screen's platform choice is the
    // user's answer to a different question.
    expect(v.remedy).not.toMatch(/platform/);
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
      "can't check on this platform",
      "the traffic verdict is what proves it",
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
      "can't check on this platform",
      "the traffic verdict is what proves it",
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
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
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

// --- What the lead sentence may say about this machine (E-ONBOARD.W4, F16) ---
// The headline used to state "everything below is still set up" whatever the
// probes said. It may state how much is left, and only where the probes answer.

describe("how much of this machine is still to do", () => {
  test("counts the two probes that are not green yet", () => {
    expect(stepsLeftHere("untrusted", "none")).toBe(2);
    expect(stepsLeftHere("trusted", "none")).toBe(1);
    expect(stepsLeftHere("untrusted", "ours")).toBe(1);
    expect(stepsLeftHere("trusted", "ours")).toBe(0);
  });

  test("counts a foreign proxy as a step left, because it is not routing here", () => {
    expect(stepsLeftHere("trusted", "foreign")).toBe(1);
  });

  test("refuses to count where a probe could not answer", () => {
    expect(stepsLeftHere("unknown", "ours")).toBeNull();
    expect(stepsLeftHere("trusted", "unknown")).toBeNull();
    expect(stepsLeftHere("unknown", "unknown")).toBeNull();
  });
});
