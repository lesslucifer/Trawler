import { act, renderHook, waitFor } from "@testing-library/react";
import { useFlowDetail, useFlows, type FlowsScope } from "./useFlows";
import { ApiError } from "./api";
import type { FlowSummary, FlowView, StreamHandlers, TrawlerApi } from "./api";
import { apiStub } from "./apiStub";

// A captured stream handler set, so a test can fire a live `connection`.
function fakeApi() {
  let handlers: StreamHandlers | undefined;
  const api = apiStub({
    stream: (h) => {
      handlers = h;
      return () => {};
    },
  });
  return { api, fireConnection: () => handlers?.onConnection?.() };
}

// Only a live event can say "now", which is what the setup screen's traffic
// verdict reads.
test("a live connection is stamped with its time", () => {
  const f = fakeApi();
  const { result } = renderHook(() => useFlows(f.api));
  expect(result.current.lastConnectionAt).toBeNull();

  const before = Date.now();
  act(() => f.fireConnection());
  expect(result.current.lastConnectionAt).toBeGreaterThanOrEqual(before);
});

// R7: the setup screen reads every captured row whatever the list is filtered
// to, and the list, when the user comes back to it, shows only its own rows.
describe("the setup scope reads unfiltered, and the list keeps its own rows", () => {
  const rows = () => [summary("f1", "http://example.com/"), summary("f2", "http://other.test/")];
  const scopedApi = (store = rows()) =>
    apiStub({
      listFlows: vi.fn(async (filter) => store.filter((f) => !filter.host || f.uri.includes(filter.host))),
    });

  it("an empty filter answers both scopes with one listing", async () => {
    const api = scopedApi();
    const { result } = renderHook(() => useFlows(api));
    await waitFor(() => expect(result.current.flows).toHaveLength(2));
    expect(result.current.everything).toHaveLength(2);
    expect(api.listFlows).toHaveBeenCalledTimes(1);
  });

  it("a filtered list leaves everything as it was", async () => {
    const api = scopedApi();
    const { result } = renderHook(() => useFlows(api));
    await waitFor(() => expect(result.current.everything).toHaveLength(2));
    act(() => result.current.update("host", "other.test"));
    await waitFor(() => expect(result.current.flows).toHaveLength(1));
    expect(result.current.everything).toHaveLength(2);
  });

  it("the setup scope asks unfiltered, writes everything, and leaves the list's rows", async () => {
    const store = rows();
    const api = scopedApi(store);
    let scope: FlowsScope = "list";
    const { result, rerender } = renderHook(() => useFlows(api, scope));
    act(() => result.current.update("host", "other.test"));
    await waitFor(() => expect(result.current.flows).toHaveLength(1));

    store.push(summary("f3", "https://third.test/"));
    scope = "setup";
    rerender();
    await waitFor(() => expect(result.current.everything).toHaveLength(3));
    expect(api.listFlows).toHaveBeenLastCalledWith({});
    expect(result.current.flows.map((f) => f.id)).toEqual(["f2"]);

    scope = "list";
    rerender();
    await waitFor(() => expect(api.listFlows).toHaveBeenLastCalledWith({ host: "other.test" }));
    expect(result.current.flows.map((f) => f.id)).toEqual(["f2"]);
  });
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
    seed: null,
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

// A listing that fails has two causes and the shell says a different sentence for
// each, so the hook must not flatten them into "something went wrong".
function feedApi(answer: () => Promise<FlowSummary[]>) {
  const api: TrawlerApi = apiStub({
    listFlows: () => answer(),
    getFlow: async () => { throw new Error("unused"); },
    getBody: async () => { throw new Error("unused"); },
  });
  return api;
}

test("a core that says nothing is unreachable, and the rows it already sent survive", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const api = feedApi(() => answer());
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  answer = () => Promise.reject(new TypeError("Failed to fetch"));
  act(() => result.current.update("q", "x"));
  await waitFor(() => expect(result.current.silent).toBe(true));

  // Discarding evidence on a transient failure is the worse error — the band
  // says the rows are stale, it does not delete them.
  expect(result.current.flows).toHaveLength(1);
});

test("a refused query is rejected, names the value typed, and takes the rows with it", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const api = feedApi(() => answer());
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(result.current.refused).toBe("4xz"));

  // The old rows are not an answer to the query that was refused.
  expect(result.current.flows).toEqual([]);
});

test("a listing that arrives clears whichever failure preceded it", async () => {
  let answer: () => Promise<FlowSummary[]> = () => Promise.reject(new TypeError("dead"));
  const api = feedApi(() => answer());
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.silent).toBe(true));

  answer = async () => [summary("f1", "http://example.com/")];
  await act(async () => {
    await result.current.retry();
  });
  expect(result.current.silent).toBe(false);
  expect(result.current.flows).toHaveLength(1);
});

test("answered counts the listings that came back, so an empty answer is told from none yet", async () => {
  let answer: () => Promise<FlowSummary[]> = () => Promise.reject(new TypeError("dead"));
  const api = feedApi(() => answer());
  const { result } = renderHook(() => useFlows(api));
  expect(result.current.answered).toBe(0);
  await waitFor(() => expect(result.current.silent).toBe(true));
  expect(result.current.answered).toBe(0);

  answer = async () => [];
  await act(async () => {
    await result.current.retry();
  });
  expect(result.current.answered).toBe(1);
  expect(result.current.flows).toEqual([]);

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(result.current.refused).toBe("4xz"));
  expect(result.current.answered).toBe(1);
});

test("a retry that fails again leaves the statement standing", async () => {
  const api = feedApi(() => Promise.reject(new TypeError("still dead")));
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.silent).toBe(true));

  await act(async () => {
    await result.current.retry();
  });
  expect(result.current.silent).toBe(true);
});

test("retry is in flight for the whole fetch — fork D4's half the hook owns", async () => {
  let release: (rows: FlowSummary[]) => void = () => {};
  let answer: () => Promise<FlowSummary[]> = () => Promise.reject(new TypeError("dead"));
  const api = feedApi(() => answer());
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.silent).toBe(true));
  expect(result.current.retrying).toBe(false);

  answer = () => new Promise<FlowSummary[]>((res) => (release = res));
  let settled: Promise<void>;
  act(() => {
    settled = result.current.retry();
  });
  await waitFor(() => expect(result.current.retrying).toBe(true));

  await act(async () => {
    release([]);
    await settled;
  });
  expect(result.current.retrying).toBe(false);
});

function view(id: string): FlowView {
  return {
    ...summary(id, "http://example.com/"),
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [],
    response_headers: [],
    request_body: { state: "empty" },
    response_body: { state: "empty" },
  };
}

function detailApi(answer: () => Promise<FlowView>) {
  const api: TrawlerApi = apiStub({
    getFlow: () => answer(),
    getBody: async () => { throw new Error("unused"); },
  });
  return api;
}

// Without this the pane holds "loading this flow…" forever, which is the one
// state indistinguishable from a fetch that is merely slow.
test("a detail fetch that never answers is reported, not swallowed", async () => {
  const api = detailApi(() => Promise.reject(new TypeError("dead")));
  const { result } = renderHook(() => useFlowDetail(api, "f1"));
  await waitFor(() => expect(result.current.failed).toBe(true));
  expect(result.current.flow).toBeNull();
});

// Every failed fetch reports, including a 404 — the pane names no cause, so it
// has none to get wrong. A dropped flow deserves a better sentence than "could
// not load this flow", and the run that owns "no longer captured" will write it;
// until then the pane says something true rather than something false.
test("a dropped flow reports too, because the pane claims only that it has no flow", async () => {
  const api = detailApi(() => Promise.reject(new ApiError(404, "gone")));
  const { result } = renderHook(() => useFlowDetail(api, "f1"));
  await waitFor(() => expect(result.current.failed).toBe(true));
  expect(result.current.flow).toBeNull();
});

test("a bumped nonce re-runs the detail fetch, so the shell's retry covers the pane", async () => {
  let answer: () => Promise<FlowView> = () => Promise.reject(new TypeError("dead"));
  const api = detailApi(() => answer());
  const { result, rerender } = renderHook(
    ({ nonce }: { nonce: number }) => useFlowDetail(api, "f1", nonce),
    { initialProps: { nonce: 0 } },
  );
  await waitFor(() => expect(result.current.failed).toBe(true));

  answer = async () => view("f1");
  rerender({ nonce: 1 });
  await waitFor(() => expect(result.current.flow).not.toBeNull());
  expect(result.current.failed).toBe(false);
});

// The one signal that arrives with no user act. Without it a core that dies
// under an open window is discovered only at the next keystroke, and the design
// says the band appears while the user does nothing at all.
function streamApi() {
  let handlers: StreamHandlers | undefined;
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const api: TrawlerApi = apiStub({
    listFlows: () => answer(),
    getFlow: async () => { throw new Error("unused"); },
    getBody: async () => { throw new Error("unused"); },
    stream: (h) => {
      handlers = h;
      return () => {};
    },
  });
  return {
    api,
    fireDown: () => handlers?.onDown?.(),
    fireUp: () => handlers?.onUp?.(),
    setAnswer: (a: () => Promise<FlowSummary[]>) => (answer = a),
  };
}

test("a dropped stream states the core is unreachable without waiting for a keystroke", async () => {
  const f = streamApi();
  const { result } = renderHook(() => useFlows(f.api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));
  expect(result.current.silent).toBe(false);

  act(() => f.fireDown());
  expect(result.current.silent).toBe(true);
  // The rows it already sent were true when they arrived.
  expect(result.current.flows).toHaveLength(1);
});

test("a stream that comes back re-asks, and a listing that answers clears the statement", async () => {
  const f = streamApi();
  const { result } = renderHook(() => useFlows(f.api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  act(() => f.fireDown());
  expect(result.current.silent).toBe(true);

  f.setAnswer(async () => [summary("f1", "http://example.com/"), summary("f2", "http://b.test/")]);
  act(() => f.fireUp());
  await waitFor(() => expect(result.current.silent).toBe(false));
  expect(result.current.flows).toHaveLength(2);
});

// Regression (review round 1, correctness): refresh had no sequencing guard, so
// a straggler could write state on top of a newer answer. The visible cost is a
// retry that appears to work and then silently un-works itself.
test("a straggling refresh cannot overwrite the answer a later one already gave", async () => {
  const pending: { resolve: (rows: FlowSummary[]) => void; reject: (e: unknown) => void }[] = [];
  const api: TrawlerApi = apiStub({
    listFlows: () =>
      new Promise<FlowSummary[]>((resolve, reject) => pending.push({ resolve, reject })),
    getFlow: async () => { throw new Error("unused"); },
    getBody: async () => { throw new Error("unused"); },
  });
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(pending).toHaveLength(1)); // the mount's own refresh

  // A second, later refresh starts and succeeds while the first is still out.
  let second: Promise<void>;
  act(() => {
    second = result.current.retry();
  });
  await waitFor(() => expect(pending).toHaveLength(2));

  await act(async () => {
    pending[1].resolve([summary("f1", "http://example.com/")]);
    await second;
  });
  expect(result.current.silent).toBe(false);
  expect(result.current.flows).toHaveLength(1);

  // The straggler now fails. It is answering a question already answered.
  await act(async () => {
    pending[0].reject(new TypeError("Failed to fetch"));
  });
  expect(result.current.silent).toBe(false);
  expect(result.current.flows).toHaveLength(1);
});

// Regression (review rounds 3 and 4): the refusal and the silence are two facts.
// Round 3 kept the refusal by refusing to record the silence, which lost a real
// outage; both are now recorded, and precedence is the renderer's business.
test("a dropped stream records the silence without unsaying the refusal", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const f = streamApi();
  const api: TrawlerApi = apiStub({ ...f.api, listFlows: () => answer() });
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(result.current.refused).toBe("4xz"));

  act(() => f.fireDown());
  expect(result.current.refused).toBe("4xz");
  expect(result.current.silent).toBe(true);
});

// Regression (epic gate): a refusal is about the value that was asked. When the
// box moves on and the request for the NEW value comes back with no answer, the
// old refusal describes nothing a reader can see — the row region would quote a
// value the box no longer holds and the core never judged.
test("a refusal does not outlive the value it was about", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const f = streamApi();
  const api: TrawlerApi = apiStub({ ...f.api, listFlows: () => answer() });
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(result.current.refused).toBe("4xz"));

  answer = () => Promise.reject(new TypeError("dead socket"));
  act(() => result.current.update("status", "200"));
  await waitFor(() => expect(result.current.silent).toBe(true));
  expect(result.current.refused).toBeNull();
});

// Its pair: asking the SAME refused value again and getting no answer teaches
// nothing new about it, so the refusal stands rather than flickering off on a
// transient failure.
test("a silence on the refused value itself leaves the refusal standing", async () => {
  let answer: () => Promise<FlowSummary[]> = async () => [summary("f1", "http://example.com/")];
  const f = streamApi();
  const api: TrawlerApi = apiStub({ ...f.api, listFlows: () => answer() });
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  answer = () => Promise.reject(new ApiError(400, "GET /flows failed: 400"));
  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(result.current.refused).toBe("4xz"));

  answer = () => Promise.reject(new TypeError("dead socket"));
  act(() => result.current.update("q", "x"));
  await waitFor(() => expect(result.current.silent).toBe(true));
  expect(result.current.refused).toBe("4xz");
});

// A listing that arrives is the only thing that ends the silence — the stream
// coming back is a reason to ask again, not an answer in itself.
test("silence ends when a listing arrives, not when the stream reconnects", async () => {
  const f = streamApi();
  const { result } = renderHook(() => useFlows(f.api));
  await waitFor(() => expect(result.current.flows).toHaveLength(1));

  act(() => f.fireDown());
  expect(result.current.silent).toBe(true);

  f.setAnswer(() => Promise.reject(new TypeError("still dead")));
  act(() => f.fireUp());
  await waitFor(() => expect(result.current.silent).toBe(true));

  f.setAnswer(async () => [summary("f1", "http://example.com/")]);
  act(() => f.fireUp());
  await waitFor(() => expect(result.current.silent).toBe(false));
});

// Regression (review round 5): the refusal named whatever the box held when the
// answer arrived, not what was asked. Edit inside the debounce window and the
// core is quoted refusing a value it never saw — or, with the box cleared, the
// sentence becomes " is not a status".
test("a refusal names the value that was asked, not whatever the box holds now", async () => {
  const calls: { status: string; reject: (e: unknown) => void }[] = [];
  const api: TrawlerApi = apiStub({
    listFlows: (filter) =>
      new Promise<FlowSummary[]>((_, reject) =>
        calls.push({ status: filter.status ?? "", reject }),
      ),
    getFlow: async () => { throw new Error("unused"); },
    getBody: async () => { throw new Error("unused"); },
  });
  const { result } = renderHook(() => useFlows(api));
  await waitFor(() => expect(calls).toHaveLength(1)); // the mount's own listing

  act(() => result.current.update("status", "4xz"));
  await waitFor(() => expect(calls).toHaveLength(2));
  expect(calls[1].status).toBe("4xz");

  // The box changes while that listing is still out, and before the debounce
  // has dispatched anything for the new value.
  act(() => result.current.update("status", ""));

  await act(async () => {
    calls[1].reject(new ApiError(400, "GET /flows failed: 400"));
  });
  expect(result.current.refused).toBe("4xz");
});
