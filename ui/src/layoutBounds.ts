import tokens from "../tailwind.config.js";
import type { ColumnWidths, SavedLayout } from "./layoutStore";

export type Bounds = { min: number; max: number };

type Scale = Record<string, string>;
const theme = tokens.theme as unknown as { spacing: Scale; maxHeight: (helpers: { theme: () => Scale }) => Scale };
const spacing = theme.spacing;
const zones = theme.maxHeight({ theme: () => ({}) });

const px = (token: string) => Number.parseFloat(spacing[token]);
const share = (token: string) => Number.parseFloat(zones[token]) / 100;

export const PANE: Bounds & { initial: number } = { initial: px("pane"), min: px("pane-min"), max: px("pane-max") };
export const HEADERS_SHARE: Bounds & { initial: number } = {
  initial: share("zone-cap"),
  min: share("zone-min"),
  max: share("zone-max"),
};

export const ROW_CHROME = { inset: "px-4", gap: "gap-3" } as const;

const tokenOf = (cls: string) => cls.slice(cls.indexOf("-") + 1);
const ROW_PADDING = 2 * px(tokenOf(ROW_CHROME.inset));
const CELL_GAP = px(tokenOf(ROW_CHROME.gap));
const PATH_MIN = px("path-min");
const SPLIT_LINE = 1;

// The time column holds the cross-day timestamp an import produces (115.2px)
// plus the source mark's reserved 16px box; the mark never enters the arithmetic.
export const DEFAULT_COLUMNS: ColumnWidths = { time: 160, method: 70, host: 180, status: 70, size: 90 };

export const DEFAULT_LAYOUT: SavedLayout = {
  pane: PANE.initial,
  headersShare: HEADERS_SHARE.initial,
  columns: DEFAULT_COLUMNS,
};

export function columnTemplate({ time, method, host, status, size }: ColumnWidths): string {
  return `${time}px ${method}px ${host}px 1fr ${status}px ${size}px`;
}

export function clamp(value: number, { min, max }: Bounds): number {
  return Math.min(Math.max(value, min), max);
}

export function listMin(columns: ColumnWidths): number {
  const tracks = Object.values(columns);
  const fixed = tracks.reduce((sum, width) => sum + width, 0);
  return fixed + ROW_PADDING + tracks.length * CELL_GAP + PATH_MIN;
}

export function paneBounds(windowWidth: number, columns: ColumnWidths): Bounds {
  const roomLeft = windowWidth - SPLIT_LINE - listMin(columns);
  return { min: PANE.min, max: Math.max(PANE.min, Math.min(PANE.max, roomLeft)) };
}
