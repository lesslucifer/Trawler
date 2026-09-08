import { act, renderHook, waitFor } from "@testing-library/react";
import { useFlows } from "./useFlows";
import type { FlowSummary, StreamHandlers } from "./api";
import { apiStub } from "./apiStub";

// Fake API with a hand-resolved /stats promise and a captured stream handler set,
// so a test can interleave a live `connection` bump with the seed's resolution.
function fakeApi() {
  let handlers: StreamHandlers | undefined;
  let resolveStats: ((n: number) => void) | null = null;
  const api = apiStub({
    connectionCount: () => new Promise<number>((res) => (resolveStats = res)),
    stream: (h) => {
      handlers = h;
      return () => {};
    },
  });
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

// A tunnel counted before the UI subscribed has no time, so the seed leaves the
// timestamp empty; only a live event can say "now", which is what the setup
// screen's traffic verdict reads.
test("a live connection is stamped with its time; the /stats seed stamps nothing", async () => {
  const f = fakeApi();
  const { result } = renderHook(() => useFlows(f.api));
  await waitFor(() => expect(f.statsPending()).toBe(true));

  await act(async () => {
    f.resolveStats(4);
  });
  expect(result.current.connections).toBe(4);
  expect(result.current.lastConnectionAt).toBeNull();

  const before = Date.now();
  act(() => f.fireConnection());
  expect(result.current.lastConnectionAt).toBeGreaterThanOrEqual(before);
});

// The host options must survive their own filter. `flows` is server-filtered, so
// deriving the options from it would shrink them to the host already chosen and
// leave the user no way to pick another.
function hostApi(pages: FlowSummary[][]) {
  let page = 0;
  return apiStub({ listFlows: async () => pages[Math.min(page++, pages.length - 1)] });
}

function summary(id: string, uri: string): FlowSummary {
  return {
    id,
    started_at: "2026-07-12T10:00:00Z",
    duration_ms: 1,
    method: "GET",
    uri,
    status: 200,
    source: "captured",
    request_body_state: "empty",
    response_body_state: "empty",
  };
}

test("a host stays on offer after the filter narrows the list away from it", async () => {
  const api = hostApi([
    [summary("f1", "http://example.com/"), summary("f2", "http://other.test/")],
    [summary("f2", "http://other.test/")],
  ]);
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.hosts).toEqual(["example.com", "other.test"]));

  act(() => result.current.update("host", "other.test"));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  expect(result.current.hosts).toEqual(["example.com", "other.test"]);
});

test("a URI with no parseable host contributes no option", async () => {
  const api = hostApi([[summary("f1", "/relative/only"), summary("f2", "http://example.com/")]]);
  const { result } = renderHook(() => useFlows(api));

  await waitFor(() => expect(result.current.hosts).toEqual(["example.com"]));
});
