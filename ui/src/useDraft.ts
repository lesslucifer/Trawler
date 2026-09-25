import { useCallback, useRef, useState } from "react";
import type { FlowView, TrawlerApi } from "./api";
import { blankDraft, canSend, draftFrom, toRequest, type Draft } from "./draft";

export type Outcome =
  | { phase: "idle" }
  | { phase: "sending" }
  | { phase: "sent"; flow: FlowView }
  | { phase: "failed"; reason: string };

export interface Kept {
  serial: number;
  draft: Draft;
  outcome: Outcome;
}

const IDLE: Outcome = { phase: "idle" };

// One draft for the app, kept until another replaces it or the app quits
// (epic decision 8). It lives above the composer, so a reply that lands after
// the composer closed still lands on the draft that sent it — and the serial is
// what stops it landing on a draft that has since replaced that one.
export function useDraft(api: TrawlerApi) {
  const [kept, setKept] = useState<Kept | null>(null);
  const serial = useRef(0);

  const open = useCallback(
    (seed: FlowView | null) => {
      const same = kept && (seed ? kept.draft.seed?.id === seed.id : kept.draft.seed === null);
      if (same) return;
      serial.current += 1;
      setKept({ serial: serial.current, draft: seed ? draftFrom(seed) : blankDraft(), outcome: IDLE });
    },
    [kept],
  );

  const edit = useCallback((update: (draft: Draft) => Draft) => {
    setKept((k) => k && { ...k, draft: update(k.draft) });
  }, []);

  const send = useCallback(async () => {
    if (!kept || kept.outcome.phase === "sending" || !canSend(kept.draft)) return;
    const mine = kept.serial;
    const land = (outcome: Outcome) => setKept((k) => (k && k.serial === mine ? { ...k, outcome } : k));
    const { seedId, fields } = toRequest(kept.draft);
    land({ phase: "sending" });
    try {
      land({ phase: "sent", flow: seedId === null ? await api.send(fields) : await api.replay(seedId, fields) });
    } catch (error) {
      land({ phase: "failed", reason: error instanceof Error ? error.message : String(error) });
    }
  }, [api, kept]);

  return { kept, seedId: kept?.draft.seed?.id ?? null, open, edit, send };
}
