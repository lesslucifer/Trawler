import { apiAddr } from "./api";
import { Button, Notice } from "./components/index";

// One sentence for one cause, mounted in two places: the shell band while there
// is something below to qualify, the row region when there is not. The trailing
// clause characterises what the reader is looking at, so it is passed only when
// there is something true to say — an omitted clause is how this states the
// silence without also claiming why the region below it is empty.
//
// Fork D4 is why `retrying` drives both the disabled button and the detail line:
// disabling a button without saying what is awaited is half a contract.
export function NotAnswering({
  trailing,
  retrying,
  onRetry,
}: {
  trailing?: string;
  retrying: boolean;
  onRetry: () => void;
}) {
  const addr = apiAddr();
  const silence = trailing === undefined ? `no answer from ${addr}` : `no answer from ${addr} — ${trailing}`;
  return (
    <Notice
      tone="warn"
      detail={retrying ? `trying ${addr}…` : silence}
      action={
        <Button onClick={onRetry} disabled={retrying}>
          retry
        </Button>
      }
    >
      trawler is not answering
    </Notice>
  );
}
