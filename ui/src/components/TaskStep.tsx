import type { ReactNode } from "react";
import type { StepStatusView } from "../verdicts";
import { Help } from "./Help";
import { StatusLine } from "./StatusLine";

/**
 * TaskStep — one numbered task of a job: its number, what the user does, why as a note, the state something observed for it, and a slot for the doing.
 * @tier T2 — owns the task's form and the rule that makes the number safe: a number is a reading aid, a state is a reading. It renders a state only as a resolved StepStatusView, so the source of every state is a domain resolver and never the caller's memory of a click
 * @anatomy a baseline row: the number in Inter ink-mute tabular-nums in a 18px marker column, so every task body shares one left edge · the title in Inter ink-body, with an optional Help holding why the task matters, drawn under it while notes are shown · on a dimmed task, the step it waits on below the title in Inter 11px ink-mute · the observed state as a StatusLine 8px under that · a free slot below for what the user does — a button, a guide, a value to copy · dimmed whole at the disabled step when its precondition is not met, and still numbered
 * @use the steps of a job whose progress something observes — a probe, a captured request, a state this app itself holds. The setup screen's three tasks on this machine and its four for a device
 * @avoid manual instructions inside a task, or any sequence nothing observes (that is StepList, which refuses per-step state for exactly that reason) · a ✓ the user clicked into existence — pass no status where nothing read one, and a can't-check verdict where something tried · a progress count, a cursor or a "current step" highlight — a task is read, never pointed at · an unnumbered task (the number is the reading order, and a task without one is prose)
 * @states rendered · rendered dimmed (precondition unmet) · with or without a why, a state and a slot. Not interactive itself — every interaction belongs to the slot
 * @madeof StatusLine (T2) — the observed state · Help (T1) — why, while notes are shown
 */
export function TaskStep({
  n,
  title,
  help,
  waitsOn,
  status,
  dim = false,
  children,
}: {
  n: number;
  title: string;
  help?: string;
  waitsOn?: string;
  status?: StepStatusView;
  dim?: boolean;
  children?: ReactNode;
}) {
  return (
    <div
      data-testid={`task-${n}`}
      data-dim={dim || undefined}
      className={`flex gap-3 px-4 pt-4 ${dim ? "opacity-disabled" : ""}`}
    >
      <span className="w-step-marker shrink-0 text-right font-ui text-body tabular-nums text-ink-mute">
        {n}
      </span>
      <div className="min-w-0 flex-1">
        <p className="font-ui text-body text-ink">
          {help === undefined ? (
            title
          ) : (
            <Help text={help}>
              {title}
            </Help>
          )}
        </p>
        {waitsOn !== undefined && <p className="pt-chrome-tight font-ui text-chrome text-ink-mute">{waitsOn}</p>}
        {status !== undefined && (
          <div data-testid={`task-${n}-state`} className="pt-2">
            <StatusLine status={status} />
          </div>
        )}
        {children !== undefined && <div className="pt-2">{children}</div>}
      </div>
    </div>
  );
}
