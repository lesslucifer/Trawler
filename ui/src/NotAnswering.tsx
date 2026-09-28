import { apiAddr } from "./api";
import { Button, Notice } from "./components/index";

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
  const addr = apiAddr();
  const silence = `The app gets no answer from it at ${addr}.${trailing === undefined ? "" : ` ${trailing}`}`;
  return (
    <Notice
      tone="warn"
      layout={region ? "region" : "inline"}
      detail={retrying ? `Trying ${addr} again…` : silence}
      action={
        <Button onClick={onRetry} disabled={retrying}>
          retry
        </Button>
      }
    >
      Trawler's core is not answering
    </Notice>
  );
}
