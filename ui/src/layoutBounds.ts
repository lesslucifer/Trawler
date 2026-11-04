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

export type Track = keyof ColumnWidths;
type Sized = Bounds & { initial: number };

export const TRACKS: readonly Track[] = ["time", "method", "host", "status", "duration"];

const trackBounds = (track: Track): Sized => ({
  initial: px(`col-${track}`),
  min: px(`col-${track}-min`),
  max: px(`col-${track}-max`),
});

const perTrack = <T>(of: (track: Track) => T) =>
  Object.fromEntries(TRACKS.map((track) => [track, of(track)])) as Record<Track, T>;

export const COLUMN_BOUNDS = perTrack(trackBounds);

export const DEFAULT_COLUMNS: ColumnWidths = perTrack((track) => COLUMN_BOUNDS[track].initial);

export function restorableColumns(saved: ColumnWidths): ColumnWidths {
  return perTrack((track) => clamp(saved[track], COLUMN_BOUNDS[track]));
}

export const DEFAULT_LAYOUT: SavedLayout = {
  pane: PANE.initial,
  headersShare: HEADERS_SHARE.initial,
  columns: DEFAULT_COLUMNS,
};

export function columnTemplate({ time, method, host, status, duration }: ColumnWidths): string {
  return `${time}px ${method}px ${host}px minmax(${PATH_MIN}px, 1fr) ${status}px ${duration}px`;
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
