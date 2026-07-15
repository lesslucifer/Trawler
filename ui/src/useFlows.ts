import { useCallback, useEffect, useRef, useState } from "react";
import type { Filter, FlowSummary, FlowView, TrawlerApi } from "./api";

const DEBOUNCE_MS = 150;

// Live flow feed, ported from the web shell: server-side filtering (the API owns
// the semantics) + a debounced refetch on filter change and on any SSE lifecycle
// event. The list is whatever GET /flows returns for the current filter.
export function useFlows(api: TrawlerApi) {
  const [filter, setFilter] = useState<Filter>({});
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  // Count of CONNECT tunnels the proxy has opened — every HTTPS attempt, whether
  // or not it later decrypts into a flow (an untrusted-CA handshake produces a
  // tunnel but no flow). Onboarding treats connections>0 as "proxy is routing";
  // the flow list itself ignores them. Seeded once from /stats, then bumped live.
  const [connections, setConnections] = useState(0);

  const filterRef = useRef(filter);
  filterRef.current = filter;

  const refresh = useCallback(() => {
    api.listFlows(filterRef.current).then(setFlows).catch(() => {});
  }, [api]);

  const timer = useRef<ReturnType<typeof setTimeout>>();
  const debouncedRefresh = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(refresh, DEBOUNCE_MS);
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
    api.connectionCount().then((c) => setConnections((n) => Math.max(n, c))).catch(() => {});
    const close = api.stream({
      onEvent: debouncedRefresh,
      onConnection: () => setConnections((n) => n + 1),
    });
    return close;
  }, [api, debouncedRefresh]);

  const update = useCallback(
    (key: keyof Filter, value: string) => setFilter((f) => ({ ...f, [key]: value })),
    [],
  );

  return { filter, flows, connections, update };
}

// Fetches GET /flows/{id} when the selection changes; null clears it. A cancel
// flag drops a stale response if the selection moves before the fetch resolves.
export function useFlowDetail(api: TrawlerApi, id: string | null) {
  const [flow, setFlow] = useState<FlowView | null>(null);

  useEffect(() => {
    setFlow(null);
    if (id == null) return;
    let cancelled = false;
    api
      .getFlow(id)
      .then((f) => {
        if (!cancelled) setFlow(f);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api, id]);

  return flow;
}
