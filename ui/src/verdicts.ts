// DEC-33's two layers of truth, resolved. THIS MACHINE: two read-only probes,
// re-read whenever their answer can change — does this host trust the CA, does
// its system proxy point at Trawler — and, where neither can answer, the one
// request this app makes of its own (DEC-62). TRAFFIC: what has reached the
// proxy, timestamped. A verdict never borrows from the other layer: a flow
// captured an hour ago says nothing about the proxy now, and a phone pointed
// here says nothing about this machine (D7). Pure data → unit-tested; the
// probes live in Rust.

// What the shell read of the live certificate: trusted; installed and not
// trusted (a keychain copy nobody marked Always Trust); not installed; or
// nothing on this host could read a store.
export type TrustProbe = "trusted" | "installed" | "absent" | "unknown";
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
// this setup session. `openTunnelAt` is the oldest tunnel no decrypted request
// has followed: a refusal is timed from it, so a client that retries every few
// seconds never restarts a grace the tunnel before it already used up.
export interface Traffic {
  lastDecryptedAt: number | null;
  lastPlainAt: number | null;
  lastTunnelAt: number | null;
  openTunnelAt: number | null;
}

export const NO_TRAFFIC: Traffic = {
  lastDecryptedAt: null,
  lastPlainAt: null,
  lastTunnelAt: null,
  openTunnelAt: null,
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
  const unanswered = seen.openTunnelAt !== null && now - seen.openTunnelAt >= TUNNEL_GRACE_MS;
  return {
    reached: decrypted !== null || seen.lastPlainAt !== null || seen.lastTunnelAt !== null,
    decrypted: decrypted !== null,
    unanswered,
    refused: unanswered && decrypted === null,
  };
}

// A probe that concludes is the whole state (DEC-33) - traffic, however much of
// it, never turns it green. Where it cannot conclude, the reading is whatever
// the caller names for that host: This machine's own request (DEC-62).
export function trustStatus(probe: TrustProbe, running: Running, unread: StepStatusView): StepStatusView {
  if (running === "checking") return verdict("checking", "the certificate store");
  if (running) return SETTING_UP;
  if (probe === "trusted") return verdict("done", "installed and trusted");
  if (probe === "installed") return verdict("not-done", "installed, not trusted");
  if (probe === "absent") return verdict("not-done", "not installed");
  return unread;
}

// A device no probe reads promised its first HTTPS request as its reading, and
// the traffic is that reading.
export function trustByTraffic(evidence: Evidence, confirmOn: string): StepStatusView {
  if (evidence.decrypted) return verdict("done", "a request decrypted");
  if (evidence.refused) return verdict("not-working", "nothing decrypted");
  return awaiting(confirmOn);
}

// The time a proof was made, as the line says it: 24 h, to the minute.
export function clock(at: number): string {
  const date = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

// This machine, where no probe answers (DEC-62). Its own request - the button
// in its step 3 - travels this host's system proxy and is checked against this
// host's store, so one that came through decrypted settles both tasks, with the
// time it did: the proof lasts the setup session. Until then each task names
// that request, the second as the same one, so the two lines never say one
// sentence twice.
export function byOwnRequest(task: "trust" | "route", provedAt: number | null): StepStatusView {
  if (provedAt === null)
    return awaiting(task === "trust" ? "the request in step 3 confirms it" : "the same request confirms it");
  const at = clock(provedAt);
  return verdict("done", task === "trust" ? `step 3's request decrypted at ${at}` : `step 3's request came through at ${at}`);
}

// A device's route, where nothing on this machine reads it: the request it
// sends is the only witness, and it promised to be.
export function routeByTraffic(evidence: Evidence, confirmOn: string): StepStatusView {
  return evidence.reached ? verdict("done", "requests reached Trawler") : awaiting(confirmOn);
}

// DEC-63: another device's two tasks. Its marked request is that device's by
// construction - only this screen shows the mark - so it settles both, named
// and timed. Other traffic is that device's only where this machine's own proxy
// cannot be sending its traffic here too (`mixed` false); until then nothing
// unmarked can be told apart, and each task names the request that will.
export interface DeviceReading {
  device: string;
  provedAt: number | null;
  mixed: boolean;
  checkStep: number;
}

// The marked request as the only traffic there is, where nothing else counts.
export const markedTraffic = (provedAt: number | null): Traffic =>
  provedAt === null ? NO_TRAFFIC : { ...NO_TRAFFIC, lastDecryptedAt: provedAt };

// Nothing here can read that device again, so a proof says when it was made
// and lasts the session (P2): the reader sees how old it is.
export function deviceTrustStatus(
  seen: Traffic,
  evidence: Evidence,
  reading: DeviceReading,
  confirmOn: string,
): StepStatusView {
  if (reading.provedAt !== null)
    return verdict("done", `${reading.device}'s request decrypted at ${clock(reading.provedAt)}`);
  if (reading.mixed) return awaiting(`the address in step ${reading.checkStep} confirms it`);
  if (evidence.decrypted && seen.lastDecryptedAt !== null)
    return verdict("done", `a request decrypted at ${clock(seen.lastDecryptedAt)}`);
  return trustByTraffic(evidence, confirmOn);
}

export function deviceRouteStatus(
  seen: Traffic,
  evidence: Evidence,
  reading: DeviceReading,
  confirmOn: string,
): StepStatusView {
  if (reading.provedAt !== null)
    return verdict("done", `${reading.device}'s request came through at ${clock(reading.provedAt)}`);
  if (reading.mixed) return awaiting("the same request confirms it");
  const reachedAt = Math.max(seen.lastDecryptedAt ?? 0, seen.lastPlainAt ?? 0, seen.lastTunnelAt ?? 0);
  if (evidence.reached && reachedAt > 0) return verdict("done", `a request reached Trawler at ${clock(reachedAt)}`);
  return routeByTraffic(evidence, confirmOn);
}

// The device the end may name: the one whose marked request is the newest
// decrypted proof the screen reads.
export function markedDevice(device: string, provedAt: number | null, seen: Traffic): string | null {
  return provedAt !== null && provedAt >= (seen.lastDecryptedAt ?? provedAt) ? device : null;
}

// The certificate task's own account of a refusal: the certificate is the step
// left, so the sentence says whose store, points at the steps under it, and
// gives the user who already took them the check their platform's users miss.
export const notDecryptingHere = (device: string, check: string) =>
  `something reached Trawler and did not decrypt — ${device} does not trust Trawler's certificate yet. The steps below install it. ${check}`;

export function proxyStatus(
  probe: ProxyProbe,
  running: Running,
  words: { device: string; unread: StepStatusView },
): StepStatusView {
  if (running === "checking") return verdict("checking", "the proxy settings");
  if (running) return SETTING_UP;
  if (probe === "ours") return verdict("done", `${words.device} routes through Trawler`);
  if (probe === "unknown") return words.unread;
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
  const open = seen.openTunnelAt;
  if (recent(seen.lastTunnelAt) && open !== null)
    return now - open < TUNNEL_GRACE_MS ? "just-arrived" : "not-decrypting";
  if (recent(seen.lastPlainAt)) return "plain-only";
  const anything = seen.lastDecryptedAt !== null || seen.lastPlainAt !== null || seen.lastTunnelAt !== null;
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

// What the try task's reading depends on beyond the traffic itself.
// `ownsRefusal`: the certificate task words a refusal, so this one does not.
// `trustedOn`: the device whose own probe reads its certificate trusted.
// `proofStays`: a decrypted request proved this setup, and quiet does not
// unprove it - This machine's, whose other two tasks re-read themselves (U5),
// and another device's, whose two tasks keep their proof's time (P1).
export interface TrafficContext {
  ownsRefusal: boolean;
  trustedOn: string | null;
  proofStays: boolean;
}

const NO_CONTEXT: TrafficContext = { ownsRefusal: false, trustedOn: null, proofStays: false };

// The try task words a refusal only where the certificate task does not own
// it - it reads done, or it is This machine's and no probe read it, so the
// refusal may be anyone's (D7); anywhere else the certificate task says it, once.
export function trafficStatus(seen: Traffic, now: number, around: TrafficContext = NO_CONTEXT): StepStatusView {
  const reading = trafficReading(seen, now);
  const decryptedAt = seen.lastDecryptedAt;
  if (reading === "idle" && around.proofStays && decryptedAt !== null) {
    const after = (at: number | null) => at !== null && at > decryptedAt;
    const since = after(seen.lastPlainAt) || after(seen.lastTunnelAt) ? "" : ", nothing since";
    return verdict("done", `decrypted at ${clock(decryptedAt)}${since}`);
  }
  const view = TRAFFIC_VERDICT[reading];
  if (reading !== "not-decrypting") return view;
  if (around.ownsRefusal) return { glyph: view.glyph, tone: view.tone, short: view.short };
  return around.trustedOn === null ? view : { ...view, remedy: trustedNotDecrypting(around.trustedOn) };
}

// Another device's try task where nothing unmarked is its (DEC-63). The wire may
// still say nothing decrypts, naming no device, as This machine's try task does
// where its store went unread (D7); otherwise only the marked request is read.
export function markedTryStatus(seen: Traffic, now: number, provedAt: number | null): StepStatusView {
  if (trafficReading(seen, now) === "not-decrypting") return TRAFFIC_VERDICT["not-decrypting"];
  return provedAt === null ? awaiting("the address below confirms it") : verdict("done", `decrypted at ${clock(provedAt)}`);
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
