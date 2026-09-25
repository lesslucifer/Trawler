import { describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import type { FlowView } from "./api";
import { apiStub } from "./apiStub";
import { useDraft } from "./useDraft";

function view(id: string, over: Partial<FlowView> = {}): FlowView {
  return {
    id,
    started_at: "2026-09-25T14:02:11Z",
    duration_ms: 5,
    method: "GET",
    uri: `https://${id}.test/`,
    status: 200,
    source: "captured",
    seed: null,
    request_http_version: "HTTP/1.1",
    response_http_version: "HTTP/1.1",
    request_headers: [],
    response_headers: [],
    request_body: { state: "empty" },
    response_body: { state: "empty" },
    ...over,
  };
}

describe("the one draft", () => {
  it("the door that made it reopens it, edits and all", () => {
    const { result } = renderHook(() => useDraft(apiStub()));
    act(() => result.current.open(view("a")));
    act(() => result.current.edit((d) => ({ ...d, url: "https://edited.test/" })));

    act(() => result.current.open(view("a")));

    expect(result.current.kept?.draft.url).toBe("https://edited.test/");
  });

  it("another flow, or new request over a seeded draft, replaces it", () => {
    const { result } = renderHook(() => useDraft(apiStub()));
    act(() => result.current.open(view("a")));
    act(() => result.current.open(view("b")));
    expect(result.current.kept?.draft.url).toBe("https://b.test/");

    act(() => result.current.open(null));
    expect(result.current.kept?.draft.seed).toBeNull();
    expect(result.current.kept?.draft.url).toBe("");
  });

  it("new request reopens a kept draft from scratch", () => {
    const { result } = renderHook(() => useDraft(apiStub()));
    act(() => result.current.open(null));
    act(() => result.current.edit((d) => ({ ...d, url: "https://typed.test/" })));

    act(() => result.current.open(null));

    expect(result.current.kept?.draft.url).toBe("https://typed.test/");
  });
});

describe("the send", () => {
  it("a seeded draft goes to the replay door, and the answer lands on it", async () => {
    const sent = view("sent", { source: "sent", seed: "a" });
    const replay = vi.fn(async () => sent);
    const { result } = renderHook(() => useDraft(apiStub({ replay })));
    act(() => result.current.open(view("a")));

    await act(() => result.current.send());

    expect(replay).toHaveBeenCalledWith("a", {});
    expect(result.current.kept?.outcome).toEqual({ phase: "sent", flow: sent });
  });

  it("a draft from scratch goes to the compose door", async () => {
    const send = vi.fn(async () => view("n"));
    const { result } = renderHook(() => useDraft(apiStub({ send })));
    act(() => result.current.open(null));
    act(() => result.current.edit((d) => ({ ...d, url: "https://c.test/" })));

    await act(() => result.current.send());

    expect(send).toHaveBeenCalledWith({ method: "GET", url: "https://c.test/" });
  });

  it("is sending for exactly the in-flight window, and a refusal lands as its sentence", async () => {
    let refuse: (error: Error) => void = () => {};
    const replay = vi.fn(() => new Promise<FlowView>((_, reject) => (refuse = reject)));
    const { result } = renderHook(() => useDraft(apiStub({ replay })));
    act(() => result.current.open(view("a")));

    let done: Promise<void> = Promise.resolve();
    act(() => void (done = result.current.send()));
    expect(result.current.kept?.outcome).toEqual({ phase: "sending" });

    await act(async () => {
      refuse(new Error("no flow a in the ring"));
      await done;
    });
    expect(result.current.kept?.outcome).toEqual({ phase: "failed", reason: "no flow a in the ring" });
  });

  it("sends nothing without a URL, or while a send is in flight", async () => {
    const send = vi.fn(() => new Promise<FlowView>(() => {}));
    const { result } = renderHook(() => useDraft(apiStub({ send })));
    act(() => result.current.open(null));
    await act(() => result.current.send());
    expect(send).not.toHaveBeenCalled();

    act(() => result.current.edit((d) => ({ ...d, url: "https://c.test/" })));
    act(() => void result.current.send());
    act(() => void result.current.send());
    expect(send).toHaveBeenCalledOnce();
  });

  it("a reply for a draft that was replaced does not land on its replacement", async () => {
    let answer: (flow: FlowView) => void = () => {};
    const replay = vi.fn(() => new Promise<FlowView>((resolve) => (answer = resolve)));
    const { result } = renderHook(() => useDraft(apiStub({ replay })));
    act(() => result.current.open(view("a")));
    let done: Promise<void> = Promise.resolve();
    act(() => void (done = result.current.send()));

    act(() => result.current.open(view("b")));
    await act(async () => {
      answer(view("late"));
      await done;
    });

    expect(result.current.kept?.draft.seed?.id).toBe("b");
    expect(result.current.kept?.outcome).toEqual({ phase: "idle" });
  });
});
