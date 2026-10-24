// A read of something that changes under it lands only while it is the newest,
// and a read nobody asked for waits for the one in flight rather than
// overtaking it, so an answer slower than the asking still reaches the screen.
// A caller holding a fresher answer of its own overtakes whatever is in flight.
export interface NewestRead {
  begin(mode: "shown" | "quiet"): (() => boolean) | null;
  supersede(): void;
}

export function newestRead(): NewestRead {
  let newest = 0;
  let inFlight = false;
  return {
    begin(mode) {
      if (mode === "quiet" && inFlight) return null;
      const read = ++newest;
      inFlight = true;
      return () => {
        if (read !== newest) return false;
        inFlight = false;
        return true;
      };
    },
    supersede() {
      newest += 1;
      inFlight = false;
    },
  };
}
