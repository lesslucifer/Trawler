import { describe, expect, test } from "vitest";
import {
  awaiting,
  byOwnRequest,
  clock,
  evidenceOf,
  foreignProxyNotice,
  notDecryptingHere,
  routeByTraffic,
  NO_TRAFFIC,
  provenDevice,
  proxyStatus,
  RECENT_MS,
  setUpOn,
  stopRoutingNote,
  trafficStatus,
  trustByTraffic,
  trustStatus,
  STATE_WORDS,
  TUNNEL_GRACE_MS,
  verdict,
  type SetupState,
  type StepStatusView,
  type Traffic,
} from "./verdicts";

const STATE_OF: Record<string, SetupState> = {
  done: "done",
  "not done": "not-done",
  "not working": "not-working",
  "can't check here": "cant-check",
  checking: "checking",
  "setting up": "setting-up",
};

describe("the closed vocabulary of a task's state (F5)", () => {
  it.each([
    ["done", "✓", "ok", "done"],
    ["not-done", "○", "muted", "not done"],
    ["not-working", "⚠", "warn", "not working"],
    ["cant-check", "◐", "muted", "can't check here"],
    ["checking", "⟳", "muted", "checking"],
    ["setting-up", "⟳", "muted", "setting up"],
  ] as const)("%s is %s, %s, and reads %s", (state, glyph, tone, word) => {
    expect(verdict(state)).toEqual({ glyph, tone, short: word });
    expect(verdict(state, "a reason").short).toBe(`${word} — a reason`);
  });

  it("a word is owned by one state, and a remedy rides beside the line, never in it", () => {
    expect(new Set(STATE_WORDS).size).toBe(STATE_WORDS.length);
    expect(verdict("not-done", "a reason", "do this")).toEqual({
      glyph: "○",
      tone: "muted",
      short: "not done — a reason",
      remedy: "do this",
    });
  });

  it("only a failure is warn: not done and not working are two different states", () => {
    expect(verdict("not-done").tone).toBe("muted");
    expect(verdict("not-working").tone).toBe("warn");
  });
});

// --- Two layers of truth (E-REDESIGN.W10, re-worded for its reader at W5) ---
// Tasks 1 and 2 are this machine, from the probes, and take no traffic input at
// all: a flow captured an hour ago cannot turn a task green, because there is
// no argument to hand it through. What changed at W5 is who the sentences are
// written for — the states now read as a person's job, not as a machine's
// report, and the honesty rule is unmoved.

const CONFIRM_ON = "your iPhone's first request confirms it";
const UNREAD = awaiting(CONFIRM_ON);
const HERE = { device: "this Mac", unread: awaiting("this Mac's first request confirms it") };
const CANT_CHECK = (confirmOn: string) => `can't check here — ${confirmOn}`;

test("trustStatus: the local probe is the whole state, said as done or not done", () => {
  expect(trustStatus("trusted", null, UNREAD)).toMatchObject({
    glyph: "✓",
    tone: "ok",
    short: "done — installed and trusted",
  });
  expect(trustStatus("absent", null, UNREAD)).toMatchObject({
    glyph: "○",
    tone: "muted",
    short: "not done — not installed",
  });
});

// D2: a copy in the keychain nobody marked Always Trust is its own answer.
test("trustStatus: a certificate installed and not trusted says so, apart from one not installed", () => {
  expect(trustStatus("installed", null, UNREAD)).toMatchObject({
    glyph: "○",
    tone: "muted",
    short: "not done — installed, not trusted",
  });
  expect(trustStatus("installed", null, UNREAD).short).not.toBe(trustStatus("absent", null, UNREAD).short);
});

test("trustStatus: a store nothing read reads what the caller names for it, never not done", () => {
  const view = trustStatus("unknown", null, UNREAD);

  expect(view).toEqual(UNREAD);
  expect(view.short).not.toMatch(/not done/i);
});

test("trustStatus: a reading in progress names the reading; a write only says it runs", () => {
  expect(trustStatus("trusted", "checking", UNREAD)).toMatchObject({
    glyph: "⟳",
    short: "checking — the certificate store",
  });
  expect(trustStatus("absent", "installing", UNREAD).short).toBe(
    "setting up",
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
  expect(proxyStatus("unknown", null, HERE)).toEqual(HERE.unread);
  expect(proxyStatus("unknown", null, HERE).short).not.toBe(proxyStatus("none", null, HERE).short);
});

test("proxyStatus: a reading in progress says which reading", () => {
  expect(proxyStatus("ours", "checking", HERE)).toMatchObject({
    glyph: "⟳",
    short: "checking — the proxy settings",
  });
  expect(proxyStatus("none", "setting", HERE).short).toBe("setting up");
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

// R17: a decrypted request is credited to this machine only where the two
// readings beside it say it could have come from here.
test.each([
  ["trusted", "ours", "this Mac"],
  ["trusted", "none", null],
  ["trusted", "foreign", null],
  ["trusted", "unknown", null],
  ["absent", "ours", null],
  ["unknown", "ours", null],
  ["absent", "none", null],
] as const)("the end names this machine for trust %s and proxy %s: %s", (trust, proxy, named) => {
  expect(provenDevice("this Mac", trust, proxy)).toBe(named);
});

// R14: the reversal names only what is installed.
test("the reversal's line names the certificate only where it reads trusted", () => {
  expect(stopRoutingNote(true)).toBe(
    "this turns the proxy off and nothing else — Trawler's certificate stays trusted, and step 1 has the steps to remove it.",
  );
  expect(stopRoutingNote(false)).toBe("this turns the proxy off and nothing else.");
  expect(stopRoutingNote(false)).not.toMatch(/certificate/);
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
      short: "not done — waiting for the first request",
    });
  });

  it("a decrypted request inside the window is capturing", () => {
    expect(at({ lastDecryptedAt: ago(RECENT_MS) })).toMatchObject({
      glyph: "✓",
      tone: "ok",
      short: "done — capturing traffic",
    });
  });

  it("a decrypted request past the window is quiet again — old evidence is not current", () => {
    const v = at({ lastDecryptedAt: ago(RECENT_MS + 1) });
    expect(v).toMatchObject({ glyph: "○", tone: "muted", short: "not done — nothing arriving right now" });
    expect(v.remedy).toBeUndefined();
  });

  it("anything this session saw, past the window, is quiet rather than waiting", () => {
    expect(at({ lastPlainAt: ago(RECENT_MS + 1) }).short).toBe("not done — nothing arriving right now");
    expect(at({ lastTunnelAt: ago(RECENT_MS + 1) }).short).toBe("not done — nothing arriving right now");
  });

  it("a tunnel inside its grace is neither proof nor failure", () => {
    expect(at({ lastTunnelAt: ago(TUNNEL_GRACE_MS - 1) })).toMatchObject({
      glyph: "⟳",
      short: "checking — a connection just arrived",
    });
  });

  it("a tunnel that no request answered says whose certificate is missing, naming no device", () => {
    const v = at({ lastTunnelAt: ago(TUNNEL_GRACE_MS) });
    expect(v).toMatchObject({ glyph: "⚠", tone: "warn", short: "not working — arriving but not decrypting" });
    expect(v.remedy).toBe(
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
    );
    // F21: the app cannot attribute a tunnel to a platform - the design run's
    // own blocked-on row says so - and the screen's choice is the user's answer
    // to a different question.
    expect(v.remedy).not.toMatch(/platform/);
  });

  // R18: the remedy defers to a certificate probe that reads trusted.
  it("a tunnel beside a trusted certificate sends no one back to the certificate step", () => {
    const v = trafficStatus({ ...NO_TRAFFIC, lastTunnelAt: ago(TUNNEL_GRACE_MS) }, NOW, {
      ownsRefusal: false,
      trustedOn: "this Mac",
    });
    expect(v).toMatchObject({ glyph: "⚠", tone: "warn", short: "not working — arriving but not decrypting" });
    expect(v.remedy).toBe(
      "something is connecting and none of it is decrypting — this Mac trusts Trawler's certificate, so it comes from another device, or from an app with a certificate store of its own, that does not trust it yet.",
    );
    expect(v.remedy).not.toMatch(/Finish the certificate step/);
  });

  it("a trusted certificate changes no other reading's words", () => {
    const quiet = trafficStatus({ ...NO_TRAFFIC, lastPlainAt: ago(1) }, NOW, {
      ownsRefusal: false,
      trustedOn: "this Mac",
    });
    expect(quiet).toEqual(at({ lastPlainAt: ago(1) }));
  });

  it("a tunnel answered by a decrypted request is capturing", () => {
    expect(at({ lastTunnelAt: ago(5_000), lastDecryptedAt: ago(4_000) }).short).toBe(
      "done — capturing traffic",
    );
  });

  it("positive evidence outranks a failing tunnel while both are recent", () => {
    expect(at({ lastDecryptedAt: ago(10_000), lastTunnelAt: ago(TUNNEL_GRACE_MS) }).tone).toBe("ok");
  });

  it("an unanswered tunnel past the window is quiet, not a standing alarm", () => {
    expect(at({ lastTunnelAt: ago(RECENT_MS + 1) }).short).toBe("not done — nothing arriving right now");
  });

  it("plain HTTP arriving is not done, and asks for the request that would prove it", () => {
    const v = at({ lastPlainAt: ago(1_000) });
    expect(v).toMatchObject({ glyph: "○", tone: "muted", short: "not done — only plain http so far" });
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
const AT_1402 = new Date(2026, 8, 30, 14, 2, 37).getTime();

describe("the short form every state carries", () => {
  const NOW = 1_000_000;
  const seen = (over: Partial<Traffic>): Traffic => ({ ...NO_TRAFFIC, ...over });

  const STATES: [string, StepStatusView, string, string | undefined][] = [
    [
      "trust · reading",
      trustStatus("trusted", "checking", UNREAD),
      "checking — the certificate store",
      undefined,
    ],
    [
      "trust · setting up",
      trustStatus("unknown", "installing", UNREAD),
      "setting up",
      undefined,
    ],
    ["trust · done", trustStatus("trusted", null, UNREAD), "done — installed and trusted", undefined],
    [
      "trust · not done",
      trustStatus("absent", null, UNREAD),
      "not done — not installed",
      undefined,
    ],
    ["trust · nothing read it", trustStatus("unknown", null, UNREAD), CANT_CHECK(CONFIRM_ON), undefined],
    [
      "trust · installed, not trusted",
      trustStatus("installed", null, UNREAD),
      "not done — installed, not trusted",
      undefined,
    ],
    [
      "trust · this machine, awaiting its own request",
      trustStatus("unknown", null, byOwnRequest("trust", null)),
      "can't check here — the request in step 3 confirms it",
      undefined,
    ],
    [
      "trust · this machine, its own request decrypted",
      trustStatus("unknown", null, byOwnRequest("trust", AT_1402)),
      "done — step 3's request decrypted at 14:02",
      undefined,
    ],
    [
      "proxy · reading",
      proxyStatus("ours", "checking", HERE),
      "checking — the proxy settings",
      undefined,
    ],
    [
      "proxy · setting up",
      proxyStatus("unknown", "setting", HERE),
      "setting up",
      undefined,
    ],
    ["proxy · done", proxyStatus("ours", null, HERE), "done — this Mac routes through Trawler", undefined],
    ["proxy · foreign", proxyStatus("foreign", null, HERE), "not done — another proxy is set", undefined],
    ["proxy · nothing read it", proxyStatus("unknown", null, HERE), HERE.unread.short, undefined],
    ["proxy · off", proxyStatus("none", null, HERE), "not done — the system proxy is off", undefined],
    [
      "proxy · this machine, awaiting its own request",
      proxyStatus("unknown", null, { device: "this machine", unread: byOwnRequest("route", null) }),
      "can't check here — the same request confirms it",
      undefined,
    ],
    [
      "proxy · this machine, its own request came through",
      proxyStatus("unknown", null, { device: "this machine", unread: byOwnRequest("route", AT_1402) }),
      "done — step 3's request came through at 14:02",
      undefined,
    ],
    [
      "traffic · capturing",
      trafficStatus(seen({ lastDecryptedAt: NOW }), NOW),
      "done — capturing traffic",
      undefined,
    ],
    [
      "traffic · tunnel in grace",
      trafficStatus(seen({ lastTunnelAt: NOW }), NOW),
      "checking — a connection just arrived",
      undefined,
    ],
    [
      "traffic · tunnel unanswered",
      trafficStatus(seen({ lastTunnelAt: NOW - TUNNEL_GRACE_MS }), NOW),
      "not working — arriving but not decrypting",
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
    ],
    [
      "traffic · plain only",
      trafficStatus(seen({ lastPlainAt: NOW }), NOW),
      "not done — only plain http so far",
      "open a website whose address starts with https://",
    ],
    [
      "traffic · quiet",
      trafficStatus(seen({ lastPlainAt: NOW - RECENT_MS - 1 }), NOW),
      "not done — nothing arriving right now",
      undefined,
    ],
    ["traffic · waiting", trafficStatus(NO_TRAFFIC, NOW), "not done — waiting for the first request", undefined],
  ];

  it("covers every state the three resolvers can return", () => {
    expect(STATES).toHaveLength(22);
  });

  it.each(STATES)("%s", (_name, view, short, remedy) => {
    expect(view.short).toBe(short);
    expect(view.remedy).toBe(remedy);
  });

  // StatusLine's ~40-char contract holds for every reason the resolvers word.
  // The one reason above it is `awaiting`'s, a promise the design wrote out in
  // full — a fragment would have to drop either the device or the moment, and
  // both are the point of the sentence.
  it.each(STATES)("%s fits StatusLine's ~40-char contract", (_name, view) => {
    const reason = view.short.split(" — ")[1] ?? "";
    if (view.glyph === "◐") return;
    expect(reason.length).toBeLessThanOrEqual(40);
  });

  // F5: the state is read from its first word, so every line opens with one of
  // the closed vocabulary's, and one glyph never carries two of them.
  it.each(STATES)("%s opens with a state word its glyph owns", (_name, view) => {
    const word = STATE_WORDS.find((w) => view.short === w || view.short.startsWith(`${w} — `));
    expect(word).toBeDefined();
    expect(verdict(STATE_OF[word!]!).glyph).toBe(view.glyph);
  });

  it("a state carries a reading and nothing else — the instruction is a separate field", () => {
    const arriving = trafficStatus(seen({ lastTunnelAt: NOW - TUNNEL_GRACE_MS }), NOW);

    expect(arriving.short).toBe("not working — arriving but not decrypting");
    expect(arriving.short).not.toMatch(/Finish the certificate step/);
    expect(arriving.remedy).toMatch(/Finish the certificate step/);
  });

  it("no state is a claim about a machine nothing read", () => {
    expect(awaiting(CONFIRM_ON)).toMatchObject({
      glyph: "◐",
      tone: "muted",
      short: CANT_CHECK(CONFIRM_ON),
    });
    expect(awaiting(CONFIRM_ON).short).not.toMatch(/done/);
  });
});

// ── Evidence: where no probe concludes, the traffic of this setup is the reading ──

describe("evidence is what this setup's traffic has shown, not what it shows now", () => {
  const NOW = 1_000_000;
  const ago = (ms: number) => NOW - ms;
  const of = (seen: Partial<Traffic>) => evidenceOf({ ...NO_TRAFFIC, ...seen }, NOW);

  it("nothing yet is no evidence", () => {
    expect(of({})).toEqual({ reached: false, decrypted: false, unanswered: false, refused: false });
  });

  it("any request that reached Trawler shows the route, however long ago", () => {
    expect(of({ lastPlainAt: ago(RECENT_MS * 10) }).reached).toBe(true);
    expect(of({ lastTunnelAt: ago(0) }).reached).toBe(true);
    expect(of({ lastDecryptedAt: ago(RECENT_MS * 10) }).reached).toBe(true);
  });

  it("a tunnel inside its grace is not yet a refusal", () => {
    expect(of({ lastTunnelAt: ago(TUNNEL_GRACE_MS - 1) })).toMatchObject({
      unanswered: false,
      refused: false,
    });
  });

  it("a tunnel past its grace with nothing decrypted is a refusal, and stays one", () => {
    expect(of({ lastTunnelAt: ago(TUNNEL_GRACE_MS) }).refused).toBe(true);
    expect(of({ lastTunnelAt: ago(RECENT_MS * 10) }).refused).toBe(true);
  });

  it("one decrypted request outranks a later refusal - the certificate works somewhere", () => {
    const mixed = of({ lastDecryptedAt: ago(RECENT_MS * 2), lastTunnelAt: ago(TUNNEL_GRACE_MS) });
    expect(mixed).toMatchObject({ decrypted: true, unanswered: true, refused: false });
  });

  it("a tunnel answered by a later decrypted request is not left unanswered", () => {
    expect(of({ lastTunnelAt: ago(10_000), lastDecryptedAt: ago(9_000) }).unanswered).toBe(false);
  });
});

describe("a task no probe concludes reads its evidence", () => {
  const CONFIRM_HTTPS = "your iPhone's first HTTPS request confirms it";
  const ev = (e: Partial<ReturnType<typeof evidenceOf>>) => ({
    reached: false,
    decrypted: false,
    unanswered: false,
    refused: false,
    ...e,
  });

  it("the certificate: decrypted is done, refused is not done, nothing is a promise", () => {
    expect(trustByTraffic(ev({ decrypted: true }), CONFIRM_HTTPS)).toMatchObject({
      glyph: "✓",
      tone: "ok",
      short: "done — a request decrypted",
    });
    expect(trustByTraffic(ev({ refused: true }), CONFIRM_HTTPS)).toMatchObject({
      glyph: "⚠",
      tone: "warn",
      short: "not working — nothing decrypted",
    });
    expect(trustByTraffic(ev({}), CONFIRM_HTTPS)).toEqual(awaiting(CONFIRM_HTTPS));
  });

  it("a probe that concludes is never overruled by what stands in for it (DEC-33)", () => {
    expect(trustStatus("absent", null, trustByTraffic(ev({ decrypted: true }), CONFIRM_HTTPS)).short).toBe(
      "not done — not installed",
    );
    expect(trustStatus("trusted", null, trustByTraffic(ev({ refused: true }), CONFIRM_HTTPS)).short).toBe(
      "done — installed and trusted",
    );
    expect(trustStatus("installed", null, byOwnRequest("trust", 1_000)).short).toBe(
      "not done — installed, not trusted",
    );
    expect(proxyStatus("none", null, { device: "this machine", unread: byOwnRequest("route", 1_000) }).short).toBe(
      "not done — the system proxy is off",
    );
  });

  it("the route: any request reaching Trawler is done, nothing yet is a promise", () => {
    expect(routeByTraffic(ev({ reached: true }), CONFIRM_ON)).toMatchObject({
      glyph: "✓",
      short: "done — requests reached Trawler",
    });
    expect(routeByTraffic(ev({}), CONFIRM_ON)).toEqual(awaiting(CONFIRM_ON));
  });

  it("a refusal said in the certificate task names whose store, points at its steps, then gives the check to make", () => {
    expect(notDecryptingHere("your iPhone", "Check the switch.")).toBe(
      "something reached Trawler and did not decrypt — your iPhone does not trust Trawler's certificate yet. The steps below install it. Check the switch.",
    );
  });

  it("the try task leaves a refusal to a certificate task that is not done", () => {
    const refused = { ...NO_TRAFFIC, lastTunnelAt: 1_000_000 - TUNNEL_GRACE_MS };
    const v = trafficStatus(refused, 1_000_000, { ownsRefusal: true, trustedOn: null });
    expect(v.short).toBe("not working — arriving but not decrypting");
    expect(v.remedy).toBeUndefined();
  });
});

// ── This machine, where no probe answers: its own request (DEC-62) ─────────

describe("This machine's own request stands in for a probe that cannot answer", () => {
  it("says the time to the minute, 24-hour, in the reader's zone", () => {
    expect(clock(AT_1402)).toBe("14:02");
    expect(clock(new Date(2026, 8, 30, 9, 5).getTime())).toBe("09:05");
  });

  it("names the request that settles each task until it has, and never one sentence twice", () => {
    const trust = byOwnRequest("trust", null);
    const route = byOwnRequest("route", null);

    expect(trust).toEqual(awaiting("the request in step 3 confirms it"));
    expect(route).toEqual(awaiting("the same request confirms it"));
    expect(trust.short).not.toBe(route.short);
  });

  it("once it came through decrypted, both tasks are done with the time it did", () => {
    expect(byOwnRequest("trust", AT_1402)).toEqual(verdict("done", "step 3's request decrypted at 14:02"));
    expect(byOwnRequest("route", AT_1402)).toEqual(verdict("done", "step 3's request came through at 14:02"));
  });
});
