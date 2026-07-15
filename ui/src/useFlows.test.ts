import { act, renderHook, waitFor } from "@testing-library/react";
import { useFlows } from "./useFlows";
import type { StreamHandlers, TrawlerApi } from "./api";

// Fake API with a hand-resolved /stats promise and a captured stream handler set,
// so a test can interleave a live `connection` bump with the seed's resolution.
function fakeApi() {
  let handlers: StreamHandlers | undefined;
  let resolveStats: ((n: number) => void) | null = null;
  const api: TrawlerApi = {
    listFlows: async () => [],
    connectionCount: () => new Promise<number>((res) => (resolveStats = res)),
    caCertPem: async () => "",
    getFlow: async () => { throw new Error("unused"); },
    getBody: async () => { throw new Error("unused"); },
    stream: (h) => {
      handlers = h;
      return () => {};
    },
  };
  return {
    api,
    fireConnection: () => handlers?.onConnection?.(),
    resolveStats: (n: number) => resolveStats?.(n),
    statsPending: () => resolveStats !== null,
  };
}

// Regression: the /stats seed must never regress a value that live `connection`
// events already advanced. A bump landing WHILE the seed fetch is in flight, then a
// stale seed count resolving, must leave the bumped value — else `connections` (and
// thus onboarding's `trafficSeen`) falsely drops back to the pre-connection count.
test("a connection bump during the in-flight /stats seed is not clobbered by a stale seed", async () => {
  const f = fakeApi();
  const { result } = renderHook(() => useFlows(f.api));

  await waitFor(() => expect(f.statsPending()).toBe(true));

  // Live tunnel observed before the seed resolves: counter goes 0 -> 1.
  act(() => f.fireConnection());
  expect(result.current.connections).toBe(1);

  // The seed now resolves with a STALE count (0 — taken before that connection).
  // async act flushes the promise's .then microtask that applies the seed.
  await act(async () => {
    f.resolveStats(0);
  });

  // Floor-merge keeps the bumped value; a plain setter would clobber it to 0.
  expect(result.current.connections).toBe(1);
});

test("the /stats seed still advances the counter when it is ahead of live bumps", async () => {
  const f = fakeApi();
  const { result } = renderHook(() => useFlows(f.api));
  await waitFor(() => expect(f.statsPending()).toBe(true));

  await act(async () => {
    f.resolveStats(4);
  });
  expect(result.current.connections).toBe(4);
  act(() => f.fireConnection());
  expect(result.current.connections).toBe(5);
});
