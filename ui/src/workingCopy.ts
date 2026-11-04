export interface Step {
  text: string;
  name: string;
}

export interface History {
  steps: Step[];
  at: number;
}

export const ORIGINAL_STEP = "original";
export const RESTORE_STEP = "restore original";

export function startHistory(original: string): History {
  return { steps: [{ text: original, name: ORIGINAL_STEP }], at: 0 };
}

export const originalText = (history: History) => history.steps[0].text;
export const workingText = (history: History) => history.steps[history.at].text;
export const isModified = (history: History) => workingText(history) !== originalText(history);
export const canUndo = (history: History) => history.at > 0;
export const canRedo = (history: History) => history.at < history.steps.length - 1;
export const editCount = (history: History) => history.at;

export function commit(history: History, text: string, name: string): History {
  if (text === workingText(history)) return history;
  const steps = [...history.steps.slice(0, history.at + 1), { text, name }];
  return { steps, at: steps.length - 1 };
}

export function amend(history: History, text: string, name: string): History {
  if (history.at === 0 || canRedo(history)) return commit(history, text, name);
  const steps = [...history.steps];
  steps[history.at] = { ...steps[history.at], text };
  return { steps, at: history.at };
}

export const undo = (history: History): History =>
  canUndo(history) ? { ...history, at: history.at - 1 } : history;

export const redo = (history: History): History =>
  canRedo(history) ? { ...history, at: history.at + 1 } : history;

export const restoreOriginal = (history: History): History =>
  commit(history, originalText(history), RESTORE_STEP);

export function workingBytes(history: History, original: Uint8Array): Uint8Array {
  return isModified(history) ? new TextEncoder().encode(workingText(history)) : original;
}
