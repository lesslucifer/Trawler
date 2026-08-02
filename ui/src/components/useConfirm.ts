import { useCallback, useEffect, useRef, useState } from "react";

export const CONFIRM_MS = 2000;

export type ConfirmState = "idle" | "confirming" | "error";

/**
 * The ~2 s confirm convention CopyButton and CopyText share: a transient
 * outcome that restarts on re-trigger and resets the instant the payload
 * changes, so a stale "copied" never sits beside a new value.
 */
export function useConfirm(resetKey: unknown): {
  state: ConfirmState;
  flash: (outcome: Exclude<ConfirmState, "idle">) => void;
} {
  const [state, setState] = useState<ConfirmState>("idle");
  const timer = useRef<ReturnType<typeof setTimeout>>();

  const clear = useCallback(() => clearTimeout(timer.current), []);

  useEffect(() => {
    setState("idle");
    clear();
  }, [resetKey, clear]);

  useEffect(() => clear, [clear]);

  const flash = useCallback(
    (outcome: Exclude<ConfirmState, "idle">) => {
      clear();
      setState(outcome);
      timer.current = setTimeout(() => setState("idle"), CONFIRM_MS);
    },
    [clear],
  );

  return { state, flash };
}
