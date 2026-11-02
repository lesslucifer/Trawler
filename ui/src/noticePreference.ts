import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

export const NOTICE_KEY = {
  remote: "remote-notice",
  unread: "unread-notice",
  clear: "clear-ask",
} as const;

export type NoticeKey = (typeof NOTICE_KEY)[keyof typeof NOTICE_KEY];
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function noticeDismissed(key: NoticeKey, call: Invoke = invoke): Promise<boolean> {
  try {
    return await call<boolean>("notice_dismissed", { key });
  } catch {
    return false;
  }
}

export async function dismissNotice(key: NoticeKey, call: Invoke = invoke): Promise<void> {
  await call("dismiss_notice", { key });
}

export async function notesShown(call: Invoke = invoke): Promise<boolean> {
  try {
    return (await call<boolean>("notes_shown")) === true;
  } catch {
    return false;
  }
}

export async function showNotes(shown: boolean, call: Invoke = invoke): Promise<void> {
  await call("show_notes", { shown });
}

// A choice made before the remembered one arrives is the newer of the two.
export function useNotesShown(call: Invoke = invoke): [boolean, (shown: boolean) => void] {
  const [shown, setShown] = useState(false);
  const chosen = useRef(false);
  useEffect(() => {
    let live = true;
    void notesShown(call).then((remembered) => {
      if (live && !chosen.current) setShown(remembered);
    });
    return () => {
      live = false;
    };
  }, [call]);
  const choose = useCallback(
    (next: boolean) => {
      chosen.current = true;
      setShown(next);
      void showNotes(next, call).catch(() => undefined);
    },
    [call],
  );
  return [shown, choose];
}
