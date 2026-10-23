import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, filterParams, type Filter, type FlowSummary, type FlowView, type TrawlerApi } from "./api";
import { hostOf, lastCaptureAt as newestCapture } from "./flow";

const DEBOUNCE_MS = 150;

// Whether the core answered, and what it said, are two facts and not two values
// of one. A core can be silent about a query it would have refused, and a
// refusal it already gave does not stop being true when the stream blips. Held
// together they need a precedence rule inside the state; held apart, precedence
// belongs where it is a design decision — in what each zone renders.
//
// `status` is the only /flows parameter with a grammar to violate, so a 400 is
// always about the value in that box, which is why a refusal can name it.
function refusalOf(error: unknown): boolean {
  return error instanceof ApiError && error.status === 400;
}

// The list reads what its filter asks for. The setup screen reads every row,
// whatever the list is filtered to (R7).
export type FlowsScope = "list" | "setup";

// Live flow feed, ported from the web shell: server-side filtering (the API owns
// the semantics) + a debounced refetch on filter change and on any SSE lifecycle
// event. The list is whatever GET /flows returns for the current filter.
export function useFlows(api: TrawlerApi, scope: FlowsScope = "list") {
  const [filter, setFilter] = useState<Filter>({});
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  // Each scope keeps its own answer, so coming back from setup never shows the
  // list rows its filter excludes. A listing with no filter answers both.
  const [everything, setEverything] = useState<FlowSummary[]>([]);
  // The core is not answering. Two things observe it — a listing that did not
  // come back, and the stream dropping — and both write here. Rows already on
  // screen survive it: they were true when they arrived, and discarding evidence
  // on a transient failure is the worse error.
  const [silent, setSilent] = useState(false);
  // The status value the core refused, or null. Distinct from silence: the core
  // answered, and what it said was no.
  const [refused, setRefused] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  // How many listings the core has answered with rows. An empty list is also
  // what the hook starts with, so this is what tells "the core said nothing is
  // captured" apart from "nothing has answered yet".
  const [answered, setAnswered] = useState(0);
  // When each CONNECT tunnel was seen — every HTTPS attempt, whether or not it
  // later decrypts into a flow (an untrusted-CA handshake produces a tunnel but
  // no flow). Onboarding reads them; the flow list ignores them.
  const [tunnelTimes, setTunnelTimes] = useState<readonly number[]>([]);
  // The newest tunnel to each host, for a caller waiting on one request's
  // route (D9). Read when asked rather than rendered, so it costs no render.
  const tunnels = useRef(new Map<string, number>());
  const tunnelAt = useCallback((host: string) => tunnels.current.get(host) ?? null, []);
  // The newest captured request this window has heard of, from any listing and
  // from the core's own newest row, so neither a filter nor a clear can lower
  // it. Undefined until the core has answered either.
  const [lastCaptureAt, setLastCaptureAt] = useState<number | null | undefined>(undefined);
  const raiseCapture = useCallback(
    (at: number | null) => setLastCaptureAt((mark) => (at === null ? (mark ?? null) : Math.max(mark ?? at, at))),
    [],
  );
  // Every host capture has ever seen, which is NOT what `flows` holds: that list
  // is server-filtered, so deriving the host options from it would collapse them
  // to the one already chosen and strand the user there. Accumulated instead, so
  // a host stays selectable once observed.
  const [hosts, setHosts] = useState<string[]>([]);

  const filterRef = useRef(filter);
  filterRef.current = filter;
  const scopeRef = useRef(scope);
  scopeRef.current = scope;

  // Four things trigger a listing — the filter effect, an SSE event, the stream
  // coming back, and retry — so responses can land out of order. Only the newest
  // request may write: a straggler is answering a question already answered, and
  // letting it through un-does a retry the user just watched succeed.
  const latest = useRef(0);

  // Two-armed `then` rather than `catch`, so a throw from the success arm is
  // never read as a failed fetch.
  const refresh = useCallback(() => {
    const mine = ++latest.current;
    const unfiltered = filterParams(filterRef.current).toString() === "";
    const forList = scopeRef.current === "list";
    const query = forList ? filterRef.current : {};
    // What was asked, captured with the request: by the time a refusal comes
    // back the box may hold something the core has never judged.
    const asked = query.status ?? "";
    api.latestCapture().then(
      (row) => raiseCapture(row && newestCapture([row])),
      () => {},
    );
    return api.listFlows(query).then(
      (list) => {
        if (mine !== latest.current) return;
        if (forList || unfiltered) setFlows(list);
        if (!forList || unfiltered) setEverything(list);
        setAnswered((n) => n + 1);
        raiseCapture(newestCapture(list));
        setHosts((known) => merge(known, list));
        setRefused(null);
        setSilent(false);
      },
      (error: unknown) => {
        if (mine !== latest.current) return;
        if (refusalOf(error)) {
          // The core answered, so it is not silent — and a refused query has no
          // answer to show, so the rows go with it and the count reads what is
          // on screen.
          setFlows([]);
          setRefused(asked);
          setSilent(false);
          return;
        }
        // No answer, or one the core should not have given. Either way the rows
        // are unverified rather than absent, so they stay.
        setSilent(true);
        // A refusal is about the value it was asked with. Silence on a
        // different value teaches nothing about it, but leaves it describing
        // something the box no longer holds — so it goes. Silence on the
        // refused value itself leaves it standing.
        setRefused((refusal) => (refusal === asked ? refusal : null));
      },
    );
  }, [api, raiseCapture]);

  const timer = useRef<ReturnType<typeof setTimeout>>();
  const debouncedRefresh = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(refresh, DEBOUNCE_MS);
  }, [refresh]);

  // Undebounced on purpose — the user asked for this one fetch, and its whole
  // in-flight window is rendered (fork D4), so it may not begin 150ms after the
  // button says it has.
  const retry = useCallback(() => {
    clearTimeout(timer.current);
    setRetrying(true);
    return refresh().finally(() => setRetrying(false));
  }, [refresh]);

  useEffect(() => {
    debouncedRefresh();
    return () => clearTimeout(timer.current);
  }, [filter, scope, debouncedRefresh]);

  useEffect(() => {
    const close = api.stream({
      onEvent: debouncedRefresh,
      onConnection: (authority) => {
        const at = Date.now();
        if (tunnels.current.size >= TUNNEL_HOSTS_MAX) tunnels.current = new Map();
        tunnels.current.set(tunnelHost(authority), at);
        setTunnelTimes((times) => [...times.slice(1 - TUNNEL_TIMES_MAX), at]);
      },
      // A core that dies under an open window sends nothing — so the dropped
      // stream is the only thing that can say so before the next keystroke. It
      // says only that: a refusal the core already gave is a separate fact and
      // stays exactly as it was.
      onDown: () => setSilent(true),
      // The stream came back; whether the listing did is the listing's to say.
      onUp: debouncedRefresh,
    });
    return close;
  }, [api, debouncedRefresh]);

  const update = useCallback(
    (key: keyof Filter, value: string) => setFilter((f) => ({ ...f, [key]: value })),
    [],
  );

  return {
    filter,
    flows,
    everything,
    hosts,
    tunnelTimes,
    tunnelAt,
    lastCaptureAt,
    silent,
    answered,
    refused,
    retrying,
    retry,
    update,
  };
}

// A remembered tunnel is only ever asked about seconds after it opened, so a
// reset loses nothing a caller is waiting on.
const TUNNEL_HOSTS_MAX = 1024;
// Setup reads the oldest tunnel nothing answered. A thousand tunnels never open
// inside one grace, so the oldest one kept past this many is a refusal already,
// and dropping older ones changes no reading.
export const TUNNEL_TIMES_MAX = 1024;

export function tunnelHost(authority: string): string {
  const port = authority.lastIndexOf(":");
  const host = port > authority.lastIndexOf("]") ? authority.slice(0, port) : authority;
  return host.toLowerCase();
}

export function merge(known: string[], list: FlowSummary[]): string[] {
  const seen = new Set(known);
  for (const flow of list) {
    const host = hostOf(flow.uri);
    if (host) seen.add(host);
  }
  return seen.size === known.length ? known : [...seen].sort();
}

// Fetches GET /flows/{id} when the selection changes; null clears it. A cancel
// flag drops a stale response if the selection moves before the fetch resolves.
// `failed` is what separates a fetch that did not produce a flow from one still
// in flight — without it the pane holds "loading this flow…" forever, which is
// the one claim that is false in both cases. It says only that: the pane names
// no cause, because the causes it would have to tell apart (a dropped flow, a
// refused id, a dead core) are the shell's or a later run's to word.
// `nonce` lets the shell's retry re-run this fetch alongside the listing.
export function useFlowDetail(api: TrawlerApi, id: string | null, nonce = 0) {
  const [flow, setFlow] = useState<FlowView | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFlow(null);
    setFailed(false);
    if (id == null) return;
    let cancelled = false;
    api.getFlow(id).then(
      (f) => {
        if (!cancelled) setFlow(f);
      },
      () => {
        if (!cancelled) setFailed(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, id, nonce]);

  return { flow, failed };
}
