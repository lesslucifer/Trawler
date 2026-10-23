import type { FlowSummary } from "./api";
import { isCaptured, sessionTraffic } from "./flow";
import type { PlatformId } from "./setup";
import { NO_TRAFFIC, type Traffic } from "./verdicts";

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

// A stretch in which this machine's proxy read off or another, as this window
// read it: nothing this machine sent through its system proxy then reached
// Trawler. The newest is open while that is still the reading.
export interface ClearSpan {
  from: number;
  to: number | null;
}

const isOpen = (spans: readonly ClearSpan[]) => spans.length > 0 && spans[spans.length - 1].to === null;

export function nextClearSpans(spans: readonly ClearSpan[], clear: boolean, at: number): readonly ClearSpan[] {
  if (clear === isOpen(spans)) return spans;
  return clear
    ? [...spans, { from: at, to: null }]
    : [...spans.slice(0, -1), { from: spans[spans.length - 1].from, to: at }];
}

// What another device's tasks read, beside the traffic that is its (DEC-63):
// the newest of its marked requests, whether this machine's own traffic may be
// arriving now, and the wire since its address was allowed, which is nobody's.
export interface AnotherDevice {
  provedAt: number | null;
  mixed: boolean;
  wire: Traffic;
}

// From the moment its address was allowed (A2), that device's marked rows are
// its own whatever this machine's proxy read, and an unmarked row or a tunnel is
// its only where it arrived in a clear span. So a proof, once read, lasts the
// session (rule 4): no later reading moves the span it arrived in.
export function deviceTraffic(
  flows: FlowSummary[],
  device: { since: number | null; mark: string; clear: readonly ClearSpan[] },
  tunnels: readonly number[],
): AnotherDevice & { traffic: Traffic } {
  const { since, mark, clear } = device;
  const mixed = !isOpen(clear);
  if (since === null) return { traffic: NO_TRAFFIC, wire: NO_TRAFFIC, provedAt: null, mixed };
  const inClear = (at: number) => clear.some((span) => at >= span.from && (span.to === null || at < span.to));
  const its = flows.filter((flow) => {
    const marked = markOf(flow);
    return marked === null ? inClear(Date.parse(flow.started_at)) : marked === mark;
  });
  return {
    traffic: sessionTraffic(its, since, tunnels.filter(inClear)),
    wire: sessionTraffic(flows, since, tunnels),
    provedAt: lastProvedAt(flows, since, (seen) => seen === mark),
    mixed,
  };
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
