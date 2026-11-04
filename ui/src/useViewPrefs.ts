import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DEFAULT_VIEW_PREFS, readViewPrefs, writeViewPrefs, type ViewPrefs } from "./viewPrefsStore";

export const VIEW_PREFS_WRITE_MS = 250;

export type ViewPrefsStore = {
  read: () => Promise<Partial<ViewPrefs>>;
  write: (prefs: ViewPrefs) => Promise<void>;
};

export type ViewPrefsControls = {
  prefs: ViewPrefs;
  set: <K extends keyof ViewPrefs>(field: K, value: ViewPrefs[K]) => void;
};

const shell: ViewPrefsStore = { read: readViewPrefs, write: writeViewPrefs };

export function useViewPrefs(store: ViewPrefsStore = shell): ViewPrefsControls {
  const [prefs, setPrefs] = useState(DEFAULT_VIEW_PREFS);
  const latest = useRef(DEFAULT_VIEW_PREFS);
  const touched = useRef(new Set<keyof ViewPrefs>());
  const settled = useRef(false);
  const owed = useRef(false);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  const storeRef = useRef(store);

  const flush = useCallback(() => {
    if (pending.current !== null) clearTimeout(pending.current);
    pending.current = null;
    if (!settled.current) return;
    owed.current = false;
    storeRef.current.write(latest.current).catch(() => {});
  }, []);

  const set = useCallback(
    <K extends keyof ViewPrefs>(field: K, value: ViewPrefs[K]) => {
      if (latest.current[field] === value) return;
      touched.current.add(field);
      latest.current = { ...latest.current, [field]: value };
      setPrefs(latest.current);
      owed.current = true;
      if (pending.current !== null) clearTimeout(pending.current);
      pending.current = setTimeout(flush, VIEW_PREFS_WRITE_MS);
    },
    [flush],
  );

  useEffect(() => {
    let live = true;
    storeRef.current
      .read()
      .catch(() => ({}))
      .then((saved: Partial<ViewPrefs>) => {
        settled.current = true;
        if (!live) return;
        const untouched = Object.entries(saved).filter(([field]) => !touched.current.has(field as keyof ViewPrefs));
        if (untouched.length > 0) {
          latest.current = { ...latest.current, ...Object.fromEntries(untouched) };
          setPrefs(latest.current);
        }
        if (owed.current && pending.current === null) flush();
      });
    return () => {
      live = false;
    };
  }, [flush]);

  useEffect(
    () => () => {
      if (owed.current) flush();
    },
    [flush],
  );

  return useMemo(() => ({ prefs, set }), [prefs, set]);
}
