import type { ReactNode } from "react";
import { Button, CommandBlock, CopyText, Disclosure, QrCode, Select, StepList } from "./components/index";
import { isCommand, isSave, isScan, type CaFormat, type GuideStep } from "./setup";
import type { ActionHandle } from "./useAction";

// The two screens that show a platform's instructions — setup and the quit
// dialog — render its steps the same way, so a step fixed for one reads fixed
// in the other (W11's S4).

// A value is read inside the sentence that names it, because that sentence is
// what says which field it goes in. A command is read somewhere else entirely,
// so it leaves the sentence and takes a surface of its own.
function Value({ value, label, wire }: { value: string; label: string; wire: string }) {
  return (
    <span data-wire={wire} className="font-mono">
      <CopyText value={value} label={label} />
    </span>
  );
}

export function StepParts({ step, save }: { step: GuideStep; save?: ActionHandle }) {
  const said = step.filter((part) => !isCommand(part));
  const commands = step.filter(isCommand);
  const scans = step.filter(isScan);
  return (
    <>
      {said.length > 0 && (
        <span className="flex flex-wrap items-center gap-chrome-gap">
          {said.map((part, i) => {
            if (typeof part === "string") return <span key={i}>{part}</span>;
            if ("copy" in part)
              return <Value key={i} value={part.copy} label={part.label} wire={part.wire} />;
            return save?.trigger ? (
              <Button
                key={i}
                frame="bare"
                tone="mute"
                onClick={save.trigger}
                disabled={save.state === "running"}
              >
                save…
              </Button>
            ) : null;
          })}
        </span>
      )}
      {commands.map((part, i) => (
        <CommandBlock key={i} value={part.command} label={part.label} />
      ))}
      {scans.map((part, i) => (
        <div key={i} data-testid="scan-code" className="pt-2">
          <QrCode value={part.copy} label={part.label} />
        </div>
      ))}
    </>
  );
}

// The save control produces the file the step names, so its outcome lands inside
// that step rather than under the guide that contains it.
export function GuideSteps({
  steps,
  save,
  saveNote,
}: {
  steps: GuideStep[];
  save?: ActionHandle;
  saveNote?: (format: CaFormat) => ReactNode;
}) {
  return (
    <StepList
      steps={steps.map((step, i) => {
        const saves = step.find(isSave);
        return (
          <div key={i} className="flex min-w-0 flex-col gap-chrome-gap">
            <StepParts step={step} save={save} />
            {saves && saveNote?.(saves.save)}
          </div>
        );
      })}
    />
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

// Fork D4: Disclosure always discloses, so the host owns the "collapsed or not at
// all" branch — the manual route hides behind a summary where an action leads,
// and renders bare where none does.
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
