import type { FlowSummary, ReachState } from "./api";
import { hostOf, isCaptured, sessionTraffic } from "./flow";
import type { PlatformId } from "./setup";
import { NO_TRAFFIC, TUNNEL_GRACE_MS, type AddressRefusal, type Traffic } from "./verdicts";

// DEC-56: one request, on the user's click, to a destination the sentence
// names and Trawler does not own. The window's own fetch goes the way a browser
// on this machine goes — through its system proxy, checked against its trust
// store — so what Trawler captures of it is the reading. Its own outcome says
// only whether it completed: an opaque answer is all a cross-origin request may
// read, and all the button needs.
export const REQUEST_HOST = "example.com";

// DEC-62: the request carries a mark, so the row it produces is known to be
// this window's and no other device's, and is visible as such in the list.
const MARK = "trawler-check";
const WINDOW_MARK = Math.random().toString(36).slice(2, 10);
let sent = 0;

export function nextMark(): string {
  sent += 1;
  return `${WINDOW_MARK}.${sent}`;
}

export const markedUrl = (mark: string) => `https://${REQUEST_HOST}/?${MARK}=${mark}`;

export async function makeRequest(mark: string): Promise<void> {
  await fetch(markedUrl(mark), { mode: "no-cors", cache: "no-store" });
}

function markOf(flow: FlowSummary): string | null {
  if (!isCaptured(flow) || !flow.uri.includes(`${MARK}=`)) return null;
  try {
    const url = new URL(flow.uri);
    return url.protocol === "https:" && url.hostname === REQUEST_HOST ? url.searchParams.get(MARK) : null;
  } catch {
    return null;
  }
}

// A captured https:// row is one Trawler decrypted, so this window's marked
// row is its request through this host's proxy, trusted by this host's store.
export function isOwnRequest(flow: FlowSummary, mark: string): boolean {
  return markOf(flow) === mark;
}

// DEC-63: another device's request carries a mark too, typed or scanned on that
// device, so the row it produces is known to be that device's. One per setup
// session and platform, short enough to type, and derived rather than held, so
// the screen that shows it and the reading that finds it cannot disagree.
export function deviceMark(session: number | null, platform: PlatformId): string {
  let hash = 0x811c9dc5;
  for (const char of `${WINDOW_MARK}:${session}:${platform}`) {
    hash = Math.imul(hash ^ char.charCodeAt(0), 0x01000193);
  }
  return ((hash >>> 0) % 36 ** 4).toString(36).padStart(4, "0");
}

function lastProvedAt(flows: FlowSummary[], since: number | null, mine: (mark: string) => boolean): number | null {
  let proved: number | null = null;
  for (const flow of flows) {
    const mark = markOf(flow);
    if (mark === null || !mine(mark)) continue;
    const at = Date.parse(flow.started_at);
    if (Number.isNaN(at) || (since !== null && at < since)) continue;
    proved = Math.max(proved ?? at, at);
  }
  return proved;
}

// When this window's own request last came through, decrypted, since the setup
// session opened: the proof lasts the session (Q4), and a row from before it is
// some earlier session's.
export function ownRequestProvedAt(flows: FlowSummary[], since: number | null): number | null {
  return lastProvedAt(flows, since, (mark) => mark.startsWith(`${WINDOW_MARK}.`));
}

// When the address reach holds now was first seen armed. Nothing off this
// machine could reach Trawler there before it, and a device pointed at an
// address armed earlier is not pointed at this one, so another device's
// evidence starts here (A2): the same address keeps its moment, another starts
// anew, and none clears it.
export type ArmedSince = { proxy: string; at: number } | null;

export function nextArmedSince(was: ArmedSince, reach: ReachState, at: number): ArmedSince {
  const proxy = reach.armed ? reach.proxy : null;
  if (proxy === null) return null;
  return was?.proxy === proxy ? was : { proxy, at };
}

// A row from before the setup session opened is an earlier session's (DEC-60).
export function deviceEvidenceSince(session: number | null, armed: ArmedSince): number | null {
  return armed === null ? null : Math.max(session ?? 0, armed.at);
}

// This window's readings of this machine's proxy as off or another, as runs
// from the first such reading to the last: between two of them nothing this
// machine sent through its system proxy reached Trawler. A reading speaks only
// for its own moment - somewhere between being sent and landing - so a run
// starts when its first landed and ends when its last was sent, and what
// arrives after that is no one's until the next reading says the same.
export interface ClearSpan {
  from: number;
  to: number;
}

export interface ClearReadings {
  spans: readonly ClearSpan[];
  open: boolean;
}

export const NO_CLEAR_READINGS: ClearReadings = { spans: [], open: false };

// The reading flips only when this machine's proxy changes or stops answering,
// so a session reads few runs; past this many, the oldest go, with any proof
// that only they held.
export const CLEAR_SPANS_MAX = 64;


// Two readings further apart than `apartMax` vouch for nothing between them.
export function nextClearReadings(
  was: ClearReadings,
  clear: boolean,
  read: { sent: number; landed: number },
  apartMax: number,
): ClearReadings {
  const last = was.spans[was.spans.length - 1];
  if (was.open && clear && read.sent - last.to <= apartMax)
    return { spans: [...was.spans.slice(0, -1), { from: last.from, to: read.sent }], open: true };
  // A run of one reading vouches for no time at all, so it keeps no place.
  const closed = was.open && last.from > last.to ? was.spans.slice(0, -1) : was.spans;
  if (!clear) return was.open ? { spans: closed, open: false } : was;
  return { spans: [...closed.slice(1 - CLEAR_SPANS_MAX), { from: read.landed, to: read.sent }], open: true };
}

// What another device's tasks read, beside the traffic that is its (DEC-63):
// the newest of its marked requests, whether this machine's own traffic may be
// arriving now, and - only while it may - the wire since its address was
// allowed, which is nobody's.
export interface AnotherDevice {
  provedAt: number | null;
  mixed: boolean;
  wire: Traffic;
  refusal?: AddressRefusal;
}

// From the moment its address was allowed (A2) - and nothing while none is -
// that device's marked rows are its own whatever this machine's proxy read, and
// an unmarked row or a tunnel is its only where it arrived inside a clear span.
// So a proof, once read, lasts the session (rule 4): a later reading neither
// moves nor widens a closed span.
export function deviceTraffic(
  flows: FlowSummary[],
  device: { allowedSince: number | null; mark: string; clear: ClearReadings },
  tunnels: readonly number[],
  addressTunnelAt: number | null = null,
): AnotherDevice & { traffic: Traffic } {
  const { allowedSince: since, mark, clear } = device;
  const mixed = !clear.open;
  if (since === null) return { traffic: NO_TRAFFIC, wire: NO_TRAFFIC, provedAt: null, mixed };
  const inClear = (at: number) => clear.spans.some((span) => at >= span.from && at <= span.to);
  const its = flows.filter((flow) => {
    const marked = markOf(flow);
    return marked === null ? inClear(Date.parse(flow.started_at)) : marked === mark;
  });
  return {
    traffic: sessionTraffic(its, since, tunnels.filter(inClear), flows),
    wire: mixed ? sessionTraffic(flows, since, tunnels) : NO_TRAFFIC,
    provedAt: lastProvedAt(flows, since, (seen) => seen === mark),
    mixed,
    refusal: addressRefusal(flows, since, addressTunnelAt, inClear),
  };
}

// The address the try task offers is opened on that device, so a tunnel to its
// host that no decrypted request to that host answered is that address
// refused - or anyone's opening of the same host, which is why it is the
// device's only inside a clear span.
function addressRefusal(
  flows: FlowSummary[],
  since: number,
  tunnelAt: number | null,
  inClear: (at: number) => boolean,
): AddressRefusal | undefined {
  if (tunnelAt === null || tunnelAt < since) return undefined;
  // The tunnel is timed when its event lands here and the request by the
  // core's clock, so the request it carries may read a moment older.
  const answered = flows.some(
    (flow) =>
      isCaptured(flow) &&
      flow.uri.startsWith("https://") &&
      hostOf(flow.uri) === REQUEST_HOST &&
      Date.parse(flow.started_at) >= tunnelAt - TUNNEL_GRACE_MS,
  );
  return answered ? undefined : { host: REQUEST_HOST, at: tunnelAt, mine: inClear(tunnelAt) };
}

// Which way one request went, as far as the core saw it (D9): decrypted, a
// tunnel to the host and nothing decrypted, or nothing at all while the page
// still got its answer.
export type RequestRoute = "through" | "undecrypted" | "around";

export interface RequestSeen {
  decrypted: boolean;
  tunnelAt: number | null;
  sentAt: number;
  answered: boolean;
}

// Null is a request that went nowhere at all: no row, no tunnel, no answer.
export function requestRoute({ decrypted, tunnelAt, sentAt, answered }: RequestSeen): RequestRoute | null {
  if (decrypted) return "through";
  if ((tunnelAt ?? 0) >= sentAt) return "undecrypted";
  return answered ? "around" : null;
}
