import { describe, expect, it, vi } from "vitest";
import { subscribe, takeOnEvery } from "./shellEvent";

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function fakeListen() {
  const handlers: Array<() => void> = [];
  const unlisten = vi.fn();
  let stand: () => void = () => {};
  const listen = vi.fn((_event: string, handler: () => void) => {
    handlers.push(handler);
    return new Promise<() => void>((resolve) => {
      stand = () => resolve(unlisten);
    });
  });
  return { listen, handlers, unlisten, stand: () => stand() };
}

describe("takeOnEvery", () => {
  it("takes only once the listener stands, then on every announcement", async () => {
    const shell = fakeListen();
    const take = vi.fn();
    takeOnEvery(shell.listen, "quit-requested", take);
    await flush();
    expect(take).not.toHaveBeenCalled();
    shell.stand();
    await flush();
    expect(take).toHaveBeenCalledTimes(1);
    shell.handlers[0]();
    expect(take).toHaveBeenCalledTimes(2);
  });

  it("an unmount before the listener stands unlistens and takes nothing", async () => {
    const shell = fakeListen();
    const take = vi.fn();
    const stop = takeOnEvery(shell.listen, "dialog-requested", take);
    stop();
    shell.stand();
    await flush();
    expect(shell.unlisten).toHaveBeenCalledTimes(1);
    expect(take).not.toHaveBeenCalled();
  });

  it("a failed listen still takes once, and no listen at all takes once", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const take = vi.fn();
    takeOnEvery(() => Promise.reject(new Error("no shell")), "dialog-requested", take);
    takeOnEvery(undefined, "quit-requested", take);
    await flush();
    expect(take).toHaveBeenCalledTimes(2);
    expect(error).toHaveBeenCalledWith("listening for dialog-requested failed:", expect.any(Error));
    error.mockRestore();
  });
});

describe("subscribe", () => {
  it("without a ready step only handles announcements, and unlistens on unmount", async () => {
    const shell = fakeListen();
    const handler = vi.fn();
    const stop = subscribe(shell.listen, "close-window-requested", handler);
    shell.stand();
    await flush();
    expect(handler).not.toHaveBeenCalled();
    shell.handlers[0]();
    expect(handler).toHaveBeenCalledTimes(1);
    stop();
    expect(shell.unlisten).toHaveBeenCalledTimes(1);
  });
});
