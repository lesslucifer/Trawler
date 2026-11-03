import { DEFAULT_COLUMNS, DEFAULT_LAYOUT, HEADERS_SHARE, PANE, clamp, listMin, paneBounds } from "./layoutBounds";

describe("layoutBounds", () => {
  it("reads every bound from the token config", () => {
    expect(PANE).toEqual({ initial: 520, min: 384, max: 960 });
    expect(HEADERS_SHARE).toEqual({ initial: 0.35, min: 0.15, max: 0.6 });
    expect(DEFAULT_LAYOUT).toEqual({ pane: 520, headersShare: 0.35, columns: DEFAULT_COLUMNS });
  });

  it("derives the list's minimum from its tracks, paddings, gaps and path-min", () => {
    expect(listMin(DEFAULT_COLUMNS)).toBe(790);
    expect(listMin({ ...DEFAULT_COLUMNS, host: 480 })).toBe(1090);
  });

  it.each([
    [1440, 649],
    [1311, 520],
    [1280, 489],
    [1132, 384],
    [850, 384],
  ])("at a %i px window splitter A reaches %i px", (window, max) => {
    expect(paneBounds(window, DEFAULT_COLUMNS)).toEqual({ min: 384, max });
  });

  it("never lets the live maximum pass pane-max on a wide window", () => {
    expect(paneBounds(4000, DEFAULT_COLUMNS).max).toBe(960);
  });

  it("lowers splitter A's maximum when a column widens", () => {
    expect(paneBounds(1440, { ...DEFAULT_COLUMNS, host: 480 }).max).toBe(384);
  });

  it("clamps into the bounds and leaves a value inside them alone", () => {
    expect(clamp(10, { min: 20, max: 30 })).toBe(20);
    expect(clamp(40, { min: 20, max: 30 })).toBe(30);
    expect(clamp(25, { min: 20, max: 30 })).toBe(25);
  });
});
