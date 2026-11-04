import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ColumnWidths = {
  time: number;
  method: number;
  host: number;
  status: number;
  duration: number;
};

export type SavedLayout = {
  pane: number;
  headersShare: number;
  columns: ColumnWidths;
};

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export async function readLayout(call: Invoke = invoke): Promise<SavedLayout | null> {
  try {
    return (await call<SavedLayout | null>("layout_read")) ?? null;
  } catch {
    return null;
  }
}

export async function writeLayout(layout: SavedLayout, call: Invoke = invoke): Promise<void> {
  await call("layout_write", { layout });
}

export const RESET_LAYOUT_EVENT = "layout-reset";

type Unlisten = () => void;
type Listen = (event: string, handler: () => void) => Promise<Unlisten>;

export async function onLayoutReset(handler: () => void, subscribe: Listen = listen): Promise<Unlisten> {
  try {
    return await subscribe(RESET_LAYOUT_EVENT, handler);
  } catch {
    return () => {};
  }
}
