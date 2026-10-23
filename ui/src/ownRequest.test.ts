import { afterEach, describe, expect, it, vi } from "vitest";
import type { FlowSummary } from "./api";
import {
  deviceMark,
  deviceTraffic,
  isOwnRequest,
  makeRequest,
  markedUrl,
  nextClearSpans,
  nextMark,
  ownRequestProvedAt,
  REQUEST_HOST,
  requestRoute,
} from "./ownRequest";
import { NO_TRAFFIC } from "./verdicts";

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

// D9: the route is what the core saw of the request, never the page's answer alone.
describe("which way the request went", () => {
  const sentAt = Date.parse("2026-09-30T07:02:00Z");
  const seen = { decrypted: false, tunnelAt: null, sentAt, answered: true };

  it("went through Trawler when its marked row was decrypted, whatever else was seen", () => {
    expect(requestRoute({ ...seen, decrypted: true, tunnelAt: sentAt, answered: false })).toBe("through");
  });

  it("reached Trawler and did not decrypt when a tunnel to its host opened after it was sent", () => {
    expect(requestRoute({ ...seen, tunnelAt: sentAt })).toBe("undecrypted");
    expect(requestRoute({ ...seen, tunnelAt: sentAt + 5, answered: false })).toBe("undecrypted");
  });

  it("went around Trawler when the page answered and the only tunnel is older than the request", () => {
    expect(requestRoute(seen)).toBe("around");
    expect(requestRoute({ ...seen, tunnelAt: sentAt - 1 })).toBe("around");
  });

  it("went nowhere when nothing was seen and nothing answered", () => {
    expect(requestRoute({ ...seen, answered: false })).toBeNull();
  });
});

// DEC-63: another device's request carries a mark that only this screen shows.
describe("another device's mark", () => {
  const SESSION = Date.parse("2026-10-23T07:00:00Z");

  it("is short enough to type, and the same wherever the same session and platform ask", () => {
    const mark = deviceMark(SESSION, "mac");

    expect(mark).toMatch(/^[0-9a-z]{4}$/);
    expect(deviceMark(SESSION, "mac")).toBe(mark);
  });

  it("is another mark for another platform, or another session", () => {
    const mark = deviceMark(SESSION, "mac");

    expect(deviceMark(SESSION, "windows")).not.toBe(mark);
    expect(deviceMark(SESSION + 1, "mac")).not.toBe(mark);
  });

  it("is never one of this window's own marks", () => {
    expect(deviceMark(SESSION, "mac")).not.toContain(".");
  });
});

describe("the proof another device's marked request leaves", () => {
  const SESSION = Date.parse("2026-10-23T07:00:00Z");
  const mark = deviceMark(SESSION, "mac");
  const allowed = Date.parse("2026-10-23T07:02:00Z");
  const provedAt = (flows: FlowSummary[]) => deviceTraffic(flows, { since: allowed, mark, clear: [] }, []).provedAt;

  it("is the newest decrypted row carrying its mark since its address was allowed", () => {
    const flows = [
      row(markedUrl(mark), { started_at: "2026-10-23T07:03:00Z" }),
      row(markedUrl(mark), { started_at: "2026-10-23T07:05:00Z" }),
    ];

    expect(provedAt(flows)).toBe(Date.parse("2026-10-23T07:05:00Z"));
  });

  it("is nothing from before the address was allowed (A2)", () => {
    expect(provedAt([row(markedUrl(mark), { started_at: "2026-10-23T07:01:00Z" })])).toBeNull();
  });

  it("is never this window's own request, another platform's, or one that did not decrypt", () => {
    const flows = [
      row(markedUrl(nextMark()), { started_at: "2026-10-23T07:03:00Z" }),
      row(markedUrl(deviceMark(SESSION, "windows")), { started_at: "2026-10-23T07:03:00Z" }),
      row(`http://example.com/?trawler-check=${mark}`, { started_at: "2026-10-23T07:03:00Z" }),
    ];

    expect(provedAt(flows)).toBeNull();
  });

  it("leaves this window's own proof to this window's marks", () => {
    expect(ownRequestProvedAt([row(markedUrl(mark), { started_at: "2026-10-23T07:03:00Z" })], SESSION)).toBeNull();
  });
});

// DEC-63 rule 3: a reading of this machine's proxy says only what it says
// while it holds.
describe("the spans in which this machine's own traffic could not arrive", () => {
  it("open on a reading of off or another, and close on the next that is not", () => {
    const opened = nextClearSpans([], true, 1);
    const closed = nextClearSpans(opened, false, 5);

    expect(opened).toEqual([{ from: 1, to: null }]);
    expect(closed).toEqual([{ from: 1, to: 5 }]);
    expect(nextClearSpans(closed, true, 9)).toEqual([
      { from: 1, to: 5 },
      { from: 9, to: null },
    ]);
  });

  it("are untouched by a reading that changes nothing", () => {
    const none: readonly { from: number; to: number | null }[] = [];
    const open = [{ from: 1, to: null }];

    expect(nextClearSpans(none, false, 3)).toBe(none);
    expect(nextClearSpans(open, true, 3)).toBe(open);
  });
});

// DEC-63: what the screen may read as another device's.
describe("another device's traffic", () => {
  const SESSION = Date.parse("2026-10-23T07:00:00Z");
  const mark = deviceMark(SESSION, "mac");
  const at = (minute: number) => Date.parse(`2026-10-23T07:${String(minute).padStart(2, "0")}:00Z`);
  const decrypted = (minute: number, id = `d${minute}`) =>
    row("https://example.org/", { id, started_at: new Date(at(minute)).toISOString() });
  const allowed = { since: at(2), mark };

  it("is its marked rows whatever this machine's proxy read, and nothing unmarked while it may be arriving", () => {
    const flows = [decrypted(3), row(markedUrl(mark), { id: "m", started_at: new Date(at(4)).toISOString() })];
    const read = deviceTraffic(flows, { ...allowed, clear: [] }, [at(3)]);

    expect(read.traffic).toEqual({ ...NO_TRAFFIC, lastDecryptedAt: at(4) });
    expect(read.provedAt).toBe(at(4));
    expect(read.mixed).toBe(true);
  });

  it("is an unmarked row or a tunnel only where it arrived in a clear span", () => {
    const clear = [
      { from: at(3), to: at(5) },
      { from: at(7), to: null },
    ];
    const read = deviceTraffic([decrypted(4), decrypted(6)], { ...allowed, clear }, [at(6), at(8)]);

    expect(read.traffic.lastDecryptedAt).toBe(at(4));
    expect(read.traffic.lastTunnelAt).toBe(at(8));
    expect(read.mixed).toBe(false);
  });

  // Rule 4: no later reading moves the span a proof arrived in.
  it("keeps a proof from a clear span through a reading that failed after it", () => {
    let clear = nextClearSpans([], true, at(3));
    clear = nextClearSpans(clear, false, at(5));
    clear = nextClearSpans(clear, true, at(6));

    expect(deviceTraffic([decrypted(4)], { ...allowed, clear }, []).traffic.lastDecryptedAt).toBe(at(4));
  });

  it("is never this window's own marked request, or another platform's", () => {
    const flows = [
      row(markedUrl(nextMark()), { started_at: new Date(at(4)).toISOString() }),
      row(markedUrl(deviceMark(SESSION, "windows")), { started_at: new Date(at(4)).toISOString() }),
    ];
    const clear = [{ from: at(3), to: null }];

    expect(deviceTraffic(flows, { ...allowed, clear }, []).traffic).toEqual(NO_TRAFFIC);
  });

  it("reads the whole wire since the allow beside it, as nobody's", () => {
    const read = deviceTraffic([decrypted(1), decrypted(4)], { ...allowed, clear: [] }, [at(4)]);

    expect(read.wire.lastDecryptedAt).toBe(at(4));
    expect(read.wire.lastTunnelAt).toBe(at(4));
    expect(read.traffic).toEqual(NO_TRAFFIC);
  });

  it("is nothing at all before an address is allowed", () => {
    const flows = [decrypted(4), row(markedUrl(mark), { id: "m", started_at: new Date(at(4)).toISOString() })];
    const read = deviceTraffic(flows, { since: null, mark, clear: [{ from: at(1), to: null }] }, [at(4)]);

    expect(read).toEqual({ traffic: NO_TRAFFIC, wire: NO_TRAFFIC, provedAt: null, mixed: false });
  });
});
