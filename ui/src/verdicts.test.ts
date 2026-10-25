import { describe, expect, test } from "vitest";
import {
  addressRefusedNow,
  addressTrustStatus,
  awaiting,
  byOwnRequest,
  clock,
  deviceRouteStatus,
  deviceTrustStatus,
  emulatorRefusal,
  emulatorRouteStatus,
  emulatorsRouted,
  emulatorTrustStatus,
  evidenceOf,
  foreignProxyNotice,
  lastReachedAt,
  markedDevice,
  mayCarryOwnTraffic,
  mixedTryStatus,
  notDecryptingHere,
  routeByTraffic,
  NO_EVIDENCE,
  NO_TRAFFIC,
  provenDevice,
  proxyStatus,
  RECENT_MS,
  REFUSED_AFTER_PROOF,
  refusedAddress,
  setUpOn,
  storesTrusted,
  simulatorTrustStatus,
  stopRoutingNote,
  trafficStatus,
  trustByTraffic,
  trustedEmulators,
  trustedSimulators,
  trustStatus,
  STATE_WORDS,
  TUNNEL_GRACE_MS,
  verdict,
  type EmulatorReading,
  type SetupState,
  type SimulatorTrust,
  type StepStatusView,
  type Traffic,
  type TrafficContext,
} from "./verdicts";

// One tunnel that no decrypted request has followed.
const openTunnel = (at: number) => ({ lastTunnelAt: at, openTunnelAt: at });

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
    expect(at(openTunnel(ago(RECENT_MS + 1))).short).toBe("not done — nothing arriving right now");
  });

  it("a tunnel inside its grace is neither proof nor failure", () => {
    expect(at(openTunnel(ago(TUNNEL_GRACE_MS - 1)))).toMatchObject({
      glyph: "⟳",
      short: "checking — a connection just arrived",
    });
  });

  it("a tunnel that no request answered says whose certificate is missing, naming no device", () => {
    const v = at(openTunnel(ago(TUNNEL_GRACE_MS)));
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
    const v = trafficStatus({ ...NO_TRAFFIC, ...openTunnel(ago(TUNNEL_GRACE_MS)) }, NOW, {
      ownsRefusal: false,
      trustedOn: "this Mac",
      proofStays: "never",
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
      proofStays: "never",
    });
    expect(quiet).toEqual(at({ lastPlainAt: ago(1) }));
  });

  it("a tunnel answered by a decrypted request is capturing", () => {
    expect(at({ lastTunnelAt: ago(5_000), lastDecryptedAt: ago(4_000) }).short).toBe(
      "done — capturing traffic",
    );
  });

  it("positive evidence outranks a failing tunnel while both are recent", () => {
    expect(at({ lastDecryptedAt: ago(10_000), ...openTunnel(ago(TUNNEL_GRACE_MS)) }).tone).toBe("ok");
  });

  it("an unanswered tunnel past the window is quiet, not a standing alarm", () => {
    expect(at(openTunnel(ago(RECENT_MS + 1))).short).toBe("not done — nothing arriving right now");
  });

  it("plain HTTP arriving is not done, and asks for the request that would prove it", () => {
    const v = at({ lastPlainAt: ago(1_000) });
    expect(v).toMatchObject({ glyph: "○", tone: "muted", short: "not done — only plain http so far" });
    expect(v.remedy).toBe("open a website whose address starts with https://");
  });

  it("a client retrying a refused tunnel keeps reading not decrypting, never a connection that just arrived", () => {
    expect(at({ lastTunnelAt: ago(1_000), openTunnelAt: ago(5_000) }).short).toBe(
      "not working — arriving but not decrypting",
    );
  });

  it("a failing tunnel outranks plain HTTP: the tunnel is the failure the plain one cannot excuse", () => {
    expect(at({ lastPlainAt: ago(1_000), ...openTunnel(ago(5_000)) }).tone).toBe("warn");
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
      trustStatus("unknown", null, byOwnRequest("trust", null, 3)),
      "can't check here — the request in step 3 confirms it",
      undefined,
    ],
    [
      "trust · this machine, its own request decrypted",
      trustStatus("unknown", null, byOwnRequest("trust", AT_1402, 3)),
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
      proxyStatus("unknown", null, { device: "this machine", unread: byOwnRequest("route", null, 3) }),
      "can't check here — the same request confirms it",
      undefined,
    ],
    [
      "proxy · this machine, its own request came through",
      proxyStatus("unknown", null, { device: "this machine", unread: byOwnRequest("route", AT_1402, 3) }),
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
      trafficStatus(seen(openTunnel(NOW)), NOW),
      "checking — a connection just arrived",
      undefined,
    ],
    [
      "traffic · tunnel unanswered",
      trafficStatus(seen(openTunnel(NOW - TUNNEL_GRACE_MS)), NOW),
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
    [
      "traffic · quiet after its proof, on This machine",
      trafficStatus(seen({ lastDecryptedAt: AT_1402 }), AT_1402 + RECENT_MS + 1, {
        ownsRefusal: false,
        trustedOn: null,
        proofStays: "once-quiet",
      }),
      "done — decrypted at 14:02, nothing since",
      undefined,
    ],
  ];

  it("covers every state the three resolvers can return", () => {
    expect(STATES).toHaveLength(23);
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
    const arriving = trafficStatus(seen(openTunnel(NOW - TUNNEL_GRACE_MS)), NOW);

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
    expect(of({})).toEqual(NO_EVIDENCE);
  });

  it("any request that reached Trawler shows the route, however long ago", () => {
    expect(of({ lastPlainAt: ago(RECENT_MS * 10) }).reached).toBe(true);
    expect(of(openTunnel(ago(0))).reached).toBe(true);
    expect(of({ lastDecryptedAt: ago(RECENT_MS * 10) }).reached).toBe(true);
  });

  it("a tunnel inside its grace is not yet a refusal", () => {
    expect(of(openTunnel(ago(TUNNEL_GRACE_MS - 1)))).toMatchObject({
      unanswered: false,
      refused: false,
    });
  });

  it("a tunnel past its grace with nothing decrypted is a refusal, and stays one", () => {
    expect(of(openTunnel(ago(TUNNEL_GRACE_MS))).refused).toBe(true);
    expect(of(openTunnel(ago(RECENT_MS * 10))).refused).toBe(true);
  });

  it("one decrypted request outranks a later refusal - the certificate works somewhere", () => {
    const mixed = of({ lastDecryptedAt: ago(RECENT_MS * 2), ...openTunnel(ago(TUNNEL_GRACE_MS)) });
    expect(mixed).toMatchObject({ decrypted: true, unanswered: true, refused: false });
  });

  it("a tunnel answered by a later decrypted request is not left unanswered", () => {
    expect(of({ lastTunnelAt: ago(10_000), lastDecryptedAt: ago(9_000) }).unanswered).toBe(false);
  });

  // Recorded on the owner's Mac: Chrome retried a refused tunnel every 5 s, and
  // a grace timed from the newest one took the refusal back for 3 s of each 5.
  it("a newer tunnel inside its grace never takes back a refusal the one before it made", () => {
    const retrying = { lastTunnelAt: ago(1_000), openTunnelAt: ago(5_000) };
    expect(of(retrying)).toMatchObject({ unanswered: true, refused: true });
  });
});

describe("a task no probe concludes reads its evidence", () => {
  const CONFIRM_HTTPS = "your iPhone's first HTTPS request confirms it";
  const ev = (e: Partial<ReturnType<typeof evidenceOf>>) => ({ ...NO_EVIDENCE, ...e });

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
    expect(trustStatus("installed", null, byOwnRequest("trust", 1_000, 3)).short).toBe(
      "not done — installed, not trusted",
    );
    expect(proxyStatus("none", null, { device: "this machine", unread: byOwnRequest("route", 1_000, 3) }).short).toBe(
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
    const refused = { ...NO_TRAFFIC, ...openTunnel(1_000_000 - TUNNEL_GRACE_MS) };
    const v = trafficStatus(refused, 1_000_000, { ownsRefusal: true, trustedOn: null, proofStays: "never" });
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
    const trust = byOwnRequest("trust", null, 3);
    const route = byOwnRequest("route", null, 3);

    expect(trust).toEqual(awaiting("the request in step 3 confirms it"));
    expect(route).toEqual(awaiting("the same request confirms it"));
    expect(trust.short).not.toBe(route.short);
  });

  it("once it came through decrypted, both tasks are done with the time it did", () => {
    expect(byOwnRequest("trust", AT_1402, 3)).toEqual(verdict("done", "step 3's request decrypted at 14:02"));
    expect(byOwnRequest("route", AT_1402, 3)).toEqual(verdict("done", "step 3's request came through at 14:02"));
  });
});

// ── U5 · Q3: This machine's proof does not expire with the traffic window ──

describe("the try task once a decrypted request proved this setup", () => {
  const STAYS: TrafficContext = { ownsRefusal: false, trustedOn: null, proofStays: "once-quiet" };
  const quiet = AT_1402 + RECENT_MS + 1;

  it("stays done on This machine when traffic goes quiet, with the time it was proven", () => {
    expect(trafficStatus({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402 }, quiet, STAYS)).toEqual(
      verdict("done", "decrypted at 14:02, nothing since"),
    );
  });

  it("says nothing of since where something arrived after the proof", () => {
    const later = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, lastPlainAt: AT_1402 + 1_000 };
    expect(trafficStatus(later, quiet + 1_000, STAYS).short).toBe("done — decrypted at 14:02");
  });

  it("is quiet again anywhere the proof does not stay", () => {
    expect(trafficStatus({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402 }, quiet).short).toBe(
      "not done — nothing arriving right now",
    );
  });

  it("changes no reading while traffic is still arriving, or before any decrypt", () => {
    const capturing = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402 };
    expect(trafficStatus(capturing, AT_1402 + 1_000, STAYS).short).toBe("done — capturing traffic");
    const plainOnly = { ...NO_TRAFFIC, lastPlainAt: AT_1402 };
    expect(trafficStatus(plainOnly, quiet, STAYS).short).toBe("not done — nothing arriving right now");
    const refusing = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402 - RECENT_MS, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    expect(trafficStatus(refusing, quiet, STAYS).short).toBe("not working — arriving but not decrypting");
  });
});

// ── S1: another device's proof outlasts what follows it ────────────────────

describe("another device's try task once a decrypted request proved it", () => {
  const PAST: TrafficContext = { ownsRefusal: false, trustedOn: null, proofStays: "past-anything" };
  const quiet = AT_1402 + RECENT_MS + 1;

  it("stays done past a refusal, with a note that sends no one back to the certificate", () => {
    const refusing = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const v = trafficStatus(refusing, quiet, PAST);

    expect(v).toEqual(verdict("done", "decrypted at 14:02", REFUSED_AFTER_PROOF));
    expect(v.remedy).not.toMatch(/Finish the certificate step/);
  });

  it("stays done past plain http and a tunnel still in its grace, with no note", () => {
    const plain = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, lastPlainAt: quiet - 1_000 };
    const arriving = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, ...openTunnel(quiet - 1_000) };

    expect(trafficStatus(plain, quiet, PAST)).toEqual(verdict("done", "decrypted at 14:02"));
    expect(trafficStatus(arriving, quiet, PAST)).toEqual(verdict("done", "decrypted at 14:02"));
  });

  it("changes nothing while it captures, or before any decrypt", () => {
    expect(trafficStatus({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402 }, AT_1402 + 1_000, PAST).short).toBe(
      "done — capturing traffic",
    );
    const refused = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    expect(trafficStatus(refused, quiet, PAST).remedy).toMatch(/Finish the certificate step/);
  });

  it("says the platform's own account of the refusal where it has one (W6 S2)", () => {
    const refusing = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const plain = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, lastPlainAt: quiet - 1_000 };
    const own: TrafficContext = { ...PAST, refusedAfterProof: "a browser with a store of its own" };

    expect(trafficStatus(refusing, quiet, own)).toEqual(
      verdict("done", "decrypted at 14:02", "a browser with a store of its own"),
    );
    expect(trafficStatus(plain, quiet, own)).toEqual(verdict("done", "decrypted at 14:02"));
  });

  it("is This machine's once-quiet rule nowhere else: past a refusal, This machine still reads it", () => {
    const refusing = { ...NO_TRAFFIC, lastDecryptedAt: AT_1402, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const once: TrafficContext = { ...PAST, proofStays: "once-quiet" };
    expect(trafficStatus(refusing, quiet, once).short).toBe("not working — arriving but not decrypting");
  });
});

// ── Another device's evidence (DEC-63) ─────────────────────────────────────

describe("another device's two tasks read what could have come from it", () => {
  const ev = (e: Partial<ReturnType<typeof evidenceOf>>) => ({ ...NO_EVIDENCE, ...e });
  const THAT_MAC = { device: "that Mac", provedAt: null, mixed: false, tryStep: 4 };
  const FIRST = "that Mac's first request confirms it";
  const FIRST_HTTPS = "that Mac's first HTTPS request confirms it";

  it("its marked request settles both, named and timed, whatever else arrived (A1)", () => {
    for (const mixed of [false, true]) {
      const marked = { ...THAT_MAC, provedAt: AT_1402, mixed };
      expect(deviceTrustStatus(ev({ refused: true }), marked, FIRST_HTTPS)).toEqual(
        verdict("done", "that Mac's request decrypted at 14:02"),
      );
      expect(deviceRouteStatus(ev({}), marked, FIRST)).toEqual(
        verdict("done", "that Mac's request came through at 14:02"),
      );
    }
  });

  it("where this machine's traffic may be arriving too and nothing is that device's yet, each names what will", () => {
    const mixed = { ...THAT_MAC, mixed: true };
    const trust = deviceTrustStatus(NO_EVIDENCE, mixed, FIRST_HTTPS);
    const route = deviceRouteStatus(NO_EVIDENCE, mixed, FIRST);

    expect(trust).toEqual(awaiting("the address in step 4 confirms it"));
    expect(route).toEqual(awaiting("the same request confirms it"));
    expect(trust.short).not.toBe(route.short);
  });

  // Rule 4: a proof that was that device's when it arrived stays its.
  it("a proof already that device's outranks this machine's traffic arriving now", () => {
    const mixed = { ...THAT_MAC, mixed: true };
    const its = evidenceOf({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402 }, AT_1402);

    expect(deviceTrustStatus(its, mixed, FIRST_HTTPS)).toEqual(verdict("done", "a request decrypted at 14:02"));
    expect(deviceRouteStatus(its, mixed, FIRST)).toEqual(verdict("done", "a request reached Trawler at 14:02"));
  });

  // P2: nothing here can read that device again, so the proof says its age.
  it("elsewhere its traffic settles them with the time of the newest proof", () => {
    const plainLater = evidenceOf({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402, lastPlainAt: AT_1402 + 60_000 }, AT_1402);

    expect(deviceTrustStatus(plainLater, THAT_MAC, FIRST_HTTPS)).toEqual(verdict("done", "a request decrypted at 14:02"));
    expect(deviceRouteStatus(plainLater, THAT_MAC, FIRST)).toEqual(verdict("done", "a request reached Trawler at 14:03"));
  });

  it("and reads a refusal, or nothing yet, as any device's traffic does", () => {
    const refusing = evidenceOf({ ...NO_TRAFFIC, ...openTunnel(AT_1402) }, AT_1402 + TUNNEL_GRACE_MS);
    expect(deviceTrustStatus(refusing, THAT_MAC, FIRST_HTTPS).short).toBe("not working — nothing decrypted");
    expect(deviceTrustStatus(NO_EVIDENCE, THAT_MAC, FIRST_HTTPS)).toEqual(awaiting(FIRST_HTTPS));
    expect(deviceRouteStatus(NO_EVIDENCE, THAT_MAC, FIRST)).toEqual(awaiting(FIRST));
  });

  it("the end names the device only while its marked request is the newest proof (A4)", () => {
    const seen = (at: number) => ({ ...NO_TRAFFIC, lastDecryptedAt: at });

    expect(markedDevice("that Mac", AT_1402, seen(AT_1402))).toBe("that Mac");
    expect(markedDevice("that Mac", AT_1402, seen(AT_1402 + 1))).toBeNull();
    expect(markedDevice("that Mac", null, seen(AT_1402))).toBeNull();
  });
});

describe("another device's try task, while nothing arriving is told apart from this machine's", () => {
  const quiet = AT_1402 + RECENT_MS + 1;

  it("names the address until a proof is that device's, then says when", () => {
    expect(mixedTryStatus(NO_TRAFFIC, quiet, null)).toEqual(awaiting("the address below confirms it"));
    expect(mixedTryStatus(NO_TRAFFIC, quiet, AT_1402)).toEqual(verdict("done", "decrypted at 14:02"));
  });

  it("reads none of the wire's capture as its own", () => {
    const capturing = { ...NO_TRAFFIC, lastDecryptedAt: quiet - 1_000 };
    expect(mixedTryStatus(capturing, quiet, null).short).toBe("can't check here — the address below confirms it");
  });

  it("still says nothing decrypts, naming no device (A3)", () => {
    const refusing = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const v = mixedTryStatus(refusing, quiet, null);
    expect(v.short).toBe("not working — arriving but not decrypting");
    expect(v.remedy).toMatch(/the device it comes from does not trust/);
  });

  it("keeps a proof already that device's past the wire's refusal, as a note (S1)", () => {
    const refusing = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    expect(mixedTryStatus(refusing, quiet, AT_1402)).toEqual(
      verdict("done", "decrypted at 14:02", REFUSED_AFTER_PROOF),
    );
  });

  it("says the platform's own account of a refusal after the proof, and only after one (W6 S2)", () => {
    const refusing = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const own = "a browser with a store of its own";

    expect(mixedTryStatus(refusing, quiet, AT_1402, { refusedAfterProof: own })).toEqual(verdict("done", "decrypted at 14:02", own));
    expect(mixedTryStatus(refusing, quiet, null, { refusedAfterProof: own }).remedy).toMatch(/the device it comes from does not trust/);
  });

  // W7's S2: where a probe read the chosen device's store as trusting it, the
  // refusal is someone else's, and the remedy says so rather than sending
  // anyone to finish a certificate step that is done.
  it("names the store a probe read as trusted, so the refusal is someone else's", () => {
    const refusing = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS) };
    const v = mixedTryStatus(refusing, quiet, null, { trustedOn: "iPhone 17 Pro" });
    expect(v.short).toBe("not working — arriving but not decrypting");
    expect(v.remedy).toBe(
      "something is connecting and none of it is decrypting — iPhone 17 Pro trusts Trawler's certificate, so it comes from another device, or from an app with a certificate store of its own, that does not trust it yet.",
    );
    expect(mixedTryStatus(NO_TRAFFIC, quiet, null, { trustedOn: "iPhone 17 Pro" })).toEqual(
      awaiting("the address below confirms it"),
    );
  });
});

// ── The iPhone: its test is the address it is offered (W9) ─────────────────

describe("a device whose test is its address", () => {
  const ev = (e: Partial<ReturnType<typeof evidenceOf>>) => ({ ...NO_EVIDENCE, ...e });
  const IPHONE = { device: "your iPhone", provedAt: null, mixed: false, tryStep: 4 };
  const FIRST_HTTPS = "your iPhone's first HTTPS request confirms it";
  const AT_1410 = AT_1402 + 8 * 60_000;
  const address = (at: number, mine = true) => ({ host: "example.com", at, mine });
  const past = (at: number) => at + TUNNEL_GRACE_MS;

  it("its address refused is the certificate's only past its grace and inside a clear span", () => {
    expect(refusedAddress(address(AT_1402), NO_EVIDENCE, IPHONE, past(AT_1402))).toEqual(address(AT_1402));
    expect(refusedAddress(address(AT_1402), NO_EVIDENCE, IPHONE, past(AT_1402) - 1)).toBeNull();
    expect(refusedAddress(address(AT_1402, false), NO_EVIDENCE, IPHONE, past(AT_1402))).toBeNull();
    expect(refusedAddress(undefined, NO_EVIDENCE, IPHONE, past(AT_1402))).toBeNull();
  });

  // Q2: a proof older than the refusal showed a certificate since removed.
  it("and only where it is newer than every proof, marked or not", () => {
    const now = past(AT_1410);
    expect(refusedAddress(address(AT_1410), ev({ decryptedAt: AT_1402 }), IPHONE, now)).toEqual(address(AT_1410));
    expect(refusedAddress(address(AT_1402), ev({ decryptedAt: AT_1410 }), IPHONE, now)).toBeNull();
    expect(refusedAddress(address(AT_1402), NO_EVIDENCE, { ...IPHONE, provedAt: AT_1410 }, now)).toBeNull();
  });

  it("its certificate reads that refusal not working, with the proof it outlasted", () => {
    expect(addressTrustStatus(NO_EVIDENCE, IPHONE, address(AT_1402), FIRST_HTTPS)).toEqual(
      verdict("not-working", "example.com did not decrypt"),
    );
    expect(addressTrustStatus(ev({ decryptedAt: AT_1402 }), IPHONE, address(AT_1410), FIRST_HTTPS)).toEqual(
      verdict("not-working", "example.com did not decrypt at 14:10, after decrypting at 14:02"),
    );
  });

  // Q1: Apple's own services refuse whatever its certificate does.
  it("any other refusal before a proof points at the address, never at the certificate", () => {
    const refusing = ev({ reached: true, unanswered: true, refused: true });
    expect(addressTrustStatus(refusing, IPHONE, null, FIRST_HTTPS)).toEqual(
      awaiting("the address in step 4 confirms it"),
    );
  });

  it("everything else reads as another device's", () => {
    expect(addressTrustStatus(NO_EVIDENCE, IPHONE, null, FIRST_HTTPS)).toEqual(awaiting(FIRST_HTTPS));
    expect(addressTrustStatus(ev({ decryptedAt: AT_1402 }), IPHONE, null, FIRST_HTTPS)).toEqual(
      verdict("done", "a request decrypted at 14:02"),
    );
    expect(addressTrustStatus(NO_EVIDENCE, { ...IPHONE, mixed: true }, null, FIRST_HTTPS)).toEqual(
      awaiting("the address in step 4 confirms it"),
    );
  });

  it("the try task reads its address refused as the refusal now only past its grace and inside the window", () => {
    expect(addressRefusedNow(address(AT_1402), past(AT_1402))).toBe(true);
    expect(addressRefusedNow(address(AT_1402), past(AT_1402) - 1)).toBe(false);
    expect(addressRefusedNow(address(AT_1402), AT_1402 + RECENT_MS + 1)).toBe(false);
    expect(addressRefusedNow(undefined, past(AT_1402))).toBe(false);
  });

  // F19: the certificate task says a refusal it owns, so the try task does not.
  it("a refusal its certificate task owns leaves the try task bare, before a proof and after one", () => {
    const now = AT_1410 + TUNNEL_GRACE_MS;
    const refusing = { ...NO_TRAFFIC, ...openTunnel(AT_1410) };
    expect(mixedTryStatus(refusing, now, null, { ownsRefusal: true })).toEqual({
      glyph: "⚠",
      tone: "warn",
      short: "not working — arriving but not decrypting",
    });
    expect(mixedTryStatus(refusing, now, AT_1402, { ownsRefusal: true })).toEqual(verdict("done", "decrypted at 14:02"));
    const proved = { ...refusing, lastDecryptedAt: AT_1402 };
    expect(trafficStatus(proved, now, { ownsRefusal: true, trustedOn: null, proofStays: "past-anything" })).toEqual(
      verdict("done", "decrypted at 14:02"),
    );
  });
});

// ── The Simulator: its own store, else its own marked request (W7) ─────────

describe("the Simulator's certificate", () => {
  const sim = (name: string, trust: SimulatorTrust["trust"]): SimulatorTrust => ({ name, trust });
  const reading = (provedAt: number | null = null) => ({ device: "the Simulator", provedAt, mixed: true, tryStep: 3 });

  it("reads trusted only where every running Simulator's store holds it, and unread where none answered", () => {
    expect(storesTrusted([sim("iPhone 17 Pro", "trusted")])).toBe(true);
    expect(storesTrusted([sim("iPhone 17 Pro", "trusted"), sim("iPad mini", "absent")])).toBe(false);
    expect(storesTrusted([sim("iPhone 17 Pro", "trusted"), sim("iPad mini", "unknown")])).toBeNull();
    expect(storesTrusted([])).toBeNull();
  });

  it("names the Simulator its store was read in, by the name its window shows", () => {
    expect(simulatorTrustStatus([sim("iPhone 17 Pro", "trusted")], null, reading())).toEqual(
      verdict("done", "trusted in iPhone 17 Pro"),
    );
    expect(simulatorTrustStatus([sim("iPhone 17 Pro", "absent")], null, reading())).toEqual(
      verdict("not-done", "not in iPhone 17 Pro"),
    );
    const two = [sim("iPad mini", "trusted"), sim("iPhone 17 Pro", "trusted")];
    expect(simulatorTrustStatus(two, null, reading())).toEqual(verdict("done", "trusted in every running Simulator"));
    expect(trustedSimulators(two)).toBe("every running Simulator");
  });

  it("counts every running Simulator still missing it", () => {
    const missing = [sim("iPad mini", "absent"), sim("iPhone 17 Pro", "absent"), sim("iPad Air", "trusted")];
    expect(simulatorTrustStatus(missing, null, reading()).short).toBe("not done — not in 2 running Simulators");
  });

  // S1: this Mac's own traffic is on the Simulator's wire, so only the store
  // or the mark is its reading - and a store that reads absent outranks a mark.
  it("falls back to its own marked request where no store answered, and to that alone", () => {
    expect(simulatorTrustStatus([], null, reading())).toEqual(awaiting("the address in step 3 confirms it"));
    expect(simulatorTrustStatus([sim("iPhone 17 Pro", "unknown")], null, reading(AT_1402))).toEqual(
      verdict("done", "the Simulator's request decrypted at 14:02"),
    );
    expect(simulatorTrustStatus([sim("iPhone 17 Pro", "absent")], null, reading(AT_1402)).short).toBe(
      "not done — not in iPhone 17 Pro",
    );
  });

  it("says it is checking while a read the user asked for runs", () => {
    expect(simulatorTrustStatus([sim("iPhone 17 Pro", "trusted")], "checking", reading())).toEqual(
      verdict("checking", "the certificate store"),
    );
  });

  it("is never told a refusal's check it has none of", () => {
    expect(notDecryptingHere("this Mac")).toBe(
      "something reached Trawler and did not decrypt — this Mac does not trust Trawler's certificate yet. The steps below install it.",
    );
  });
});

// ── The emulator: its own store and proxy through adb, else another device's
// readings (W8) ─────────────────────────────────────────────────────────────

describe("the emulator's certificate and proxy", () => {
  const emu = (
    name: string,
    trust: EmulatorReading["trust"],
    proxy: EmulatorReading["proxy"] = "none",
  ): EmulatorReading => ({ name, trust, proxy });
  const reading = (provedAt: number | null = null, mixed = false) => ({
    device: "the emulator",
    provedAt,
    mixed,
    tryStep: 3,
  });
  const CONFIRM_HTTPS = "the emulator's first HTTPS request confirms it";
  const CONFIRM = "the emulator's first request confirms it";
  const refused = evidenceOf({ ...NO_TRAFFIC, ...openTunnel(AT_1402) }, AT_1402 + TUNNEL_GRACE_MS);
  const decrypted = evidenceOf({ ...NO_TRAFFIC, lastDecryptedAt: AT_1402 }, AT_1402);

  it("reads its own store by the name of its AVD, and counts every one still missing it", () => {
    expect(emulatorTrustStatus([emu("Pixel 7 API 35", "trusted")], null, reading(), NO_EVIDENCE, CONFIRM_HTTPS)).toEqual(
      verdict("done", "trusted in Pixel 7 API 35"),
    );
    expect(emulatorTrustStatus([emu("Pixel 7 API 35", "absent")], null, reading(AT_1402), decrypted, CONFIRM_HTTPS)).toEqual(
      verdict("not-done", "not in Pixel 7 API 35"),
    );
    const two = [emu("Pixel 7 API 35", "trusted"), emu("Pixel Fold", "trusted")];
    expect(emulatorTrustStatus(two, null, reading(), NO_EVIDENCE, CONFIRM_HTTPS).short).toBe(
      "done — trusted in every running emulator",
    );
    expect(trustedEmulators(two)).toBe("every running emulator");
    expect(trustedEmulators([emu("Pixel 7 API 35", "unknown")])).toBeNull();
    const missing = [emu("Pixel 7 API 35", "absent"), emu("Pixel Fold", "absent")];
    expect(emulatorTrustStatus(missing, null, reading(), NO_EVIDENCE, CONFIRM_HTTPS).short).toBe(
      "not done — not in 2 running emulators",
    );
  });

  // S1: where no store is read, it is another device: its mark, else traffic
  // from a span this machine's proxy did not point here, each timed.
  it("falls back to its marked request, then to what could have come from it, timed", () => {
    const unread = [emu("Pixel 7 API 35", "unknown")];
    expect(emulatorTrustStatus(unread, null, reading(AT_1402, true), NO_EVIDENCE, CONFIRM_HTTPS)).toEqual(
      verdict("done", "the emulator's request decrypted at 14:02"),
    );
    expect(emulatorTrustStatus([], null, reading(), decrypted, CONFIRM_HTTPS)).toEqual(
      verdict("done", "a request decrypted at 14:02"),
    );
    expect(emulatorTrustStatus([], null, reading(null, true), NO_EVIDENCE, CONFIRM_HTTPS)).toEqual(
      awaiting("the address in step 3 confirms it"),
    );
    expect(emulatorTrustStatus([], null, reading(), NO_EVIDENCE, CONFIRM_HTTPS)).toEqual(awaiting(CONFIRM_HTTPS));
  });

  // S2: its own services refuse a user certificate it holds.
  it("never reads a refusal as its certificate not working", () => {
    for (const read of [[], [emu("Pixel 7 API 35", "unknown")]])
      expect(emulatorTrustStatus(read, null, reading(), refused, CONFIRM_HTTPS)).toEqual(awaiting(CONFIRM_HTTPS));
  });

  it("says it is checking while a read the user asked for runs", () => {
    expect(emulatorTrustStatus([emu("Pixel 7 API 35", "trusted")], "checking", reading(), NO_EVIDENCE, CONFIRM_HTTPS)).toEqual(
      verdict("checking", "the certificate store"),
    );
    expect(emulatorRouteStatus([emu("Pixel 7 API 35", "trusted", "ours")], "checking", reading(), NO_EVIDENCE, CONFIRM)).toEqual(
      verdict("checking", "the proxy settings"),
    );
  });

  // S4 · C4: a proxy set on a network the emulator does not send over routes nothing.
  it("reads its own proxy, and says why a proxy it set routes nothing", () => {
    expect(emulatorRouteStatus([emu("Pixel 7 API 35", "unknown", "ours")], null, reading(), NO_EVIDENCE, CONFIRM)).toEqual(
      verdict("done", "Pixel 7 API 35 routes through Trawler"),
    );
    expect(emulatorRouteStatus([emu("Pixel 7 API 35", "unknown", "wifi-only")], null, reading(), decrypted, CONFIRM)).toEqual(
      verdict("not-done", "Pixel 7 API 35 sends over mobile data, not Wi-Fi"),
    );
    expect(emulatorRouteStatus([emu("Pixel 7 API 35", "unknown", "foreign")], null, reading(), NO_EVIDENCE, CONFIRM)).toEqual(
      verdict("not-done", "another proxy is set in Pixel 7 API 35"),
    );
    const both = [emu("Pixel 7 API 35", "unknown", "ours"), emu("Pixel Fold", "unknown", "ours")];
    expect(emulatorRouteStatus(both, null, reading(), NO_EVIDENCE, CONFIRM).short).toBe(
      "done — every running emulator routes through Trawler",
    );
  });

  // A proxy set at its start lives on this machine, where no guest reads it.
  it("never reads no proxy as not done: another device's readings settle it", () => {
    for (const proxy of ["none", "unknown"] as const) {
      const read = [emu("Pixel 7 API 35", "unknown", proxy)];
      expect(emulatorsRouted(read)).toBeNull();
      expect(emulatorRouteStatus(read, null, reading(), NO_EVIDENCE, CONFIRM)).toEqual(awaiting(CONFIRM));
      expect(emulatorRouteStatus(read, null, reading(AT_1402, true), NO_EVIDENCE, CONFIRM)).toEqual(
        verdict("done", "the emulator's request came through at 14:02"),
      );
    }
    expect(emulatorsRouted([])).toBeNull();
    expect(emulatorsRouted([emu("A", "unknown", "ours"), emu("B", "unknown", "none")])).toBeNull();
    expect(emulatorsRouted([emu("A", "unknown", "ours"), emu("B", "unknown", "wifi-only")])).toBe(false);
  });
});

describe("the emulator's try task words a refusal for what it is (W8's S2)", () => {
  const quiet = AT_1402 + RECENT_MS + 1;
  const refusing = { ...NO_TRAFFIC, ...openTunnel(quiet - TUNNEL_GRACE_MS - 1) };
  const around: TrafficContext = { ownsRefusal: false, trustedOn: null, proofStays: "past-anything" };

  it("says Android's own services never decrypt, and names the store that trusts it", () => {
    expect(emulatorRefusal(null)).toBe(
      "something is connecting and none of it is decrypting — Android's own services ignore a user certificate, so they never decrypt. A website in Chrome is the test.",
    );
    expect(emulatorRefusal("Pixel 7 API 35")).toBe(
      "something is connecting and none of it is decrypting — Pixel 7 API 35 trusts Trawler's certificate, so it is Android's own services, which ignore a user certificate, or an app that pins its certificate. A website in Chrome is the test.",
    );
  });

  it("carries that account wherever the wire refuses, mixed or not", () => {
    const refused = emulatorRefusal(null);
    expect(trafficStatus(refusing, quiet, { ...around, refused })).toEqual(
      verdict("not-working", "arriving but not decrypting", refused),
    );
    expect(mixedTryStatus(refusing, quiet, null, { refused })).toEqual(
      verdict("not-working", "arriving but not decrypting", refused),
    );
  });
});

// One owner for "when did anything last reach Trawler": the route's done line,
// the evidence and the idle reading all read it.
describe("when anything last reached Trawler", () => {
  it("is the newest of a decrypted request, a plain one and a tunnel", () => {
    expect(lastReachedAt(NO_TRAFFIC)).toBeNull();
    expect(lastReachedAt({ ...NO_TRAFFIC, lastDecryptedAt: 1, lastPlainAt: 3, lastTunnelAt: 2 })).toBe(3);
    expect(lastReachedAt({ ...NO_TRAFFIC, lastTunnelAt: 7 })).toBe(7);
  });

  it("is the time the evidence carries, beside the decrypted one", () => {
    const seen = { ...NO_TRAFFIC, lastDecryptedAt: 5, lastPlainAt: 9 };
    expect(evidenceOf(seen, 10)).toMatchObject({ reached: true, reachedAt: 9, decrypted: true, decryptedAt: 5 });
    expect(evidenceOf(NO_TRAFFIC, 10)).toEqual(NO_EVIDENCE);
  });
});

describe("whether this machine's own traffic may be arriving (DEC-63)", () => {
  it("may while its proxy points here or cannot be read, and may not while it is off or another", () => {
    expect(mayCarryOwnTraffic("ours")).toBe(true);
    expect(mayCarryOwnTraffic("unknown")).toBe(true);
    expect(mayCarryOwnTraffic("none")).toBe(false);
    expect(mayCarryOwnTraffic("foreign")).toBe(false);
  });
});

test("This machine's own request is named by the step its try task is", () => {
  expect(byOwnRequest("trust", null, 4)).toEqual(awaiting("the request in step 4 confirms it"));
  expect(byOwnRequest("trust", AT_1402, 4)).toEqual(verdict("done", "step 4's request decrypted at 14:02"));
});
