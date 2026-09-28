import { useState } from "react";

// What a triggered action is doing, for every control that runs one: a native
// command, a dialog the shell owns, a call to the core. "nothing" is the
// outcome a user chose — a cancelled save, a refused confirm — which is neither
// a success nor a failure and must not read as either.
export type ActionState = "idle" | "running" | "done" | "failed" | "nothing";

// Why an action failed, as the shell splits it: `refused` means the user was
// asked and said no — the prompt was dismissed, the authorization denied — so
// nothing on the machine changed and the screen owes a sentence rather than the
// tool's text. `unshowable` means the OS needed an approval it could not show,
// so the user was never asked and may not be told they said no. `detail` is the
// tool's text, and it stays reachable either way.
export interface ActionFailure {
  refused: boolean;
  unshowable: boolean;
  detail: string;
}

// A rejection crosses the IPC boundary as whatever the command returned, so the
// structured failure arrives as a plain object and everything else — a thrown
// Error, a string from the core's client — arrives as itself.
function asFailure(err: unknown): ActionFailure {
  if (typeof err === "object" && err !== null && "detail" in err) {
    const { refused, unshowable, detail } = err as {
      refused?: unknown;
      unshowable?: unknown;
      detail?: unknown;
    };
    return { refused: refused === true, unshowable: unshowable === true, detail: String(detail) };
  }
  return {
    refused: false,
    unshowable: false,
    detail: err instanceof Error ? err.message : String(err),
  };
}

// A control that runs one of these owes both halves of Button's fork D4:
// disabled while it is in flight, and a line beside it saying what happened.
// The hook is what makes the second half possible — a raw async handler passed
// as onClick loses its rejection, and the click leaves no trace at all.
export function useAction(run?: () => Promise<string | null | void>) {
  const [state, setState] = useState<ActionState>("idle");
  const [failure, setFailure] = useState<ActionFailure>();
  const [named, setNamed] = useState("");
  const trigger =
    run &&
    (() => {
      setState("running");
      setFailure(undefined);
      run().then(
        (outcome) => {
          if (typeof outcome === "string") setNamed(outcome);
          setState(outcome === null ? "nothing" : "done");
        },
        (err: unknown) => {
          console.error("setup action failed:", err);
          setState("failed");
          setFailure(asFailure(err));
        },
      );
    });
  return { state, failure, error: failure?.detail, named, trigger };
}

export type ActionHandle = ReturnType<typeof useAction>;
