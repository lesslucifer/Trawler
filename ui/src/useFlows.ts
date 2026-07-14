import { useCallback, useEffect, useRef, useState } from "react";
import type { Filter, FlowSummary, TrawlerApi } from "./api";

const DEBOUNCE_MS = 150;

// Live flow feed, ported from the web shell: server-side filtering (the API owns
// the semantics) + a debounced refetch on filter change and on any SSE lifecycle
// event. The list is whatever GET /flows returns for the current filter.
export function useFlows(api: TrawlerApi) {
  const [filter, setFilter] = useState<Filter>({});
  const [flows, setFlows] = useState<FlowSummary[]>([]);
  const [live, setLive] = useState(false);
  // Tunnels that reached the proxy but may never become a flow (untrusted-CA
  // handshakes). Onboarding treats connections>0 as "proxy is routing"; the
  // flow list itself ignores them. Seeded once from /stats, then bumped live.
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
    api.connectionCount().then(setConnections).catch(() => {});
    const close = api.stream({
      onEvent: debouncedRefresh,
      onLive: setLive,
      onConnection: () => setConnections((n) => n + 1),
    });
    return () => {
      setLive(false);
      close();
    };
  }, [api, debouncedRefresh]);

  const update = useCallback(
    (key: keyof Filter, value: string) => setFilter((f) => ({ ...f, [key]: value })),
    [],
  );

  return { filter, flows, live, connections, update };
}
