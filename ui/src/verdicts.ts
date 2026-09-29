// DEC-33's two layers of truth, resolved. THIS MACHINE: two read-only probes,
// re-read on demand — does this host trust the CA, does its system proxy point
// at Trawler. TRAFFIC: what has reached the proxy, timestamped. A verdict never
// borrows from the other layer: a flow captured an hour ago says nothing about
// the proxy now, and a probe says nothing about a phone pointed here. Pure data
// → unit-tested; the probes live in Rust.

export type TrustProbe = "trusted" | "untrusted" | "unknown";
export type ProxyProbe = "ours" | "foreign" | "none" | "unknown";

// What the shell read of this machine's system proxy: the classification, and
// the address it classified. The address exists so the screen can name the
// proxy it is about to replace instead of asking the user to take its word for
// it; it is null wherever nothing is set, and wherever the probe could not read.
export interface ProxyReading {
  status: ProxyProbe;
  address: string | null;
}

export const NO_PROXY_READING: ProxyReading = { status: "unknown", address: null };

export type StepTone = "ok" | "warn" | "muted";

// `short` is the ≤40-char fragment StatusLine renders — a verdict, not a
// sentence. Where the verdict also implies an instruction, `remedy` carries it
// out to a Notice: what is true and what to do about it are two elements.
export interface StepStatusView {
  glyph: string;
  tone: StepTone;
  short: string;
  remedy?: string;
}

// When each kind of traffic last reached Trawler, as epoch ms; null = not in
// this setup session.
export interface Traffic {
  lastDecryptedAt: number | null;
  lastPlainAt: number | null;
  lastTunnelAt: number | null;
}

export const NO_TRAFFIC: Traffic = {
  lastDecryptedAt: null,
  lastPlainAt: null,
  lastTunnelAt: null,
};

// "Now" is the last 30 s: long enough to read the screen after one curl, short
// enough that a browser left alone stops reading as captured.
export const RECENT_MS = 30_000;
// A tunnel is answered by its decrypted flow within a moment of opening; until
// then it is neither proof nor failure.
export const TUNNEL_GRACE_MS = 3_000;

export type Running = "installing" | "setting" | "checking" | null;

// The closed vocabulary of a setup task's state (F5): one glyph, one tone and
// one word each, so the state is read from the word before the reason and no
// glyph means two things. The reason after the dash is the resolver's.
export type SetupState = "done" | "not-done" | "not-working" | "cant-check" | "checking" | "setting-up";

const STATE_FACE: Record<SetupState, { glyph: string; tone: StepTone; word: string }> = {
  done: { glyph: "✓", tone: "ok", word: "done" },
  "not-done": { glyph: "○", tone: "muted", word: "not done" },
  "not-working": { glyph: "⚠", tone: "warn", word: "not working" },
  "cant-check": { glyph: "◐", tone: "muted", word: "can't check here" },
  checking: { glyph: "⟳", tone: "muted", word: "checking" },
  "setting-up": { glyph: "⟳", tone: "muted", word: "setting up" },
};

export const STATE_WORDS = Object.values(STATE_FACE).map((face) => face.word);

export function verdict(state: SetupState, reason?: string, remedy?: string): StepStatusView {
  const { glyph, tone, word } = STATE_FACE[state];
  const short = reason === undefined ? word : `${word} — ${reason}`;
  return remedy === undefined ? { glyph, tone, short } : { glyph, tone, short, remedy };
}

// A write in progress says only that it runs: what it waits on is the user,
// and the note under the button that started it says so, once (F19).
const SETTING_UP = verdict("setting-up");

// A task about something nothing here can read. Not a verdict about that
// device — nobody looked at it — but the promise that its first request is
// what will settle this.
export const awaiting = (confirmOn: string): StepStatusView => verdict("cant-check", confirmOn);

// What this setup's own traffic says about the two tasks, for wherever no probe
// concludes them. It is evidence gathered, not a reading of now: a request that
// reached Trawler proved the route when it arrived, and does not stop having
// proved it when the window of "now" closes. A tunnel is a refusal only once
// its grace has passed with no decrypted request after it (`unanswered`), and
// one decrypted request outranks any number of refusals - it is the
// certificate working somewhere - so `refused` is a refusal with no decrypt at
// all.
export interface Evidence {
  reached: boolean;
  decrypted: boolean;
  unanswered: boolean;
  refused: boolean;
}

export const NO_EVIDENCE: Evidence = {
  reached: false,
  decrypted: false,
  unanswered: false,
  refused: false,
};

export function evidenceOf(seen: Traffic, now: number): Evidence {
  const decrypted = seen.lastDecryptedAt;
  const tunnel = seen.lastTunnelAt;
  const unanswered =
    tunnel !== null &&
    (decrypted === null || decrypted < tunnel) &&
    now - tunnel >= TUNNEL_GRACE_MS;
  return {
    reached: decrypted !== null || seen.lastPlainAt !== null || tunnel !== null,
    decrypted: decrypted !== null,
    unanswered,
    refused: unanswered && decrypted === null,
  };
}

// A probe that concludes is the whole state (DEC-33) - traffic, however much of
// it, never turns it green. Where it cannot conclude, the task promised its
// device's first HTTPS request as its reading, and the traffic is that reading.
export function trustStatus(
  probe: TrustProbe,
  running: Running,
  confirmOn: string,
  evidence: Evidence = NO_EVIDENCE,
): StepStatusView {
  if (running === "checking") return verdict("checking", "the certificate store");
  if (running) return SETTING_UP;
  if (probe === "trusted") return verdict("done", "installed and trusted");
  if (probe === "untrusted") return verdict("not-done", "the certificate isn't trusted");
  if (evidence.decrypted) return verdict("done", "a request decrypted");
  if (evidence.refused) return verdict("not-working", "nothing decrypted");
  return awaiting(confirmOn);
}

// A device's route, where nothing on this machine reads it: the request it
// sends is the only witness, and it promised to be.
export function routeByTraffic(evidence: Evidence, confirmOn: string): StepStatusView {
  return evidence.reached ? verdict("done", "requests reached Trawler") : awaiting(confirmOn);
}

// The certificate task's own account of a refusal: the certificate is the step
// left, so the sentence says whose store, points at the steps under it, and
// gives the user who already took them the check their platform's users miss.
export const notDecryptingHere = (device: string, check: string) =>
  `something reached Trawler and did not decrypt — ${device} does not trust Trawler's certificate yet. The steps below install it. ${check}`;

export function proxyStatus(
  probe: ProxyProbe,
  running: Running,
  words: { device: string; confirmOn: string },
): StepStatusView {
  if (running === "checking") return verdict("checking", "the proxy settings");
  if (running) return SETTING_UP;
  if (probe === "ours") return verdict("done", `${words.device} routes through Trawler`);
  if (probe === "unknown") return awaiting(words.confirmOn);
  if (probe === "foreign") return verdict("not-done", "another proxy is set");
  return verdict("not-done", "the system proxy is off");
}

// What a user is owed before a button replaces something they set: which proxy
// it is, and that Trawler does not put it back. The address is the shell's to
// report; where it could not read one the sentence says what it knows and no
// more, and the confirm dialog carries the rest either way.
export function foreignProxyNotice(device: string, address: string | null): string {
  const named = address === null ? "" : `: ${address}`;
  return `another proxy is set on ${device}${named}. Setting Trawler replaces it while Trawler runs, and restores nothing when it quits.`;
}

// What the wire says, as one name. The words each reading carries are two
// different things — a precondition's verdict and the screen's lead sentence —
// so the reading is resolved once and worded twice.
export type TrafficReading =
  | "capturing"
  | "just-arrived"
  | "not-decrypting"
  | "plain-only"
  | "idle"
  | "waiting";

// Positive evidence outranks a failing tunnel: with one client decrypting and
// another not, the screen's purpose is met, and the failing client shows the
// moment the working one goes quiet.
export function trafficReading(seen: Traffic, now: number): TrafficReading {
  const recent = (at: number | null): at is number => at !== null && now - at <= RECENT_MS;
  if (recent(seen.lastDecryptedAt)) return "capturing";
  const tunnel = seen.lastTunnelAt;
  const unanswered =
    tunnel !== null && (seen.lastDecryptedAt === null || seen.lastDecryptedAt < tunnel);
  if (recent(tunnel) && unanswered)
    return now - tunnel < TUNNEL_GRACE_MS ? "just-arrived" : "not-decrypting";
  if (recent(seen.lastPlainAt)) return "plain-only";
  const anything = seen.lastDecryptedAt !== null || seen.lastPlainAt !== null || tunnel !== null;
  return anything ? "idle" : "waiting";
}

const TRAFFIC_VERDICT: Record<TrafficReading, StepStatusView> = {
  capturing: verdict("done", "capturing traffic"),
  "just-arrived": verdict("checking", "a connection just arrived"),
  // Nothing here knows which platform a tunnel came from — the app cannot
  // attribute one — so the remedy names the device that is connecting and
  // leaves the screen's own platform choice out of it.
  "not-decrypting": verdict(
    "not-working",
    "arriving but not decrypting",
    "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
  ),
  "plain-only": verdict(
    "not-done",
    "only plain http so far",
    "open a website whose address starts with https://",
  ),
  idle: verdict("not-done", "nothing arriving right now"),
  waiting: verdict("not-done", "waiting for the first request"),
};

// Where the chosen platform's own certificate probe reads trusted, a tunnel
// nothing decrypts cannot be that store's missing certificate, and the remedy
// may not send the user back to a step the task above it reads as done.
const trustedNotDecrypting = (device: string) =>
  `something is connecting and none of it is decrypting — ${device} trusts Trawler's certificate, so it comes from another device, or from an app with a certificate store of its own, that does not trust it yet.`;

// The try task words a refusal only where the certificate task reads done and
// so cannot own it; anywhere else the certificate task says it, once.
export function trafficStatus(
  seen: Traffic,
  now: number,
  certificate: { done: boolean; trustedOn: string | null } = { done: true, trustedOn: null },
): StepStatusView {
  const reading = trafficReading(seen, now);
  const view = TRAFFIC_VERDICT[reading];
  if (reading !== "not-decrypting") return view;
  if (!certificate.done) return { glyph: view.glyph, tone: view.tone, short: view.short };
  return certificate.trustedOn === null
    ? view
    : { ...view, remedy: trustedNotDecrypting(certificate.trustedOn) };
}

// The sentence that ends setup, in the two forms the evidence supports. A
// decrypted request proves Trawler is capturing; which device sent it is a
// thing the app cannot know (the design run's open blocked-on row), so the
// device is named only where this machine is the device — the one case where
// the answer is the same either way.
export function setUpOn(device: string | null): string {
  const here = device === null ? "" : ` from ${device}`;
  return `You're set up — Trawler is capturing traffic${here}`;
}

// The device a decrypted request may be credited to. This machine is named only
// where both of its probes say the request could have come from it; anywhere
// else the sentence proves capture and names no one.
export function provenDevice(device: string, trust: TrustProbe, proxy: ProxyProbe): string | null {
  return trust === "trusted" && proxy === "ours" ? device : null;
}

// The reversal turns the proxy off and nothing else, and says so. Where the
// certificate reads trusted it also says that stays, and where to remove it;
// anywhere else there is nothing installed to name.
export function stopRoutingNote(certificateTrusted: boolean): string {
  return certificateTrusted
    ? "this turns the proxy off and nothing else — Trawler's certificate stays trusted, and step 1 has the steps to remove it."
    : "this turns the proxy off and nothing else.";
}
