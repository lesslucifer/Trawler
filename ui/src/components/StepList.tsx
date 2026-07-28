import type { ReactNode } from "react";

/**
 * StepList — ordered manual instructions: a numbered list with a free slot per step.
 * @tier T1 — renders the steps it is handed, in order, and decides nothing about their content
 * @anatomy a real <ol> · 18px right-aligned marker column, Inter 11px ink-mute tabular ("1." · "2.") so every step body shares one left edge · one ReactNode slot per step, Inter 12px ink-body at 1.55, min-w-0 and wrapping (never truncated — a cut command is a broken command), stacking its children with a 6px gap · 10px between steps · no box, background, rule or padding of its own
 * @use any ordered sequence the user performs by hand — a certificate install, a device setting, a remedy, a migration. Pass a command step as a CopyText inside the slot
 * @avoid a single unordered statement (that is prose, or Notice if it is about a zone's state) · wire data key/value pairs (use KeyValueRow) · nesting a StepList inside a step, or sub-steps of any kind — flatten instead · adding a copy affordance: a command step is a CopyText, which already owns click-to-copy and its confirm · rendering per-step completion or a current-step highlight — the app cannot observe it; the probed verdict belongs to StatusLine above · truncating, clamping or scrolling a step
 * @states rendered — the only state. No empty state (a host with no steps renders no list) · no per-step done/current, no progress count, no interaction states, no collapsed state (disclosure is the host's)
 * @madeof —
 */
export function StepList({ steps, className = "" }: { steps: ReactNode[]; className?: string }) {
  return (
    <ol className={`m-0 flex list-none flex-col gap-step-gap p-0 ${className}`}>
      {steps.map((step, i) => (
        <li key={i} className="grid grid-cols-[18px_1fr] items-baseline gap-2">
          <span
            aria-hidden="true"
            className="text-right font-ui text-[11px] tabular-nums text-ink-mute"
          >
            {i + 1}.
          </span>
          <div className="flex min-w-0 flex-col gap-chrome-gap font-ui text-body leading-relaxed text-ink-body">
            {step}
          </div>
        </li>
      ))}
    </ol>
  );
}
