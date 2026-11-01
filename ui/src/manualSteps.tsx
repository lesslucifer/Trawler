import type { ReactNode } from "react";
import {
  Button,
  CommandBlock,
  CopyText,
  Disclosure,
  Help,
  MenuPath,
  QrCode,
  Select,
  StepList,
  StepMedia,
} from "./components/index";
import {
  isCommand,
  isHelp,
  isMedia,
  isPath,
  isRoute,
  isSave,
  isScan,
  type CaFormat,
  type GuideStep,
  type RouteName,
} from "./setup";
import { STEP_MEDIA } from "./stepMedia";
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

// A route opens a screen for the step it sits in, so it is a control in the
// sentence, as the save is. The guide's first control is the one framed where
// it is the next thing to do on the screen (W12's E1).
export interface StepControls {
  save?: ActionHandle;
  open?: (route: RouteName) => void;
  framed?: boolean;
}

export function StepParts({ step, save, open, framed = false }: { step: GuideStep } & StepControls) {
  const said = step.filter((part) => !isCommand(part) && !isMedia(part) && !isHelp(part));
  const help = step.find(isHelp);
  const commands = step.filter(isCommand);
  const scans = step.filter(isScan);
  const media = step.filter(isMedia);
  const frame = (control: boolean) => (framed && control ? {} : ({ frame: "bare", tone: "mute" } as const));
  const firstControl = said.findIndex(
    (part) => typeof part === "object" && ((isRoute(part) && !part.disabled) || isSave(part)),
  );
  const parts = said.map((part, i) => {
    if (typeof part === "string") return <span key={i}>{part}</span>;
    if ("copy" in part) return <Value key={i} value={part.copy} label={part.label} wire={part.wire} />;
    if (isPath(part)) return <MenuPath key={i} path={part.path} />;
    if (isRoute(part))
      return open ? (
        <Button key={i} {...frame(i === firstControl)} onClick={() => open(part.open)} disabled={part.disabled}>
          {part.label}
        </Button>
      ) : null;
    return save?.trigger ? (
      <Button key={i} {...frame(i === firstControl)} onClick={save.trigger} disabled={save.state === "running"}>
        save…
      </Button>
    ) : null;
  });
  return (
    <>
      {said.length > 0 &&
        (help ? (
          // Words flowing as text, so the ? follows the last of them wherever it
          // wraps; a flex row that wraps takes the full width and strands it (W17's D1).
          <Explained help={help}>
            <span data-testid="step-words">
              {parts.filter((part) => part !== null).flatMap((part, i) => (i === 0 ? [part] : [" ", part]))}
            </span>
          </Explained>
        ) : (
          <span data-testid="step-words" className="flex flex-wrap items-center gap-chrome-gap">
            {parts}
          </span>
        ))}
      {commands.map((part, i) => (
        <CommandBlock key={i} value={part.command} label={part.label} />
      ))}
      {media.map((part) => (
        <StepMedia key={part.media} media={STEP_MEDIA[part.media]} alt={part.alt} shows={part.shows} />
      ))}
      {scans.map((part, i) => (
        <div key={i} data-testid="scan-code" className="pt-2">
          <QrCode value={part.copy} label={part.label} />
        </div>
      ))}
    </>
  );
}

// What explains a step waits behind a ? at its end (W13's R1).
export function Explained({ help, children }: { help?: { help: string; topic: string }; children: ReactNode }) {
  if (help === undefined) return <>{children}</>;
  return (
    <Help topic={help.topic} text={help.help}>
      {children}
    </Help>
  );
}

// The save control produces the file the step names, so its outcome lands inside
// that step rather than under the guide that contains it.
export function GuideSteps({
  steps,
  save,
  saveNote,
  open,
  framed = false,
}: {
  steps: GuideStep[];
  saveNote?: (format: CaFormat) => ReactNode;
} & StepControls) {
  return (
    <StepList
      steps={steps.map((step, i) => {
        const saves = step.find(isSave);
        return (
          <div key={i} className="flex min-w-0 flex-col gap-chrome-gap">
            <StepParts step={step} save={save} open={open} framed={framed && i === 0} />
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
