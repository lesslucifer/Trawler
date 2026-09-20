import { useState } from "react";

// What a triggered action is doing, for every control that runs one: a native
// command, a dialog the shell owns, a call to the core. "nothing" is the
// outcome a user chose — a cancelled save, a refused confirm — which is neither
// a success nor a failure and must not read as either.
export type ActionState = "idle" | "running" | "done" | "failed" | "nothing";

// A control that runs one of these owes both halves of Button's fork D4:
// disabled while it is in flight, and a line beside it saying what happened.
// The hook is what makes the second half possible — a raw async handler passed
// as onClick loses its rejection, and the click leaves no trace at all.
export function useAction(run?: () => Promise<string | null | void>) {
  const [state, setState] = useState<ActionState>("idle");
  const [error, setError] = useState<string>();
  const [named, setNamed] = useState("");
  const trigger =
    run &&
    (() => {
      setState("running");
      setError(undefined);
      run().then(
        (outcome) => {
          if (typeof outcome === "string") setNamed(outcome);
          setState(outcome === null ? "nothing" : "done");
        },
        (err: unknown) => {
          console.error("setup action failed:", err);
          setState("failed");
          setError(err instanceof Error ? err.message : String(err));
        },
      );
    });
  return { state, error, named, trigger };
}

export type ActionHandle = ReturnType<typeof useAction>;
