import { createContext, useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  LOG_LINES,
  onCoreLog,
  onCoreState,
  readCoreLog,
  readCoreStatus,
  restartCore,
  type CoreState,
  type Invoke,
  type Listen,
  type LogEntry,
} from "./coreService";

export interface CoreBridge {
  call: Invoke;
  on: Listen;
}

export const tauriBridge: CoreBridge = { call: invoke, on: listen };

export interface CoreService {
  state: CoreState | null;
  restarting: boolean;
  refusal: string | null;
  restart: () => Promise<void>;
}

export const CoreServiceContext = createContext<CoreService | null>(null);

function released(subscription: Promise<UnlistenFn | undefined>) {
  void subscription.then((unlisten) => unlisten?.());
}

export function useCoreService(bridge: CoreBridge = tauriBridge): CoreService {
  const [state, setState] = useState<CoreState | null>(null);
  const [restarting, setRestarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    let heard = false;
    const subscription = onCoreState((next) => {
      heard = true;
      if (live) setState(next);
    }, bridge.on).catch(() => undefined);
    void readCoreStatus(bridge.call).then((read) => {
      if (live && !heard) setState(read);
    });
    return () => {
      live = false;
      released(subscription);
    };
  }, [bridge]);

  const restart = useCallback(async () => {
    setRestarting(true);
    setRefusal(null);
    try {
      setState(await restartCore(bridge.call));
    } catch (err) {
      setRefusal(err instanceof Error ? err.message : String(err));
    } finally {
      setRestarting(false);
    }
  }, [bridge]);

  return { state, restarting, refusal, restart };
}

export function mergeLog(kept: LogEntry[], incoming: LogEntry[], cap: number = LOG_LINES): LogEntry[] {
  let last = kept.length > 0 ? kept[kept.length - 1].seq : -1;
  const inOrder = incoming.every((entry) => {
    const after = entry.seq > last;
    last = entry.seq;
    return after;
  });
  const all = inOrder
    ? [...kept, ...incoming]
    : [...new Map([...kept, ...incoming].map((entry) => [entry.seq, entry])).values()].sort(
        (a, b) => a.seq - b.seq,
      );
  return all.length > cap ? all.slice(-cap) : all;
}

export function useCoreLog(bridge: CoreBridge = tauriBridge): LogEntry[] {
  const [entries, setEntries] = useState<LogEntry[]>([]);

  useEffect(() => {
    let live = true;
    const add = (incoming: LogEntry[]) => {
      if (live) setEntries((kept) => mergeLog(kept, incoming));
    };
    const subscription = onCoreLog((entry) => add([entry]), bridge.on).catch(() => undefined);
    void readCoreLog(bridge.call).then(add);
    return () => {
      live = false;
      released(subscription);
    };
  }, [bridge]);

  return entries;
}
