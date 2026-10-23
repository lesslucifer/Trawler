import type { FlowSummary } from "./api";
import { isCaptured } from "./flow";

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

// When this window's own request last came through, decrypted, since the setup
// session opened: the proof lasts the session (Q4), and a row from before it is
// some earlier session's.
export function ownRequestProvedAt(flows: FlowSummary[], since: number | null): number | null {
  let proved: number | null = null;
  for (const flow of flows) {
    if (!markOf(flow)?.startsWith(`${WINDOW_MARK}.`)) continue;
    const at = Date.parse(flow.started_at);
    if (Number.isNaN(at) || (since !== null && at < since)) continue;
    proved = Math.max(proved ?? at, at);
  }
  return proved;
}

// Which way one request went, as far as the core saw it (D9): decrypted, a
// tunnel to the host and nothing decrypted, or nothing at all while the page
// still got its answer.
export type RequestRoute = "through" | "undecrypted" | "around";
