import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import moment from "moment";
import { TimestampText } from "./TimestampText";

const NOW = new Date(2026, 6, 22, 15, 0, 0);

const shown = (timestamp: string | number | Date | null, format?: string) =>
  render(<TimestampText timestamp={timestamp} format={format} />).container.textContent;

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});

afterEach(() => vi.useRealTimers());

const localToday = (h: number, m: number, s: number, ms = 0) =>
  new Date(2026, 6, 22, h, m, s, ms);

describe("TimestampText", () => {
  it("renders same-day local wall-clock as HH:MM:SS", () => {
    expect(shown(localToday(14, 7, 52, 381))).toBe("14:07:52");
  });

  it("truncates sub-second rather than rounding", () => {
    expect(shown(localToday(14, 7, 52, 999))).toBe("14:07:52");
  });

  it("accepts an ISO string, epoch ms and a Date alike — the caller passes raw", () => {
    const at = localToday(14, 7, 52);
    expect(shown(at.toISOString())).toBe("14:07:52");
    expect(shown(at.getTime())).toBe("14:07:52");
    expect(shown(at)).toBe("14:07:52");
  });

  it("prefixes the date only when it is not today", () => {
    expect(shown(new Date(2026, 6, 21, 23, 14, 5))).toBe("21/07 · 23:14:05");
    expect(shown(new Date(2026, 5, 1, 8, 0, 0))).toBe("01/06 · 08:00:00");
  });

  it("appends the year only when it is not the current one", () => {
    expect(shown(new Date(2025, 11, 31, 8, 0, 0))).toBe("31/12/2025 · 08:00:00");
  });

  it("guards null and unparseable values with an em-dash, keeping the slot", () => {
    expect(shown(null)).toBe("—");
    expect(shown("not a date")).toBe("—");
  });

  it("a custom format replaces the default logic entirely, still local", () => {
    expect(shown(localToday(14, 7, 52, 381), "HH:mm:ss.SSS")).toBe("14:07:52.381");
    expect(shown(localToday(14, 7, 52), "hh:mm:ss A")).toBe("02:07:52 PM");
  });

  it("a cross-day custom format carries no automatic prefix", () => {
    expect(shown(new Date(2026, 6, 21, 14, 7, 0), "HH:mm")).toBe("14:07");
  });

  it("relative mode renders fromNow and re-renders on its own 10s tick", () => {
    const view = render(<TimestampText timestamp={moment(NOW).subtract(3, "minutes").toDate()} format="relative" />);
    expect(view.container.textContent).toBe("3 minutes ago");

    act(() => {
      vi.setSystemTime(new Date(NOW.getTime() + 5 * 60_000));
      vi.advanceTimersByTime(10_000);
    });
    expect(view.container.textContent).toBe("8 minutes ago");
  });

  it("is always local and never UTC", () => {
    const at = localToday(14, 7, 52);
    expect(shown(at)).toBe(moment(at).format("HH:mm:ss"));
  });

  it("bounds today exactly — local midnight is today, the millisecond before it is not", () => {
    expect(shown(localToday(0, 0, 0))).toBe("00:00:00");
    expect(shown(new Date(2026, 6, 21, 23, 59, 59, 999))).toBe("21/07 · 23:59:59");
  });

  it("bounds the year exactly — its first instant carries no year, the one before it does", () => {
    expect(shown(new Date(2026, 0, 1, 0, 0, 0))).toBe("01/01 · 00:00:00");
    expect(shown(new Date(2025, 11, 31, 23, 59, 59, 999))).toBe("31/12/2025 · 23:59:59");
  });

  // The same timestamp reads differently on either side of midnight, so anything
  // the component remembers about it must not survive the crossing.
  it("re-labels a timestamp once the clock leaves the day it was rendered under", () => {
    const at = localToday(14, 7, 52);
    expect(shown(at)).toBe("14:07:52");

    vi.setSystemTime(new Date(2026, 6, 23, 0, 30, 0));
    expect(shown(at)).toBe("22/07 · 14:07:52");
  });

  it("is muted mono with tabular figures for a stable column", () => {
    const { container } = render(<TimestampText timestamp={localToday(14, 7, 52)} />);
    const span = container.querySelector("span");
    expect(span?.className).toContain("text-ink-mute");
    expect(span?.className).toContain("tabular-nums");
  });
});
