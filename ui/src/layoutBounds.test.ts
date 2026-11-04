import {
  COLUMN_BOUNDS,
  DEFAULT_COLUMNS,
  DEFAULT_LAYOUT,
  HEADERS_SHARE,
  PANE,
  clamp,
  columnTemplate,
  listMin,
  paneBounds,
  restorableColumns,
} from "./layoutBounds";

describe("layoutBounds", () => {
  it("reads every bound from the token config", () => {
    expect(PANE).toEqual({ initial: 520, min: 384, max: 960 });
    expect(HEADERS_SHARE).toEqual({ initial: 0.35, min: 0.15, max: 0.6 });
    expect(DEFAULT_LAYOUT).toEqual({ pane: 520, headersShare: 0.35, columns: DEFAULT_COLUMNS });
  });

  it("reads each column's default and bounds from its tokens", () => {
    expect(COLUMN_BOUNDS).toEqual({
      time: { initial: 160, min: 136, max: 240 },
      method: { initial: 70, min: 56, max: 112 },
      host: { initial: 180, min: 96, max: 480 },
      status: { initial: 70, min: 48, max: 112 },
      duration: { initial: 90, min: 64, max: 160 },
    });
    expect(DEFAULT_COLUMNS).toEqual({ time: 160, method: 70, host: 180, status: 70, duration: 90 });
  });

  it("lays the path out as the remainder, never under path-min", () => {
    expect(columnTemplate(DEFAULT_COLUMNS)).toBe("160px 70px 180px minmax(128px, 1fr) 70px 90px");
  });

  it("clamps each restored column to its own bounds", () => {
    expect(restorableColumns({ time: 0, method: 70, host: 9999, status: 112, duration: 63.5 })).toEqual({
      time: 136,
      method: 70,
      host: 480,
      status: 112,
      duration: 64,
    });
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
