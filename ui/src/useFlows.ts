import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, type Filter, type FlowSummary, type FlowView, type TrawlerApi } from "./api";
import { hostOf } from "./flow";

const DEBOUNCE_MS = 150;

// The two ways a listing can fail, kept apart because the screen says a
// different sentence for each and offers retry for only one of them: an
// unreachable core is worth asking again, a refused query never answers
// differently until the query changes.
export type FeedFailure = { kind: "unreachable" } | { kind: "rejected"; value: string };

// `status` is the only /flows parameter with a grammar to violate, so a 400 is
// always about the value in that box — which is why the rejected failure can
// name it.
function failureOf(error: unknown, status: string): FeedFailure {
  return error instanceof ApiError && error.status === 400
    ? { kind: "rejected", value: status }
    : { kind: "unreachable" };
}

// Live flow feed, ported from the web shell: server-side filtering (the API owns
// the semantics) + a debounced refetch on filter change and on any SSE lifecycle
// event. The list is whatever GET /flows returns for the current filter.
export function useFlows(api: TrawlerApi) {
  const [filter, setFilter] = useState<Filter>({});
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  // Why the last fetch produced nothing new, or null when it produced a list.
  // Rows already on screen survive it: they were true when they arrived, and
  // discarding evidence on a transient failure is the worse error.
  const [failure, setFailure] = useState<FeedFailure | null>(null);
  const [retrying, setRetrying] = useState(false);
  // Count of CONNECT tunnels the proxy has opened — every HTTPS attempt, whether
  // or not it later decrypts into a flow (an untrusted-CA handshake produces a
  // tunnel but no flow). Onboarding reads it as "traffic has reached Trawler at
  // some point" — idle rather than waiting; the flow list itself ignores it.
  // Seeded once from /stats, then bumped live.
  const [connections, setConnections] = useState(0);
  // The moment the newest tunnel was seen. Only a live event carries one — the
  // /stats seed is a count with no time, so it can say "ever", never "now".
  const [lastConnectionAt, setLastConnectionAt] = useState<number | null>(null);
  // Every host capture has ever seen, which is NOT what `flows` holds: that list
  // is server-filtered, so deriving the host options from it would collapse them
  // to the one already chosen and strand the user there. Accumulated instead, so
  // a host stays selectable once observed.
  const [hosts, setHosts] = useState<string[]>([]);

  const filterRef = useRef(filter);
  filterRef.current = filter;

  // Resolves to whether the listing arrived, which is what lets the caller of
  // `retry` know its surface may go away. Two-armed `then` rather than `catch`,
  // so a throw from the success arm is never read as a failed fetch.
  const refresh = useCallback(
    () =>
      api.listFlows(filterRef.current).then(
        (list) => {
          setFlows(list);
          setHosts((known) => merge(known, list));
          setFailure(null);
          return true;
        },
        (error: unknown) => {
          const next = failureOf(error, filterRef.current.status ?? "");
          // A refused query has no answer to show, so the rows go with it and the
          // count reads what is on screen. An unreachable core is the opposite
          // case: its last answer was true when it arrived.
          if (next.kind === "rejected") setFlows([]);
          setFailure(next);
          return false;
        },
      ),
    [api],
  );

  const timer = useRef<ReturnType<typeof setTimeout>>();
  const debouncedRefresh = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(refresh, DEBOUNCE_MS);
  }, [refresh]);

  // Undebounced on purpose — the user asked for this one fetch, and its whole
  // in-flight window is rendered (fork D4), so it may not begin 150ms after the
  // button says it has.
  const retry = useCallback(() => {
    setRetrying(true);
    return refresh().finally(() => setRetrying(false));
  }, [refresh]);

  useEffect(() => {
    debouncedRefresh();
    return () => clearTimeout(timer.current);
  }, [filter, debouncedRefresh]);

  useEffect(() => {
    // The /stats seed and live `connection` bumps race: the seed fetch may resolve
    // after a bump already landed, and its count predates that connection. Merge as
    // a floor (max) inside the functional updater so the stale seed can never regress
    // a value live events have already advanced — the counter is monotonic anyway.
    // A failed seed says nothing the listing has not already said, so it stays quiet.
    api.connectionCount().then((c) => setConnections((n) => Math.max(n, c))).catch(() => {});
    const close = api.stream({
      onEvent: debouncedRefresh,
      onConnection: () => {
        setConnections((n) => n + 1);
        setLastConnectionAt(Date.now());
      },
      // A core that dies under an open window sends nothing — so the dropped
      // stream is the only thing that can say so before the next keystroke.
      onDown: () => setFailure({ kind: "unreachable" }),
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
    hosts,
    connections,
    lastConnectionAt,
    failure,
    retrying,
    retry,
    update,
  };
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
// `failed` is what separates a fetch that never answered from one still in
// flight — without it the pane holds "loading this flow…" forever. `nonce` lets
// the shell's retry re-run this fetch too, since the silence it explains is the
// same silence.
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
      (error: unknown) => {
        // A 404 is the ring having dropped this flow while it was selected — a
        // different cause with a different sentence, and one the pane's own
        // design still owns. Only silence is the shell's to state.
        const dropped = error instanceof ApiError && error.status === 404;
        if (!cancelled && !dropped) setFailed(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, id, nonce]);

  return { flow, failed };
}
