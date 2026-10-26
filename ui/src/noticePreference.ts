import { invoke } from "@tauri-apps/api/core";

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
