import { describe, expect, test } from "vitest";
import {
  awaiting,
  foreignProxyNotice,
  NO_TRAFFIC,
  proxyStatus,
  RECENT_MS,
  setUpOn,
  trafficStatus,
  trustStatus,
  TUNNEL_GRACE_MS,
  type StepStatusView,
  type Traffic,
} from "./verdicts";

// --- Two layers of truth (E-REDESIGN.W10, re-worded for its reader at W5) ---
// Tasks 1 and 2 are this machine, from the probes, and take no traffic input at
// all: a flow captured an hour ago cannot turn a task green, because there is
// no argument to hand it through. What changed at W5 is who the sentences are
// written for — the states now read as a person's job, not as a machine's
// report, and the honesty rule is unmoved.

const CONFIRM_ON = "we'll confirm this on your iPhone's first request";
const HERE = { device: "this Mac", confirmOn: "we'll confirm this on this Mac's first request" };

test("trustStatus: the local probe is the whole state, said as done or not done", () => {
  expect(trustStatus("trusted", null, CONFIRM_ON)).toMatchObject({
    glyph: "✓",
    tone: "ok",
    short: "done — installed and trusted",
  });
  expect(trustStatus("untrusted", null, CONFIRM_ON)).toMatchObject({
    glyph: "○",
    tone: "muted",
    short: "not done — the certificate isn't trusted",
  });
});

test("trustStatus: a store nothing read says when it will be settled, never that it is not", () => {
  const view = trustStatus("unknown", null, CONFIRM_ON);

  expect(view).toMatchObject({ glyph: "◐", tone: "muted", short: CONFIRM_ON });
  expect(view.short).not.toMatch(/not done|can't check/i);
});

test("trustStatus: a reading in progress names the reading; a write names what it waits on", () => {
  expect(trustStatus("trusted", "checking", CONFIRM_ON)).toMatchObject({
    glyph: "⟳",
    short: "reading the certificate store…",
  });
  expect(trustStatus("untrusted", "installing", CONFIRM_ON).short).toBe(
    "setting up — approve the prompt…",
  );
});

test("proxyStatus: the local probe is the whole state, and names the machine it is about", () => {
  expect(proxyStatus("ours", null, HERE)).toMatchObject({
    glyph: "✓",
    tone: "ok",
    short: "done — this Mac routes through Trawler",
  });
  expect(proxyStatus("none", null, HERE).short).toBe("not done — the system proxy is off");
});

test("proxyStatus: a foreign proxy is not done, and does not carry the sentence about itself", () => {
  const foreign = proxyStatus("foreign", null, HERE);

  expect(foreign).toMatchObject({ glyph: "○", tone: "muted", short: "not done — another proxy is set" });
  expect(foreign.remedy).toBeUndefined();
});

test("proxyStatus: not-set and nothing-read are distinct states, both calm", () => {
  expect(proxyStatus("unknown", null, HERE).short).toBe(HERE.confirmOn);
  expect(proxyStatus("unknown", null, HERE).short).not.toBe(proxyStatus("none", null, HERE).short);
});

test("proxyStatus: a reading in progress says which reading", () => {
  expect(proxyStatus("ours", "checking", HERE)).toMatchObject({
    glyph: "⟳",
    short: "reading the proxy settings…",
  });
  expect(proxyStatus("none", "setting", HERE).short).toBe("setting up — approve the prompt…");
});

// The button is about to replace something the user set, and the sentence over
// it is the only place they can see which (F17).
describe("the proxy already set on this machine", () => {
  test("is named, with what replacing it costs", () => {
    expect(foreignProxyNotice("this Mac", "10.1.1.1:8080")).toBe(
      "another proxy is set on this Mac: 10.1.1.1:8080. Setting Trawler replaces it while Trawler runs, and restores nothing when it quits.",
    );
  });

  test("drops the address where the probe read none, and claims nothing in its place", () => {
    const notice = foreignProxyNotice("this Mac", null);

    expect(notice).toBe(
      "another proxy is set on this Mac. Setting Trawler replaces it while Trawler runs, and restores nothing when it quits.",
    );
    expect(notice).not.toMatch(/null|undefined|:/);
  });
});

test("the end of setup names this machine, and claims nothing where it cannot", () => {
  expect(setUpOn("this Mac")).toBe("You're set up — Trawler is capturing traffic from this Mac");
  expect(setUpOn(null)).toBe("You're set up — Trawler is capturing traffic");
});

// Task 3 is the traffic layer, judged against a clock. The complaint that
// produced it: a flow captured earlier kept the screen green after the proxy
// had been turned off, because "seen" had no time attached.
describe("trafficStatus reads recency, never history", () => {
  const NOW = 1_000_000;
  const ago = (ms: number) => NOW - ms;
  const at = (seen: Partial<Traffic>) => trafficStatus({ ...NO_TRAFFIC, ...seen }, NOW);

  it("nothing ever: waiting for the first request", () => {
    expect(at({})).toMatchObject({
      glyph: "○",
      tone: "muted",
      short: "waiting for the first request",
    });
  });

  it("a decrypted request inside the window is capturing", () => {
    expect(at({ lastDecryptedAt: ago(RECENT_MS) })).toMatchObject({
      glyph: "✓",
      tone: "ok",
      short: "capturing traffic",
    });
  });

  it("a decrypted request past the window is quiet again — old evidence is not current", () => {
    const v = at({ lastDecryptedAt: ago(RECENT_MS + 1) });
    expect(v).toMatchObject({ glyph: "○", tone: "muted", short: "nothing arriving right now" });
    expect(v.remedy).toBeUndefined();
  });

  it("a tally with no time can only ever mean quiet", () => {
    expect(at({ ever: true }).short).toBe("nothing arriving right now");
  });

  it("a tunnel inside its grace is neither proof nor failure", () => {
    expect(at({ lastTunnelAt: ago(TUNNEL_GRACE_MS - 1) })).toMatchObject({
      glyph: "⟳",
      short: "a connection just arrived…",
    });
  });

  it("a tunnel that no request answered says whose certificate is missing, naming no device", () => {
    const v = at({ lastTunnelAt: ago(TUNNEL_GRACE_MS) });
    expect(v).toMatchObject({ glyph: "⚠", tone: "warn", short: "arriving but not decrypting" });
    expect(v.remedy).toBe(
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
    );
    // F21: the app cannot attribute a tunnel to a platform - the design run's
    // own blocked-on row says so - and the screen's choice is the user's answer
    // to a different question.
    expect(v.remedy).not.toMatch(/platform/);
  });

  it("a tunnel answered by a decrypted request is capturing", () => {
    expect(at({ lastTunnelAt: ago(5_000), lastDecryptedAt: ago(4_000) }).short).toBe(
      "capturing traffic",
    );
  });

  it("positive evidence outranks a failing tunnel while both are recent", () => {
    expect(at({ lastDecryptedAt: ago(10_000), lastTunnelAt: ago(TUNNEL_GRACE_MS) }).tone).toBe("ok");
  });

  it("an unanswered tunnel past the window is quiet, not a standing alarm", () => {
    expect(at({ lastTunnelAt: ago(RECENT_MS + 1) }).short).toBe("nothing arriving right now");
  });

  it("plain HTTP arriving is its own state, and asks for the request that would prove it", () => {
    const v = at({ lastPlainAt: ago(1_000) });
    expect(v).toMatchObject({ glyph: "◐", tone: "muted", short: "arriving — plain HTTP only" });
    expect(v.remedy).toBe("open a website whose address starts with https://");
  });

  it("a failing tunnel outranks plain HTTP: the tunnel is the failure the plain one cannot excuse", () => {
    expect(at({ lastPlainAt: ago(1_000), lastTunnelAt: ago(5_000) }).tone).toBe("warn");
  });
});

// Every state the three resolvers can return, asserted literally against the
// copy table of the archived design run. The screen may not shorten a domain
// message (StatusLine @avoid), so the wording is this module's contract — a
// drift here is a design defect, not a detail.
describe("the short form every state carries", () => {
  const NOW = 1_000_000;
  const seen = (over: Partial<Traffic>): Traffic => ({ ...NO_TRAFFIC, ...over });

  const STATES: [string, StepStatusView, string, string | undefined][] = [
    [
      "trust · reading",
      trustStatus("trusted", "checking", CONFIRM_ON),
      "reading the certificate store…",
      undefined,
    ],
    [
      "trust · setting up",
      trustStatus("unknown", "installing", CONFIRM_ON),
      "setting up — approve the prompt…",
      undefined,
    ],
    ["trust · done", trustStatus("trusted", null, CONFIRM_ON), "done — installed and trusted", undefined],
    [
      "trust · not done",
      trustStatus("untrusted", null, CONFIRM_ON),
      "not done — the certificate isn't trusted",
      undefined,
    ],
    ["trust · nothing read it", trustStatus("unknown", null, CONFIRM_ON), CONFIRM_ON, undefined],
    [
      "proxy · reading",
      proxyStatus("ours", "checking", HERE),
      "reading the proxy settings…",
      undefined,
    ],
    [
      "proxy · setting up",
      proxyStatus("unknown", "setting", HERE),
      "setting up — approve the prompt…",
      undefined,
    ],
    ["proxy · done", proxyStatus("ours", null, HERE), "done — this Mac routes through Trawler", undefined],
    ["proxy · foreign", proxyStatus("foreign", null, HERE), "not done — another proxy is set", undefined],
    ["proxy · nothing read it", proxyStatus("unknown", null, HERE), HERE.confirmOn, undefined],
    ["proxy · off", proxyStatus("none", null, HERE), "not done — the system proxy is off", undefined],
    [
      "traffic · capturing",
      trafficStatus(seen({ lastDecryptedAt: NOW }), NOW),
      "capturing traffic",
      undefined,
    ],
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
      "open a website whose address starts with https://",
    ],
    ["traffic · quiet", trafficStatus(seen({ ever: true }), NOW), "nothing arriving right now", undefined],
    ["traffic · waiting", trafficStatus(NO_TRAFFIC, NOW), "waiting for the first request", undefined],
  ];

  it("covers every state the three resolvers can return", () => {
    expect(STATES).toHaveLength(17);
  });

  it.each(STATES)("%s", (_name, view, short, remedy) => {
    expect(view.short).toBe(short);
    expect(view.remedy).toBe(remedy);
  });

  // StatusLine's ~40-char contract holds for every verdict the resolvers word.
  // The one state above it is `awaiting`, whose text is a promise the design
  // wrote out in full — a fragment would have to drop either the device or the
  // moment, and both are the point of the sentence.
  it.each(STATES)("%s fits StatusLine's ~40-char contract", (_name, view) => {
    if (view.short.startsWith("we'll confirm")) return;
    expect(view.short.length).toBeLessThanOrEqual(40);
  });

  it("a state carries a reading and nothing else — the instruction is a separate field", () => {
    const arriving = trafficStatus(seen({ lastTunnelAt: NOW - TUNNEL_GRACE_MS }), NOW);

    expect(arriving.short).toBe("arriving but not decrypting");
    expect(arriving.short).not.toMatch(/Finish the certificate step/);
    expect(arriving.remedy).toMatch(/Finish the certificate step/);
  });

  it("no state is a claim about a machine nothing read", () => {
    expect(awaiting(CONFIRM_ON)).toMatchObject({ glyph: "◐", tone: "muted", short: CONFIRM_ON });
    expect(awaiting(CONFIRM_ON).short).not.toMatch(/done/);
  });
});
