import { createContext, useCallback, useEffect, useRef, useState } from "react";
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

// Tauri registers a listener in an async command but serves a sync one inline,
// so a snapshot read sent beside the listen can be answered first, and an event
// emitted in between reaches neither. The read waits for the registration.
function released(subscription: Promise<UnlistenFn | undefined>) {
  void subscription.then((unlisten) => unlisten?.());
}

export function useCoreService(bridge: CoreBridge = tauriBridge): CoreService {
  const [state, setState] = useState<CoreState | null>(null);
  const [restarting, setRestarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const updates = useRef(0);

  useEffect(() => {
    let live = true;
    const subscription = onCoreState((next) => {
      updates.current += 1;
      if (!live) return;
      setState(next);
      if (next.state === "running") setRefusal(null);
    }, bridge.on).catch(() => undefined);
    void subscription
      .then(() => readCoreStatus(bridge.call))
      .then((read) => {
        if (live && updates.current === 0) setState(read);
      });
    return () => {
      live = false;
      released(subscription);
    };
  }, [bridge]);

  const restart = useCallback(async () => {
    setRestarting(true);
    setRefusal(null);
    const updatesBefore = updates.current;
    try {
      const restarted = await restartCore(bridge.call);
      if (updates.current === updatesBefore) {
        updates.current += 1;
        setState(restarted);
      }
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
    void subscription.then(() => readCoreLog(bridge.call)).then(add);
    return () => {
      live = false;
      released(subscription);
    };
  }, [bridge]);

  return entries;
}
