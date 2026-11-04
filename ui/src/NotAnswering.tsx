import { useContext } from "react";
import { Button, Notice } from "./components/index";
import { stoppedWords } from "./coreService";
import { CoreServiceContext } from "./useCoreService";

// One statement for one cause, mounted in three places: the shell band while
// there is something below to qualify, and as the whole region — the rows'
// or setup's — when there is not. The trailing sentence characterises what the
// reader is looking at, so it is passed only when there is something true to
// say — an omitted sentence is how this states the silence without also
// claiming why the region below it is empty.
//
// Fork D4 is why `retrying` drives both the disabled button and the detail line:
// disabling a button without saying what is awaited is half a contract.
export function NotAnswering({
  trailing,
  retrying,
  onRetry,
  region = false,
}: {
  trailing?: string;
  retrying: boolean;
  onRetry: () => void;
  region?: boolean;
}) {
  const core = useContext(CoreServiceContext);
  const exited = core?.state?.state === "exited" ? core.state : null;
  if (core && (exited || core.restarting)) {
    const detail = core.restarting
      ? "Restarting it…"
      : (core.refusal ?? (exited ? `It stopped — ${stoppedWords(exited)}.` : undefined));
    return (
      <Notice
        tone="warn"
        layout={region ? "region" : "inline"}
        detail={detail}
        action={
          <Button onClick={() => void core.restart()} disabled={core.restarting}>
            restart
          </Button>
        }
      >
        Trawler's capture service isn't answering
      </Notice>
    );
  }
  return (
    <Notice
      tone="warn"
      layout={region ? "region" : "inline"}
      detail={retrying ? "Trying again…" : (trailing ?? "It usually comes back on its own in a moment.")}
      action={
        <Button onClick={onRetry} disabled={retrying}>
          try again
        </Button>
      }
    >
      Trawler's capture service isn't answering
    </Notice>
  );
}
