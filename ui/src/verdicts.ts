// DEC-33's two layers of truth, resolved. THIS MACHINE: two read-only probes,
// re-read on demand — does this host trust the CA, does its system proxy point
// at Trawler. TRAFFIC: what has reached the proxy, timestamped. A verdict never
// borrows from the other layer: a flow captured an hour ago says nothing about
// the proxy now, and a probe says nothing about a phone pointed here. Pure data
// → unit-tested; the probes live in Rust.

export type TrustProbe = "trusted" | "untrusted" | "unknown";
export type ProxyProbe = "ours" | "foreign" | "none" | "unknown";
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

const IN_FLIGHT: Record<Exclude<Running, null>, string> = {
  installing: "installing…",
  setting: "setting the proxy…",
  checking: "checking…",
};

const inFlight = (running: Exclude<Running, null>): StepStatusView => ({
  glyph: "⟳",
  tone: "muted",
  short: IN_FLIGHT[running],
});

export function trustStatus(probe: TrustProbe, running: Running, error?: string): StepStatusView {
  if (running) return inFlight(running);
  // The probe's positive wins over a stale action error: the CA is trusted
  // whatever the last click did.
  if (probe === "trusted") return { glyph: "✓", tone: "ok", short: "trusted on this machine" };
  if (error) return { glyph: "⚠", tone: "warn", short: "install failed", remedy: error };
  if (probe === "unknown")
    return {
      glyph: "◐",
      tone: "muted",
      short: "can't check automatically",
      remedy: "step 3 shows whether traffic decrypts",
    };
  return { glyph: "○", tone: "muted", short: "not trusted on this machine yet" };
}

export function proxyStatus(probe: ProxyProbe, running: Running, error?: string): StepStatusView {
  if (running) return inFlight(running);
  if (probe === "ours") return { glyph: "✓", tone: "ok", short: "system proxy is set to Trawler" };
  if (error) return { glyph: "⚠", tone: "warn", short: "couldn't set the proxy", remedy: error };
  if (probe === "foreign")
    return {
      glyph: "○",
      tone: "muted",
      short: "another proxy is set",
      remedy: "point it at Trawler here, or capture from a device configured to use Trawler",
    };
  if (probe === "unknown")
    return {
      glyph: "◐",
      tone: "muted",
      short: "can't check automatically",
      remedy: "step 3 shows whether traffic arrives",
    };
  return { glyph: "○", tone: "muted", short: "not set on this machine yet" };
}

// Positive evidence outranks a failing tunnel: with one client decrypting and
// another not, the screen's purpose is met, and the failing client shows the
// moment the working one goes quiet.
export function trafficStatus(seen: Traffic, now: number): StepStatusView {
  const recent = (at: number | null): at is number => at !== null && now - at <= RECENT_MS;
  if (recent(seen.lastDecryptedAt)) return { glyph: "✓", tone: "ok", short: "capturing traffic" };
  const tunnel = seen.lastTunnelAt;
  const unanswered =
    tunnel !== null && (seen.lastDecryptedAt === null || seen.lastDecryptedAt < tunnel);
  if (recent(tunnel) && unanswered) {
    if (now - tunnel < TUNNEL_GRACE_MS)
      return { glyph: "⟳", tone: "muted", short: "a connection just arrived…" };
    return {
      glyph: "⚠",
      tone: "warn",
      short: "arriving but not decrypting",
      remedy:
        "the client sending it doesn't trust Trawler's certificate — install it there; step 1 covers this machine",
    };
  }
  if (recent(seen.lastPlainAt))
    return {
      glyph: "◐",
      tone: "muted",
      short: "arriving — plain HTTP only",
      remedy: "an HTTPS request is what proves decryption",
    };
  const anything =
    seen.ever || seen.lastDecryptedAt !== null || seen.lastPlainAt !== null || tunnel !== null;
  if (anything) return { glyph: "○", tone: "muted", short: "idle — nothing arriving now" };
  return { glyph: "○", tone: "muted", short: "waiting for your first request" };
}
