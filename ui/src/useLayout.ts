import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { onLayoutReset, readLayout, writeLayout, type SavedLayout } from "./layoutStore";
import { DEFAULT_LAYOUT, HEADERS_SHARE, PANE, clamp, restorableColumns, type Track } from "./layoutBounds";

export const LAYOUT_WRITE_MS = 250;

export type LayoutStore = {
  read: () => Promise<SavedLayout | null>;
  write: (layout: SavedLayout) => Promise<void>;
  onReset: (handler: () => void) => Promise<() => void>;
};

export type LayoutControls = {
  layout: SavedLayout;
  setPane: (width: number) => void;
  setHeadersShare: (share: number) => void;
  resetPane: () => void;
  resetHeadersShare: () => void;
  setColumn: (track: Track, width: number) => void;
  resetColumn: (track: Track) => void;
  commit: () => void;
};

const shell: LayoutStore = { read: readLayout, write: writeLayout, onReset: onLayoutReset };

function restorable(saved: SavedLayout): SavedLayout {
  return {
    ...saved,
    pane: clamp(saved.pane, PANE),
    headersShare: clamp(saved.headersShare, HEADERS_SHARE),
    columns: restorableColumns(saved.columns),
  };
}

export function useLayout(store: LayoutStore = shell): LayoutControls {
  const [layout, setLayout] = useState(DEFAULT_LAYOUT);
  const latest = useRef(DEFAULT_LAYOUT);
  const touched = useRef(false);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  const storeRef = useRef(store);

  const update = useCallback((next: (current: SavedLayout) => SavedLayout) => {
    touched.current = true;
    latest.current = next(latest.current);
    setLayout(latest.current);
  }, []);

  const flush = useCallback(() => {
    if (pending.current === null) return;
    clearTimeout(pending.current);
    pending.current = null;
    storeRef.current.write(latest.current).catch(() => {});
  }, []);

  const commit = useCallback(() => {
    if (pending.current !== null) clearTimeout(pending.current);
    pending.current = setTimeout(flush, LAYOUT_WRITE_MS);
    touched.current = true;
  }, [flush]);

  useEffect(() => {
    let live = true;
    storeRef.current
      .read()
      .catch(() => null)
      .then((saved) => {
        if (!live || saved === null || touched.current) return;
        latest.current = restorable(saved);
        setLayout(latest.current);
      });
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let off: (() => void) | null = null;
    let live = true;
    storeRef.current
      .onReset(() => {
        update(() => DEFAULT_LAYOUT);
        commit();
      })
      .then((unlisten) => (live ? (off = unlisten) : unlisten()));
    return () => {
      live = false;
      off?.();
    };
  }, [update, commit]);

  useEffect(() => flush, [flush]);

  return useMemo(
    () => ({
      layout,
      setPane: (pane: number) => update((current) => ({ ...current, pane: Math.round(pane) })),
      setHeadersShare: (headersShare: number) => update((current) => ({ ...current, headersShare })),
      resetPane: () => update((current) => ({ ...current, pane: DEFAULT_LAYOUT.pane })),
      resetHeadersShare: () => update((current) => ({ ...current, headersShare: DEFAULT_LAYOUT.headersShare })),
      setColumn: (track: Track, width: number) =>
        update((current) => ({ ...current, columns: { ...current.columns, [track]: Math.round(width) } })),
      resetColumn: (track: Track) =>
        update((current) => ({ ...current, columns: { ...current.columns, [track]: DEFAULT_LAYOUT.columns[track] } })),
      commit,
    }),
    [layout, update, commit],
  );
}
