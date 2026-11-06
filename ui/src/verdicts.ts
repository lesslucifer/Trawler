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

export const NO_PROXY_READING: ProxyReading = {
  status: "unknown",
  address: null,
};

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
export type SetupState =
  | "done"
  | "not-done"
  | "not-working"
  | "cant-check"
  | "checking"
  | "setting-up";

const STATE_FACE: Record<
  SetupState,
  { glyph: string; tone: StepTone; word: string }
> = {
  done: { glyph: "✓", tone: "ok", word: "done" },
  "not-done": { glyph: "○", tone: "muted", word: "not done" },
  "not-working": { glyph: "⚠", tone: "warn", word: "not working" },
  "cant-check": { glyph: "◐", tone: "muted", word: "can't check here" },
  checking: { glyph: "⟳", tone: "muted", word: "checking" },
  "setting-up": { glyph: "⟳", tone: "muted", word: "setting up" },
};

export const STATE_WORDS = Object.values(STATE_FACE).map((face) => face.word);

export function verdict(
  state: SetupState,
  reason?: string,
  remedy?: string,
): StepStatusView {
  const { glyph, tone, word } = STATE_FACE[state];
  const short = reason === undefined ? word : `${word} — ${reason}`;
  return remedy === undefined
    ? { glyph, tone, short }
    : { glyph, tone, short, remedy };
}

// A write in progress says only that it runs: what it waits on is the user,
// and the note under the button that started it says so, once (F19).
const SETTING_UP = verdict("setting-up");

// A task about something nothing here can read. Not a verdict about that
// device — nobody looked at it — but the promise that its first request is
// what will settle this.
export const awaiting = (confirmOn: string): StepStatusView =>
  verdict("cant-check", confirmOn);

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
  reachedAt: number | null;
  decryptedAt: number | null;
}

export const NO_EVIDENCE: Evidence = {
  reached: false,
  decrypted: false,
  unanswered: false,
  refused: false,
  reachedAt: null,
  decryptedAt: null,
};

// When anything last reached Trawler, of every kind the traffic layer keeps.
export function lastReachedAt(seen: Traffic): number | null {
  const times = [
    seen.lastDecryptedAt,
    seen.lastPlainAt,
    seen.lastTunnelAt,
  ].filter((at) => at !== null);
  return times.length === 0 ? null : Math.max(...times);
}

export function evidenceOf(seen: Traffic, now: number): Evidence {
  const decrypted = seen.lastDecryptedAt;
  const reachedAt = lastReachedAt(seen);
  const unanswered =
    seen.openTunnelAt !== null && now - seen.openTunnelAt >= TUNNEL_GRACE_MS;
  return {
    reached: reachedAt !== null,
    decrypted: decrypted !== null,
    unanswered,
    refused: unanswered && decrypted === null,
    reachedAt,
    decryptedAt: decrypted,
  };
}

// A probe that concludes is the whole state (DEC-33) - traffic, however much of
// it, never turns it green. Where it cannot conclude, the reading is whatever
// the caller names for that host: This machine's own request (DEC-62).
// Where the probe reads only the system's own store, its "trusted" says so:
// Linux's browsers keep stores of their own (W12's W6).
export const SYSTEM_TOOLS_TRUSTED = "trusted by the system's tools";

export function trustStatus(
  probe: TrustProbe,
  running: Running,
  unread: StepStatusView,
  trusted = "installed and trusted",
): StepStatusView {
  if (running === "checking")
    return verdict("checking", "the certificate store");
  if (running) return SETTING_UP;
  if (probe === "trusted") return verdict("done", trusted);
  if (probe === "installed")
    return verdict("not-done", "installed, not trusted");
  if (probe === "absent") return verdict("not-done", "not installed");
  return unread;
}

// A device no probe reads promised its first HTTPS request as its reading, and
// the traffic is that reading.
export function trustByTraffic(
  evidence: Evidence,
  confirmOn: string,
): StepStatusView {
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
// in its try task, step `tryStep` - travels this host's system proxy and is
// checked against this host's store, so one that came through decrypted settles
// both tasks, with the time it did: the proof lasts the setup session. Until
// then each task names that request, the second as the same one, so the two
// lines never say one sentence twice.
export function byOwnRequest(
  task: "trust" | "route",
  provedAt: number | null,
  tryStep: number,
): StepStatusView {
  if (provedAt === null)
    return awaiting(
      task === "trust"
        ? `the request in task ${tryStep} confirms it`
        : "the same request confirms it",
    );
  const at = clock(provedAt);
  return verdict(
    "done",
    task === "trust"
      ? `task ${tryStep}'s request decrypted at ${at}`
      : `task ${tryStep}'s request came through at ${at}`,
  );
}

// A device's route, where nothing on this machine reads it: the request it
// sends is the only witness, and it promised to be.
export function routeByTraffic(
  evidence: Evidence,
  confirmOn: string,
): StepStatusView {
  return evidence.reached
    ? verdict("done", "requests reached Trawler")
    : awaiting(confirmOn);
}

// DEC-63: another device's two tasks. Its marked request is that device's by
// construction - only this screen shows the mark - so it settles both, named
// and timed. Other traffic is that device's only where it arrived while this
// machine's own proxy could not be sending here too; while it may (`mixed`),
// nothing arriving can be told apart, and each task with no proof yet names
// the request that will.
export interface DeviceReading {
  device: string;
  provedAt: number | null;
  mixed: boolean;
  tryStep: number;
}

// The newest tunnel to the host of the address another device's try task
// offers, where no decrypted request to that host answered it, and whether it
// arrived where unmarked traffic is that device's (rule 3): a phone's test,
// since its own services refuse whatever its certificate does (W9's and W10's Q1).
export interface AddressRefusal {
  host: string;
  at: number;
  mine: boolean;
}

// Whether this machine's own traffic may be reaching Trawler, as its proxy
// reads: pointing here, or unread. Where it may, nothing unmarked is another
// device's (DEC-63).
export const mayCarryOwnTraffic = (probe: ProxyProbe): boolean =>
  probe === "ours" || probe === "unknown";

// Nothing here can read that device again, so a proof says when it was made
// and lasts the session (P2): the reader sees how old it is.
export function deviceTrustStatus(
  evidence: Evidence,
  reading: DeviceReading,
  confirmOn: string,
): StepStatusView {
  if (reading.provedAt !== null)
    return verdict(
      "done",
      `${reading.device}'s request decrypted at ${clock(reading.provedAt)}`,
    );
  if (evidence.decryptedAt !== null)
    return verdict(
      "done",
      `a request decrypted at ${clock(evidence.decryptedAt)}`,
    );
  if (reading.mixed)
    return awaiting(`the address in task ${reading.tryStep} confirms it`);
  return trustByTraffic(evidence, confirmOn);
}

export function deviceRouteStatus(
  evidence: Evidence,
  reading: DeviceReading,
  confirmOn: string,
): StepStatusView {
  if (reading.provedAt !== null)
    return verdict(
      "done",
      `${reading.device}'s request came through at ${clock(reading.provedAt)}`,
    );
  if (evidence.reachedAt !== null)
    return verdict(
      "done",
      `a request reached Trawler at ${clock(evidence.reachedAt)}`,
    );
  if (reading.mixed) return awaiting("the same request confirms it");
  return routeByTraffic(evidence, confirmOn);
}

// The newer of a device's two proofs: its marked request, and a request
// decrypted where unmarked traffic is its own.
function newestProof(
  evidence: Evidence,
  reading: DeviceReading,
): number | null {
  const times = [reading.provedAt, evidence.decryptedAt].filter(
    (at) => at !== null,
  );
  return times.length === 0 ? null : Math.max(...times);
}

// The address's refusal its certificate task says (W9's Q1, Q2): past its
// grace, the device's, and newer than any proof - one older than it showed a
// certificate since removed, or a trust switch since turned off.
export function refusedAddress(
  refusal: AddressRefusal | undefined,
  evidence: Evidence,
  reading: DeviceReading,
  now: number,
): AddressRefusal | null {
  if (
    refusal === undefined ||
    !refusal.mine ||
    now - refusal.at < TUNNEL_GRACE_MS
  )
    return null;
  const proof = newestProof(evidence, reading);
  return proof === null || refusal.at > proof ? refusal : null;
}

// The address's refusal a reading may stand on: the newest past its grace. A
// retry inside its own grace may yet decrypt, so the refusal before it stands
// until the retry is answered - and the newest is gone - or refused itself.
export function settledRefusal(
  newest: AddressRefusal | undefined,
  held: AddressRefusal | undefined,
  now: number,
): AddressRefusal | undefined {
  if (newest === undefined) return undefined;
  if (now - newest.at >= TUNNEL_GRACE_MS) return newest;
  return held !== undefined && held.at < newest.at ? held : undefined;
}

// A device whose test is its address: that refusal is the one its certificate
// task says, any other before a proof points at the address, and everything
// else reads as another device's (DEC-63).
export function addressTrustStatus(
  evidence: Evidence,
  reading: DeviceReading,
  refused: AddressRefusal | null,
  confirmOn: string,
): StepStatusView {
  const proof = newestProof(evidence, reading);
  if (refused !== null)
    return verdict(
      "not-working",
      proof === null
        ? `${refused.host} did not decrypt`
        : `${refused.host} did not decrypt at ${clock(refused.at)}, after decrypting at ${clock(proof)}`,
    );
  if (proof === null && !reading.mixed && evidence.refused)
    return awaiting(`the address in task ${reading.tryStep} confirms it`);
  return deviceTrustStatus(evidence, reading, confirmOn);
}

// Whether the refusal the try task reads now may be the address's: one past its
// grace and inside the window of now.
export const addressRefusedNow = (
  refusal: AddressRefusal | undefined,
  now: number,
): boolean =>
  refusal !== undefined &&
  now - refusal.at >= TUNNEL_GRACE_MS &&
  now - refusal.at <= RECENT_MS;

// What the shell read of each Simulator running: the name its own window
// shows, and whether its own store holds the live certificate (W7's Q1).
export interface SimulatorTrust {
  name: string;
  trust: TrustProbe;
}

// Every store read holds the certificate, one does not, or none answered -
// none running among them: the Simulator's, and the emulator's (W8's Q1).
export function storesTrusted(read: readonly SimulatorTrust[]): boolean | null {
  if (read.some((s) => s.trust === "absent")) return false;
  return read.length > 0 && read.every((s) => s.trust === "trusted")
    ? true
    : null;
}

export const trustedSimulators = (read: readonly SimulatorTrust[]) =>
  read.length === 1 ? read[0].name : "every running Simulator";

// The Simulator's certificate, read from its own store where one answers, and
// otherwise from its own marked request: its wire is this machine's too, so
// nothing else on it is its own (S1), and a refusal is never this task's (S2).
// `null` is no reading at all - a read that failed or has not landed - and
// never says none runs.
export function simulatorTrustStatus(
  stores: readonly SimulatorTrust[] | null,
  running: Running,
  reading: DeviceReading,
): StepStatusView {
  if (running === "checking")
    return verdict("checking", "the certificate store");
  const read = stores ?? [];
  const missing = read.filter((s) => s.trust === "absent");
  if (missing.length === 1)
    return verdict("not-done", `not in ${missing[0].name}`);
  if (missing.length > 1)
    return verdict("not-done", `not in ${missing.length} running Simulators`);
  if (storesTrusted(read))
    return verdict("done", `trusted in ${trustedSimulators(read)}`);
  if (reading.provedAt !== null)
    return verdict(
      "done",
      `${reading.device}'s request decrypted at ${clock(reading.provedAt)}`,
    );
  // The shell read every store there is, and there is none (W16's W4).
  if (stores?.length === 0) return verdict("not-done", NO_SIMULATOR_RUNNING);
  return awaiting(`the address in task ${reading.tryStep} confirms it`);
}

export const NO_SIMULATOR_RUNNING = "no Simulator running";
export const NO_EMULATOR_RUNNING = "no emulator running";
export const EMULATOR_STARTING = "the emulator is starting";

// What the shell read of each emulator running, through adb (W8's Q1, Q2):
// the name of its AVD, its own user store, and the proxy it sends through -
// set on a network it does not send over (`wifi-only`) routes nothing.
export type EmulatorProxy =
  "ours" | "wifi-only" | "foreign" | "none" | "unknown";

export interface EmulatorReading extends SimulatorTrust {
  proxy: EmulatorProxy;
}

// The Android SDK as the shell found it (W17's N1, I4): whether adb answered,
// the SDK its commands name as the host's shell writes it, and the AVD used
// last - a Google Play image's build has no su (W17's S3).
export interface AndroidSdk {
  adb: boolean;
  sdk: string | null;
  avd: { id: string; name: string; playStore: boolean } | null;
}

// starting: adb lists an emulator that has not finished booting (W17's S2).
export interface EmulatorRead extends AndroidSdk {
  emulators: EmulatorReading[];
  starting: boolean;
}

const each = (read: readonly SimulatorTrust[], every: string) =>
  read.length === 1 ? read[0].name : every;

// The emulator's certificate, read from its own store where adb reads one, and
// otherwise from what could have come from it (DEC-63 rule 9). Its own services
// refuse a user certificate by design, so a refusal is never this task's (S2).
// `null` is no reading at all - adb not found, or a read not landed - and never
// says none runs (W17's W4); one adb lists still booting is not running yet (S2).
export function emulatorTrustStatus(
  emulators: readonly EmulatorReading[] | null,
  running: Running,
  reading: DeviceReading,
  evidence: Evidence,
  confirmOn: string,
  starting = false,
): StepStatusView {
  if (running === "checking")
    return verdict("checking", "the certificate store");
  const read = emulators ?? [];
  const missing = read.filter((e) => e.trust === "absent");
  if (missing.length === 1)
    return verdict("not-done", `not in ${missing[0].name}`);
  if (missing.length > 1)
    return verdict("not-done", `not in ${missing.length} running emulators`);
  if (storesTrusted(read))
    return verdict(
      "done",
      `trusted in ${each(read, "every running emulator")}`,
    );
  if (reading.provedAt !== null)
    return verdict(
      "done",
      `${reading.device}'s request decrypted at ${clock(reading.provedAt)}`,
    );
  if (evidence.decryptedAt !== null)
    return verdict(
      "done",
      `a request decrypted at ${clock(evidence.decryptedAt)}`,
    );
  if (emulators?.length === 0)
    return verdict(
      "not-done",
      starting ? EMULATOR_STARTING : NO_EMULATOR_RUNNING,
    );
  return awaiting(
    reading.mixed
      ? `the address in task ${reading.tryStep} confirms it`
      : confirmOn,
  );
}

// Every running emulator sends through Trawler, one sends elsewhere, or none
// said: a proxy set at its start is this machine's, which no guest reads, so
// no proxy read is never "not done" (S4).
export function emulatorsRouted(
  read: readonly EmulatorReading[],
): boolean | null {
  if (read.some((e) => e.proxy === "wifi-only" || e.proxy === "foreign"))
    return false;
  return read.length > 0 && read.every((e) => e.proxy === "ours") ? true : null;
}

export function emulatorRouteStatus(
  read: readonly EmulatorReading[],
  running: Running,
  reading: DeviceReading,
  evidence: Evidence,
  confirmOn: string,
): StepStatusView {
  if (running === "checking") return verdict("checking", "the proxy settings");
  const wifiOnly = read.find((e) => e.proxy === "wifi-only");
  if (wifiOnly)
    return verdict(
      "not-done",
      `${wifiOnly.name} sends over mobile data, not Wi-Fi`,
    );
  const foreign = read.find((e) => e.proxy === "foreign");
  if (foreign)
    return verdict("not-done", `another proxy is set in ${foreign.name}`);
  if (emulatorsRouted(read))
    return verdict(
      "done",
      `${each(read, "every running emulator")} routes through Trawler`,
    );
  return deviceRouteStatus(evidence, reading, confirmOn);
}

// The try task's account of a refusal on the emulator, before any proof: its
// own services never decrypt, so a refusal says nothing of its certificate.
export function emulatorRefusal(trustedIn: string | null): string {
  return trustedIn === null
    ? "something is connecting and none of it is decrypting — Android's own services ignore a user certificate, so they never decrypt. A website in Chrome is the test."
    : `something is connecting and none of it is decrypting — ${trustedIn} trusts Trawler's certificate, so it is Android's own services, which ignore a user certificate, or an app that pins its certificate. A website in Chrome is the test.`;
}

export const trustedEmulators = (read: readonly EmulatorReading[]) =>
  storesTrusted(read) ? each(read, "every running emulator") : null;

// The device the end may name: the one whose marked request is the newest
// decrypted proof the screen reads.
export function markedDevice(
  device: string,
  provedAt: number | null,
  seen: Traffic,
): string | null {
  return provedAt !== null && provedAt >= (seen.lastDecryptedAt ?? provedAt)
    ? device
    : null;
}

// The certificate task's own account of a refusal: the certificate is the step
// left, so the sentence says whose store, points at the steps under it, and
// gives the user who already took them the check their platform's users miss.
export const notDecryptingHere = (device: string, check?: string) =>
  `something reached Trawler and did not decrypt — ${device} does not trust Trawler's certificate yet. The steps below install it.${check === undefined ? "" : ` ${check}`}`;

export function proxyStatus(
  probe: ProxyProbe,
  running: Running,
  words: { device: string; unread: StepStatusView },
): StepStatusView {
  if (running === "checking") return verdict("checking", "the proxy settings");
  if (running) return SETTING_UP;
  if (probe === "ours")
    return verdict("done", `${words.device} routes through Trawler`);
  if (probe === "unknown") return words.unread;
  if (probe === "foreign") return verdict("not-done", "another proxy is set");
  return verdict("not-done", "the system proxy is off");
}

// What a user is owed before a button replaces something they set: which proxy
// it is, and that Trawler does not put it back. The address is the shell's to
// report; where it could not read one the sentence says what it knows and no
// more, and the confirm dialog carries the rest either way.
export function foreignProxyNotice(
  device: string,
  address: string | null,
): string {
  const named = address === null ? "" : `: ${address}`;
  return `another proxy is set on ${device}${named}. Setting Trawler replaces it, and Trawler never puts it back.`;
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
  const recent = (at: number | null): at is number =>
    at !== null && now - at <= RECENT_MS;
  if (recent(seen.lastDecryptedAt)) return "capturing";
  const open = seen.openTunnelAt;
  if (recent(seen.lastTunnelAt) && open !== null)
    return now - open < TUNNEL_GRACE_MS ? "just-arrived" : "not-decrypting";
  if (recent(seen.lastPlainAt)) return "plain-only";
  return lastReachedAt(seen) === null ? "waiting" : "idle";
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
// `refusedAfterProof`: the platform's own account of a refusal after its proof.
// `proofStays`: how far a decrypted request that proved this setup outlasts
// what follows it - not at all; once quiet, on This machine, whose other two
// tasks re-read themselves (U5); or past anything after it that does not
// decrypt, on another device, which nothing here reads again (P1, S1).
export type ProofStays = "never" | "once-quiet" | "past-anything";

export interface TrafficContext {
  ownsRefusal: boolean;
  trustedOn: string | null;
  proofStays: ProofStays;
  refusedAfterProof?: string;
  refused?: string;
}

const NO_CONTEXT: TrafficContext = {
  ownsRefusal: false,
  trustedOn: null,
  proofStays: "never",
};

const OUTLASTED: Record<ProofStays, readonly TrafficReading[]> = {
  never: [],
  "once-quiet": ["idle"],
  "past-anything": ["idle", "just-arrived", "not-decrypting", "plain-only"],
};

// A refusal after a proof is not the certificate the proof showed working, so
// it sends no one back to that step (S1).
export const REFUSED_AFTER_PROOF =
  "something else is connecting and not decrypting — one request already did, so it is an app that pins its certificate or keeps a store of its own, or another device.";

function keptProof(
  decryptedAt: number,
  refusal: string | undefined,
  since = "",
): StepStatusView {
  return verdict("done", `decrypted at ${clock(decryptedAt)}${since}`, refusal);
}

// The try task words a refusal only where the certificate task does not own
// it - it reads done, or it is This machine's and no probe read it, so the
// refusal may be anyone's (D7); anywhere else the certificate task says it, once.
export function trafficStatus(
  seen: Traffic,
  now: number,
  around: TrafficContext = NO_CONTEXT,
): StepStatusView {
  const reading = trafficReading(seen, now);
  const decryptedAt = seen.lastDecryptedAt;
  if (decryptedAt !== null && OUTLASTED[around.proofStays].includes(reading)) {
    const after = (at: number | null) => at !== null && at > decryptedAt;
    const since =
      after(seen.lastPlainAt) || after(seen.lastTunnelAt)
        ? ""
        : ", nothing since";
    const refusal =
      reading === "not-decrypting" && !around.ownsRefusal
        ? (around.refusedAfterProof ?? REFUSED_AFTER_PROOF)
        : undefined;
    return keptProof(decryptedAt, refusal, since);
  }
  const view = TRAFFIC_VERDICT[reading];
  if (reading !== "not-decrypting") return view;
  if (around.ownsRefusal)
    return { glyph: view.glyph, tone: view.tone, short: view.short };
  if (around.refused !== undefined) return { ...view, remedy: around.refused };
  return around.trustedOn === null
    ? view
    : { ...view, remedy: trustedNotDecrypting(around.trustedOn) };
}

// Another device's try task while nothing arriving is told apart from this
// machine's (DEC-63). The wire may still say nothing decrypts, naming no device,
// as This machine's try task does where its store went unread (D7) - or naming
// the store a probe read as trusting it, so the refusal is someone else's;
// otherwise only a proof already that device's is read, and it outlasts the
// wire (S1).
export function mixedTryStatus(
  wire: Traffic,
  now: number,
  decryptedAt: number | null,
  around: {
    refusedAfterProof?: string;
    trustedOn?: string | null;
    refused?: string;
    ownsRefusal?: boolean;
  } = {},
): StepStatusView {
  const {
    refusedAfterProof = REFUSED_AFTER_PROOF,
    trustedOn = null,
    refused,
    ownsRefusal = false,
  } = around;
  const refusing = trafficReading(wire, now) === "not-decrypting";
  if (decryptedAt !== null)
    return keptProof(
      decryptedAt,
      refusing && !ownsRefusal ? refusedAfterProof : undefined,
    );
  if (!refusing) return awaiting("the address below confirms it");
  const view = TRAFFIC_VERDICT["not-decrypting"];
  if (ownsRefusal)
    return { glyph: view.glyph, tone: view.tone, short: view.short };
  if (refused !== undefined) return { ...view, remedy: refused };
  return trustedOn === null
    ? view
    : { ...view, remedy: trustedNotDecrypting(trustedOn) };
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
export function provenDevice(
  device: string,
  trust: TrustProbe,
  proxy: ProxyProbe,
): string | null {
  return trust === "trusted" && proxy === "ours" ? device : null;
}

