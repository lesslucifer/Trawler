type Listen = (event: string, handler: () => void) => Promise<() => void>;

/**
 * Listens to a shell event while mounted; `ready` runs once the listener stands,
 * after a failed listen too. Returns the unsubscribe.
 */
export function subscribe(
  listen: Listen | undefined,
  event: string,
  handler: () => void,
  ready?: () => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  (listen?.(event, handler) ?? Promise.resolve(undefined))
    .then((u) => (disposed ? u?.() : (unlisten = u)))
    .catch((err: unknown) => {
      console.error(`listening for ${event} failed:`, err);
    })
    .then(() => {
      if (!disposed) ready?.();
    });
  return () => {
    disposed = true;
    unlisten?.();
  };
}

// A flag the shell sets and then announces is taken only once the listener
// stands: one set in between would be announced to nobody and never taken.
export const takeOnEvery = (listen: Listen | undefined, event: string, take: () => void): (() => void) =>
  subscribe(listen, event, take, take);
