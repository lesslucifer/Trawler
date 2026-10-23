import { afterEach, describe, expect, it, vi } from "vitest";
import type { FlowSummary } from "./api";
import { isOwnRequest, makeRequest, markedUrl, nextMark, ownRequestProvedAt, REQUEST_HOST } from "./ownRequest";

function row(uri: string, over: Partial<FlowSummary> = {}): FlowSummary {
  return {
    id: "f1",
    started_at: "2026-09-30T07:02:00Z",
    duration_ms: 5,
    method: "GET",
    uri,
    status: 200,
    source: "captured",
    seed: null,
    request_body_state: "empty",
    response_body_state: "empty",
    ...over,
  };
}

describe("the request the app makes of its own (DEC-56)", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("goes to the named host over HTTPS, marked, uncached, without asking to read the answer", async () => {
    const fetch = vi.fn(async () => new Response(null));
    vi.stubGlobal("fetch", fetch);

    await makeRequest("w1.1");

    expect(REQUEST_HOST).toBe("example.com");
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledWith("https://example.com/?trawler-check=w1.1", {
      mode: "no-cors",
      cache: "no-store",
    });
  });

  it("fails when the request does not complete, so the screen can say so", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("Load failed"))));

    await expect(makeRequest("w1.1")).rejects.toThrow("Load failed");
  });
});

// DEC-62: the mark is what makes the row this window's and no other device's.
describe("the mark", () => {
  it("is new on every request, and every one belongs to this window", () => {
    const first = nextMark();
    const second = nextMark();

    expect(first).not.toBe(second);
    expect(first.split(".")[0]).toBe(second.split(".")[0]);
  });

  it("names its own row, decrypted and captured, and no other", () => {
    const mark = nextMark();

    expect(isOwnRequest(row(markedUrl(mark)), mark)).toBe(true);
    expect(isOwnRequest(row(`https://example.com:443/?trawler-check=${mark}`), mark)).toBe(true);
    expect(isOwnRequest(row(markedUrl(nextMark())), mark)).toBe(false);
    expect(isOwnRequest(row(`http://example.com/?trawler-check=${mark}`), mark)).toBe(false);
    expect(isOwnRequest(row(`https://example.org/?trawler-check=${mark}`), mark)).toBe(false);
    expect(isOwnRequest(row(markedUrl(mark), { source: "sent" }), mark)).toBe(false);
    expect(isOwnRequest(row("not a url"), mark)).toBe(false);
  });
});

describe("the proof this window's own request leaves", () => {
  const since = Date.parse("2026-09-30T07:00:00Z");

  it("is the newest of its decrypted rows in the setup session", () => {
    const flows = [
      row(markedUrl(nextMark()), { started_at: "2026-09-30T07:01:00Z" }),
      row(markedUrl(nextMark()), { started_at: "2026-09-30T07:03:00Z" }),
    ];

    expect(ownRequestProvedAt(flows, since)).toBe(Date.parse("2026-09-30T07:03:00Z"));
  });

  it("lasts the session and no longer: a row from before it opened proves nothing", () => {
    const earlier = [row(markedUrl(nextMark()), { started_at: "2026-09-30T06:59:59Z" })];

    expect(ownRequestProvedAt(earlier, since)).toBeNull();
    expect(ownRequestProvedAt(earlier, null)).toBe(Date.parse("2026-09-30T06:59:59Z"));
  });

  // D7: a phone that happens to open the same page settles nothing here.
  it("is never another device's request, however alike it looks", () => {
    const flows = [
      row("https://example.com/"),
      row("https://example.com/?trawler-check=elsewhere.1"),
      row(markedUrl(nextMark()), { uri: "http://example.com/?trawler-check=x" }),
    ];

    expect(ownRequestProvedAt(flows, since)).toBeNull();
  });

  it("parses no row that carries no mark, since the whole list is read on every refresh", () => {
    let parsed = 0;
    vi.stubGlobal(
      "URL",
      class extends URL {
        constructor(...args: ConstructorParameters<typeof URL>) {
          super(...args);
          parsed += 1;
        }
      },
    );

    ownRequestProvedAt([row("https://example.com/"), row("https://example.org/a?b=c")], since);
    vi.unstubAllGlobals();

    expect(parsed).toBe(0);
  });
});
