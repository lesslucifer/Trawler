import type { ReactNode } from "react";
import { CopyText, Disclosure, Select, StepList } from "./components/index";

// The two screens that show manual instructions share this shape: pick a target,
// read its steps, grab its command. StepList's @use asks for a command step to
// arrive as a CopyText, so the command is REPLACED in place rather than appended
// beside the prose that introduces it.
function withCommand(steps: string[], command: string | null, label: string): ReactNode[] {
  if (!command) return steps;
  return steps.map((step) =>
    step === command ? <CopyText key={step} value={step} label={label} /> : step,
  );
}

export function TargetPicker({
  options,
  value,
  onChange,
  label,
}: {
  options: readonly { id: string; label: string }[];
  value: string;
  onChange: (id: string) => void;
  label: string;
}) {
  return (
    <div className="mb-2 flex items-center gap-chrome-gap font-ui text-body text-ink-mute">
      steps for
      <Select options={[...options]} value={value} onChange={onChange} label={label} />
    </div>
  );
}

export function ManualSteps({
  steps,
  command = null,
  commandLabel = "",
  testId,
}: {
  steps: string[];
  command?: string | null;
  commandLabel?: string;
  testId: string;
}) {
  return (
    <div data-testid={testId}>
      <StepList steps={withCommand(steps, command, commandLabel)} />
    </div>
  );
}

// Fork D4: Disclosure always discloses, so the host owns the "collapsed or not at
// all" branch. Both screens make the same choice — hide the manual route behind a
// summary where an action leads, render it bare where none does.
export function ManualFallback({
  summary,
  collapsed,
  defaultOpen,
  testId,
  className = "",
  children,
}: {
  summary: string;
  collapsed: boolean;
  defaultOpen?: boolean;
  testId: string;
  className?: string;
  children: ReactNode;
}) {
  if (!collapsed) return <div className={className}>{children}</div>;
  return (
    <div className={className} data-testid={testId}>
      <Disclosure summary={summary} defaultOpen={defaultOpen}>
        {children}
      </Disclosure>
    </div>
  );
}
