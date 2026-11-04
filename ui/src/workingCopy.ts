export interface Step {
  text: string;
  name: string;
}

export interface History {
  steps: Step[];
  at: number;
  dropped: number;
}

export const ORIGINAL_STEP = "original";
export const RESTORE_STEP = "restore original";
export const HISTORY_BUDGET = 32 * 1024 * 1024;

export interface DecodedOriginal {
  text: string;
  whole: boolean;
}

export function decodeOriginal(bytes: Uint8Array): DecodedOriginal {
  try {
    return { text: new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes), whole: true };
  } catch {
    return { text: new TextDecoder("utf-8", { ignoreBOM: true }).decode(bytes), whole: false };
  }
}

export function startHistory(original: string): History {
  return { steps: [{ text: original, name: ORIGINAL_STEP }], at: 0, dropped: 0 };
}

export const originalText = (history: History) => history.steps[0].text;
export const workingText = (history: History) => history.steps[history.at].text;
export const isModified = (history: History) => workingText(history) !== originalText(history);
export const canUndo = (history: History) => history.at > 0;
export const canRedo = (history: History) => history.at < history.steps.length - 1;
export const editCount = (history: History) => history.at + history.dropped;

const heldAfterOriginal = (steps: Step[]) => steps.reduce((sum, step, i) => (i === 0 ? 0 : sum + step.text.length), 0);

export function commit(history: History, text: string, name: string, budget = HISTORY_BUDGET): History {
  if (text === workingText(history)) return history;
  const steps = [...history.steps.slice(0, history.at + 1), { text, name }];
  let dropped = history.dropped;
  while (steps.length > 2 && heldAfterOriginal(steps) > budget) {
    steps.splice(1, 1);
    dropped += 1;
  }
  return { steps, at: steps.length - 1, dropped };
}

export function amend(history: History, text: string, name: string): History {
  if (history.at === 0 || canRedo(history)) return commit(history, text, name);
  const steps = [...history.steps];
  steps[history.at] = { ...steps[history.at], text };
  return { ...history, steps };
}

export const undo = (history: History): History =>
  canUndo(history) ? { ...history, at: history.at - 1 } : history;

export const redo = (history: History): History =>
  canRedo(history) ? { ...history, at: history.at + 1 } : history;

export const restoreOriginal = (history: History, budget = HISTORY_BUDGET): History =>
  commit(history, originalText(history), RESTORE_STEP, budget);

export function workingBytes(history: History, original: Uint8Array): Uint8Array {
  return isModified(history) ? new TextEncoder().encode(workingText(history)) : original;
}
