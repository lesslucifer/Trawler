import { describe, expect, it } from "vitest";
import { looksLikeTime, time, TIME_SHAPE, type TimeRead, type TimeReading } from "./timeDecoder";

const NOW = Date.UTC(2026, 10, 5, 4, 0, 0);
const HCM = "Asia/Ho_Chi_Minh";
const NEW_YORK = "America/New_York";

function read(input: string, zone = HCM, now = NOW): TimeReading {
  const reading: TimeRead = time(input, zone, now);
  if (reading.kind === "unreadable") throw new Error(reading.reason);
  return reading;
}

const reason = (input: string) => {
  const reading = time(input, HCM, NOW);
  if (reading.kind !== "unreadable") throw new Error(`read ${input} as ${reading.local}`);
  return reading.reason;
};

describe("a Unix time", () => {
  it("is told apart by its digit count, the fraction kept to its significant digits", () => {
    expect(read("1793851200")).toMatchObject({
      local: "2026-11-05 11:00:00 +07:00",
      utc: "2026-11-05 04:00:00Z",
      zone: HCM,
      readAs: "Unix time, seconds",
      relative: "now",
    });
    expect(read("1793851200.25")).toMatchObject({ utc: "2026-11-05 04:00:00.25Z", readAs: "Unix time, seconds" });
    expect(read("1793851200250")).toMatchObject({ utc: "2026-11-05 04:00:00.25Z", readAs: "Unix time, milliseconds" });
    expect(read("1793851200000")).toMatchObject({ utc: "2026-11-05 04:00:00Z" });
    expect(read("1793851200250001")).toMatchObject({ utc: "2026-11-05 04:00:00.250001Z", readAs: "Unix time, microseconds" });
    expect(read("1793851200000000007")).toMatchObject({ utc: "2026-11-05 04:00:00.000000007Z", readAs: "Unix time, nanoseconds" });
    expect(read("946684800")).toMatchObject({ utc: "2000-01-01 00:00:00Z" });
  });

  it("says why a digit count names no unit, or a fraction follows a unit finer than seconds", () => {
    expect(reason("17938512000")).toBe(
      "11 digits is not a Unix time: seconds have 9–10, milliseconds 12–13, microseconds 15–16, nanoseconds 18–19",
    );
    expect(reason("17938512002500")).toMatch(/^14 digits is not a Unix time/);
    expect(reason("1793851200250.5")).toBe("a fraction follows only a Unix time in seconds, and this one is in milliseconds");
  });
});

describe("a date-time string", () => {
  it("reads ISO 8601 with Z or an offset, a T or a space, seconds and a fraction optional", () => {
    const at = { utc: "2026-11-05 04:00:00Z", readAs: "ISO 8601" };
    expect(read("2026-11-05T04:00:00Z")).toMatchObject({ ...at, local: "2026-11-05 11:00:00 +07:00" });
    expect(read("2026-11-05 11:00+07:00")).toMatchObject(at);
    expect(read("2026-11-04T23:00:00-0500")).toMatchObject(at);
    expect(read("2026-11-05t04:00:00z")).toMatchObject(at);
    expect(read("2026-11-05T04:00:00.123456789Z")).toMatchObject({ utc: "2026-11-05 04:00:00.123456789Z" });
    expect(read("2026-11-05T04:00:00,5Z")).toMatchObject({ utc: "2026-11-05 04:00:00.5Z" });
  });

  it("reads an HTTP date and the cookie form with dashes", () => {
    for (const input of ["Thu, 05 Nov 2026 04:00:00 GMT", "Thu, 05-Nov-2026 04:00:00 GMT", "Thu, 5 Nov 2026 04:00:00 UTC"]) {
      expect(read(input)).toMatchObject({ utc: "2026-11-05 04:00:00Z", readAs: "HTTP date" });
    }
  });

  it("reads a zone-less date-time as UTC and says so", () => {
    expect(read("2026-06-01T14:27:22")).toMatchObject({
      utc: "2026-06-01 14:27:22Z",
      local: "2026-06-01 21:27:22 +07:00",
      readAs: "ISO 8601, no zone: read as UTC",
      relative: "5 months ago",
    });
  });

  it("reads a date as the calendar day it names, never moved across zones, counted in days from today", () => {
    expect(read("2026-01-14")).toMatchObject({ local: "2026-01-14, Wednesday", utc: null, readAs: "ISO 8601 date", relative: "10 months ago" });
    expect(read("2026-01-14", "Pacific/Pago_Pago").local).toBe("2026-01-14, Wednesday");
    expect(read("2026-11-05").relative).toBe("today");
    expect(read("2026-11-04").relative).toBe("yesterday");
    expect(read("2026-11-12").relative).toBe("in 7 days");
    expect(read("2026-11-05", "Pacific/Pago_Pago").relative).toBe("tomorrow");
  });

  it("says why it cannot read a day off the calendar, an hour past 23 or an unknown month", () => {
    expect(reason("2026-02-30")).toBe("2026-02-30 is not a day of the calendar");
    expect(reason("2026-02-30T04:00:00Z")).toBe("2026-02-30 is not a day of the calendar");
    expect(reason("2026-13-01T04:00:00Z")).toBe("month 13 is not a month");
    expect(reason("2026-11-05T24:00:00Z")).toBe("24:00:00 is not a time of day");
    expect(reason("Thu, 05 Nox 2026 04:00:00 GMT")).toBe("“Nox” is not a month");
    expect(reason("next tuesday")).toBe("not a Unix time, a date or a date-time");
  });
});

describe("the local time", () => {
  it("follows the zone's offset across a daylight-saving edge", () => {
    expect(read("2026-11-01T05:30:00Z", NEW_YORK).local).toBe("2026-11-01 01:30:00 -04:00");
    expect(read("2026-11-01T06:30:00Z", NEW_YORK).local).toBe("2026-11-01 01:30:00 -05:00");
  });

  it("is the text a replace writes and the bytes it opens with", () => {
    const reading = read("1793851200");
    expect(reading.text).toBe(reading.local);
    expect(new TextDecoder().decode(reading.bytes)).toBe(reading.local);
  });
});

describe("how long ago or ahead", () => {
  it("names the largest whole unit, before and after now", () => {
    const hour = 3600;
    const at = (offsetSeconds: number) => read(String(NOW / 1000 + offsetSeconds)).relative;
    expect(at(-30)).toBe("30 seconds ago");
    expect(at(5 * 60)).toBe("in 5 minutes");
    expect(at(-3 * hour)).toBe("3 hours ago");
    expect(at(-3 * 24 * hour)).toBe("3 days ago");
    expect(at(24 * hour)).toBe("tomorrow");
    expect(at(-90 * 24 * hour)).toBe("3 months ago");
    expect(at(2 * 365 * 24 * hour)).toBe("in 2 years");
  });
});

describe("looksLikeTime and TIME_SHAPE", () => {
  it("looks like a time for a unit's digit count, a date or a date-time shape, zone or not", () => {
    for (const input of ["1793851200", " 1793851200.5 ", "1793851200250", "2026-11-05", "2026-11-05T04:00:00", "Thu, 05 Nov 2026 04:00:00 GMT"]) {
      expect(looksLikeTime(input)).toBe(true);
    }
    for (const input of ["17938512000", "1793851200250.5", "2026-11-05-report", "SGVsbG8=", "12345"]) {
      expect(looksLikeTime(input)).toBe(false);
    }
  });

  it("finds each date and date-time standing alone inside a line", () => {
    const line = '{"at": "2026-11-05T04:00:00.5+07:00", "expires": "Thu, 05 Nov 2026 04:00:00 GMT", "day": "2026-11-05", "file": "2026-11-05-r"}';
    expect(Array.from(line.matchAll(TIME_SHAPE), (match) => match[0])).toEqual([
      "2026-11-05T04:00:00.5+07:00",
      "Thu, 05 Nov 2026 04:00:00 GMT",
      "2026-11-05",
    ]);
  });
});
