import { invoke } from "@tauri-apps/api/core";

export type ColumnWidths = {
  time: number;
  method: number;
  host: number;
  status: number;
  size: number;
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
