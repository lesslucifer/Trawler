import { afterEach, describe, expect, it, vi } from "vitest";
import { UNREACHABLE, type FlowSummary } from "./api";
import {
  CLEAR_SPANS_MAX,
  deviceEvidenceSince,
  deviceMark,
  deviceTraffic,
  isOwnRequest,
  makeRequest,
  markedUrl,
  nextArmedSince,
  nextClearReadings,
  nextMark,
  NO_CLEAR_READINGS,
  ownRequestProvedAt,
  REQUEST_HOST,
  requestRoute,
} from "./ownRequest";
import { NO_TRAFFIC } from "./verdicts";

const APART = 10_000;

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
  const provedAt = (flows: FlowSummary[]) => deviceTraffic(flows, { allowedSince: allowed, mark, clear: NO_CLEAR_READINGS }, []).provedAt;

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

// DEC-63 rule 1: another device's evidence starts when the address it uses was
// first seen armed.
describe("when another device's evidence starts", () => {
  const armedOn = (proxy: string) => ({ ...UNREACHABLE, armed: true, proxy });
  const first = { proxy: "192.168.1.24:7420", at: 5 };

  it("is the moment an address is first seen armed", () => {
    expect(nextArmedSince(null, armedOn("192.168.1.24:7420"), 5)).toEqual(first);
  });

  it("stays put while the same address is read again", () => {
    expect(nextArmedSince(first, armedOn("192.168.1.24:7420"), 9)).toBe(first);
  });

  it("starts again for an address allowed anew", () => {
    expect(nextArmedSince(first, armedOn("10.0.4.9:7420"), 9)).toEqual({ proxy: "10.0.4.9:7420", at: 9 });
  });

  it("is gone once nothing is armed", () => {
    expect(nextArmedSince(first, UNREACHABLE, 9)).toBeNull();
  });

  it("is the later of that moment and the setup session's start", () => {
    expect(deviceEvidenceSince(10, null)).toBeNull();
    expect(deviceEvidenceSince(1, first)).toBe(5);
    expect(deviceEvidenceSince(9, first)).toBe(9);
    expect(deviceEvidenceSince(null, first)).toBe(5);
  });
});

// DEC-63 rule 3: a reading of this machine's proxy speaks for its own moment,
// somewhere between being sent and landing.
describe("the runs in which this machine's own traffic could not arrive", () => {
  const read = (sent: number, landed = sent + 1) => ({ sent, landed });

  it("run from the first reading of off or another to land to the last to be sent", () => {
    let readings = nextClearReadings(NO_CLEAR_READINGS, true, read(1), APART);
    readings = nextClearReadings(readings, true, read(5), APART);
    readings = nextClearReadings(readings, true, read(9), APART);

    expect(readings).toEqual({ spans: [{ from: 2, to: 9 }], open: true });
  });

  it("end with a reading that is not, and start anew with the next that is", () => {
    let readings = nextClearReadings(NO_CLEAR_READINGS, true, read(1), APART);
    readings = nextClearReadings(readings, true, read(5), APART);
    readings = nextClearReadings(readings, false, read(9), APART);
    readings = nextClearReadings(readings, true, read(13), APART);

    expect(readings).toEqual({
      spans: [
        { from: 2, to: 5 },
        { from: 14, to: 13 },
      ],
      open: true,
    });
  });

  it("are untouched by a reading that is not, where none is open", () => {
    expect(nextClearReadings(NO_CLEAR_READINGS, false, read(3), APART)).toBe(NO_CLEAR_READINGS);
  });

  // A window hidden, or setup left, between two readings: the proxy may have
  // pointed here and back with nobody reading it.
  it("start anew where two readings are too far apart to vouch for what came between", () => {
    const later = 5 + APART + 1;
    let readings = nextClearReadings(NO_CLEAR_READINGS, true, read(1), APART);
    readings = nextClearReadings(readings, true, read(5), APART);
    readings = nextClearReadings(readings, true, read(later), APART);

    expect(readings).toEqual({
      spans: [
        { from: 2, to: 5 },
        { from: later + 1, to: later },
      ],
      open: true,
    });
  });

  it("leave nothing behind for a run of one reading, which vouches for no time at all", () => {
    let readings = nextClearReadings(NO_CLEAR_READINGS, true, read(1), APART);
    readings = nextClearReadings(readings, false, read(5), APART);

    expect(readings).toEqual({ spans: [], open: false });
  });

  it("keep the newest runs only, however often the reading flips", () => {
    let readings = NO_CLEAR_READINGS;
    for (let run = 0; run < CLEAR_SPANS_MAX + 3; run += 1) {
      readings = nextClearReadings(readings, true, read(run * 6), APART);
      readings = nextClearReadings(readings, true, read(run * 6 + 2), APART);
      readings = nextClearReadings(readings, false, read(run * 6 + 4), APART);
    }

    expect(readings.spans).toHaveLength(CLEAR_SPANS_MAX);
    expect(readings.spans[0]).toEqual({ from: 3 * 6 + 1, to: 3 * 6 + 2 });
  });
});

// DEC-63: what the screen may read as another device's.
describe("another device's traffic", () => {
  const SESSION = Date.parse("2026-10-23T07:00:00Z");
  const mark = deviceMark(SESSION, "mac");
  const at = (second: number) => Date.parse("2026-10-23T07:02:00Z") + second * 1_000;
  const decrypted = (second: number, id = `d${second}`) =>
    row("https://example.org/", { id, started_at: new Date(at(second)).toISOString() });
  const rowAt = (startedAt: number) =>
    row("https://example.org/", { id: `r${startedAt}`, started_at: new Date(startedAt).toISOString() });
  const allowed = { allowedSince: at(2), mark };

  const readAt = (second: number) => ({ sent: at(second), landed: at(second) });
  const readings = (...runs: { clear: boolean; second: number }[]) =>
    runs.reduce((was, run) => nextClearReadings(was, run.clear, readAt(run.second), APART), NO_CLEAR_READINGS);

  it("is its marked rows whatever this machine's proxy read, and nothing unmarked while it may be arriving", () => {
    const flows = [decrypted(3), row(markedUrl(mark), { id: "m", started_at: new Date(at(4)).toISOString() })];
    const read = deviceTraffic(flows, { ...allowed, clear: NO_CLEAR_READINGS }, [at(3)]);

    expect(read.traffic).toEqual({ ...NO_TRAFFIC, lastDecryptedAt: at(4) });
    expect(read.provedAt).toBe(at(4));
    expect(read.mixed).toBe(true);
  });

  it("is an unmarked row or a tunnel only where it arrived between two readings of off or another", () => {
    const clear = readings(
      { clear: true, second: 3 },
      { clear: true, second: 5 },
      { clear: false, second: 6 },
      { clear: true, second: 7 },
      { clear: true, second: 9 },
    );
    const read = deviceTraffic([decrypted(4), decrypted(6)], { ...allowed, clear }, [at(6), at(8), at(10)]);

    expect(read.traffic.lastDecryptedAt).toBe(at(4));
    expect(read.traffic.lastTunnelAt).toBe(at(8));
    expect(read.mixed).toBe(false);
  });

  // Rule 3: the proxy may have turned to point here any time after the last
  // reading that said it did not.
  it("is never what arrived after the last reading of off, whatever the next says", () => {
    const flows = [decrypted(6)];
    const open = readings({ clear: true, second: 3 }, { clear: true, second: 5 });

    expect(deviceTraffic(flows, { ...allowed, clear: open }, []).traffic).toEqual(NO_TRAFFIC);
    const pointed = nextClearReadings(open, false, readAt(7), APART);
    expect(deviceTraffic(flows, { ...allowed, clear: pointed }, []).traffic).toEqual(NO_TRAFFIC);
    const offAgain = nextClearReadings(pointed, true, readAt(8), APART);
    expect(deviceTraffic(flows, { ...allowed, clear: offAgain }, []).traffic).toEqual(NO_TRAFFIC);
  });

  it("is never what arrived between two readings too far apart to vouch for it", () => {
    const apart = APART / 1_000 + 4;
    const clear = readings({ clear: true, second: 3 }, { clear: true, second: apart });

    expect(deviceTraffic([decrypted(4)], { ...allowed, clear }, []).traffic).toEqual(NO_TRAFFIC);
  });

  // A tunnel and the request it carries are one connection, seen at two times:
  // a reading's edge between them leaves the tunnel answered.
  it("answers a tunnel of its with the decrypted request after it on the wire, wherever a reading's edge falls", () => {
    const clear = readings({ clear: true, second: 3 }, { clear: true, second: 5 }, { clear: false, second: 6 });
    const tunnel = at(5) - 50;
    const read = deviceTraffic([rowAt(at(5) + 100)], { ...allowed, clear }, [tunnel]);

    expect(read.traffic.lastTunnelAt).toBe(tunnel);
    expect(read.traffic.openTunnelAt).toBeNull();
  });

  // Rule 4: no later reading moves or widens a run a proof arrived in.
  it("keeps a proof from a run through a reading that failed after it", () => {
    const clear = readings(
      { clear: true, second: 3 },
      { clear: true, second: 5 },
      { clear: false, second: 6 },
      { clear: true, second: 7 },
    );

    expect(deviceTraffic([decrypted(4)], { ...allowed, clear }, []).traffic.lastDecryptedAt).toBe(at(4));
  });

  it("is never this window's own marked request, or another platform's", () => {
    const flows = [
      row(markedUrl(nextMark()), { started_at: new Date(at(4)).toISOString() }),
      row(markedUrl(deviceMark(SESSION, "windows")), { started_at: new Date(at(4)).toISOString() }),
    ];
    const clear = readings({ clear: true, second: 3 }, { clear: true, second: 5 });

    expect(deviceTraffic(flows, { ...allowed, clear }, []).traffic).toEqual(NO_TRAFFIC);
  });

  it("reads the whole wire since the allow beside it, as nobody's, while this machine's own may be arriving", () => {
    const read = deviceTraffic([decrypted(1), decrypted(4)], { ...allowed, clear: NO_CLEAR_READINGS }, [at(4)]);

    expect(read.wire.lastDecryptedAt).toBe(at(4));
    expect(read.wire.lastTunnelAt).toBe(at(4));
    expect(read.traffic).toEqual(NO_TRAFFIC);
  });

  it("is nothing at all before an address is allowed", () => {
    const flows = [decrypted(4), row(markedUrl(mark), { id: "m", started_at: new Date(at(4)).toISOString() })];
    const clear = readings({ clear: true, second: 1 }, { clear: true, second: 9 });

    expect(deviceTraffic(flows, { allowedSince: null, mark, clear }, [at(4)])).toEqual({
      traffic: NO_TRAFFIC,
      wire: NO_TRAFFIC,
      provedAt: null,
      mixed: false,
    });
  });
});
