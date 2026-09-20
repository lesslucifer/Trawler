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
// this session. `ever` also counts what the core saw before the UI subscribed
// (the /stats tally), which carries no time and so can only ever mean "idle".
export interface Traffic {
  lastDecryptedAt: number | null;
  lastPlainAt: number | null;
  lastTunnelAt: number | null;
  ever: boolean;
}

export const NO_TRAFFIC: Traffic = {
  lastDecryptedAt: null,
  lastPlainAt: null,
  lastTunnelAt: null,
  ever: false,
};

// "Now" is the last 30 s: long enough to read the screen after one curl, short
// enough that a browser left alone stops reading as captured.
export const RECENT_MS = 30_000;
// A tunnel is answered by its decrypted flow within a moment of opening; until
// then it is neither proof nor failure.
export const TUNNEL_GRACE_MS = 3_000;

export type Running = "installing" | "setting" | "checking" | null;

// A reading in progress says which reading, because "checking…" over three
// tasks tells the user nothing about any of them. A write in progress says
// what it is waiting for, which is the user.
const SETTING_UP: StepStatusView = {
  glyph: "⟳",
  tone: "muted",
  short: "setting up — approve the prompt…",
};

const reading = (what: string): StepStatusView => ({ glyph: "⟳", tone: "muted", short: what });

// A task about something nothing here can read. Not a verdict about that
// device — nobody looked at it — but the promise that its first request is
// what will settle this.
export const awaiting = (confirmOn: string): StepStatusView => ({
  glyph: "◐",
  tone: "muted",
  short: confirmOn,
});

export function trustStatus(
  probe: TrustProbe,
  running: Running,
  confirmOn: string,
): StepStatusView {
  if (running === "checking") return reading("reading the certificate store…");
  if (running) return SETTING_UP;
  if (probe === "trusted") return { glyph: "✓", tone: "ok", short: "done — installed and trusted" };
  if (probe === "unknown") return awaiting(confirmOn);
  return { glyph: "○", tone: "muted", short: "not done — the certificate isn't trusted" };
}

export function proxyStatus(
  probe: ProxyProbe,
  running: Running,
  words: { device: string; confirmOn: string },
): StepStatusView {
  if (running === "checking") return reading("reading the proxy settings…");
  if (running) return SETTING_UP;
  if (probe === "ours")
    return { glyph: "✓", tone: "ok", short: `done — ${words.device} routes through Trawler` };
  if (probe === "unknown") return awaiting(words.confirmOn);
  if (probe === "foreign")
    return { glyph: "○", tone: "muted", short: "not done — another proxy is set" };
  return { glyph: "○", tone: "muted", short: "not done — the system proxy is off" };
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
  const anything =
    seen.ever || seen.lastDecryptedAt !== null || seen.lastPlainAt !== null || tunnel !== null;
  return anything ? "idle" : "waiting";
}

const TRAFFIC_VERDICT: Record<TrafficReading, StepStatusView> = {
  capturing: { glyph: "✓", tone: "ok", short: "capturing traffic" },
  "just-arrived": { glyph: "⟳", tone: "muted", short: "a connection just arrived…" },
  "not-decrypting": {
    glyph: "⚠",
    tone: "warn",
    short: "arriving but not decrypting",
    // Nothing here knows which platform a tunnel came from — the app cannot
    // attribute one — so the remedy names the device that is connecting and
    // leaves the screen's own platform choice out of it.
    remedy:
      "something is connecting and none of it is decrypting — the device it comes from does not trust Trawler's certificate yet. Finish the certificate step on that device.",
  },
  "plain-only": {
    glyph: "◐",
    tone: "muted",
    short: "arriving — plain HTTP only",
    remedy: "open a website whose address starts with https://",
  },
  idle: { glyph: "○", tone: "muted", short: "nothing arriving right now" },
  waiting: { glyph: "○", tone: "muted", short: "waiting for the first request" },
};

export function trafficStatus(seen: Traffic, now: number): StepStatusView {
  return TRAFFIC_VERDICT[trafficReading(seen, now)];
}

// The sentence the screen ends on. It names the thing the user chose, because
// the moment setup succeeds is the moment they find out it was THEIR device
// that worked.
export function setUpOn(device: string): string {
  return `You're set up — Trawler is capturing traffic from ${device}`;
}
