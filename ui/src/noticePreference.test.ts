import { expect, test, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { dismissNotice, noticeDismissed, NOTICE_KEY, notesShown, showNotes, useNotesShown } from "./noticePreference";

test("notice reads and writes use the closed key on the keyed commands", async () => {
  const call = vi.fn(async (command: string) => command === "notice_dismissed") as never;

  await expect(noticeDismissed(NOTICE_KEY.clear, call)).resolves.toBe(true);
  await dismissNotice(NOTICE_KEY.clear, call);

  expect(call).toHaveBeenNthCalledWith(1, "notice_dismissed", { key: "clear-ask" });
  expect(call).toHaveBeenNthCalledWith(2, "dismiss_notice", { key: "clear-ask" });
});

test("a browser without invoke treats the ask as not dismissed", async () => {
  const unavailable = vi.fn(async () => {
    throw new Error("invoke is unavailable");
  }) as never;

  await expect(noticeDismissed(NOTICE_KEY.clear, unavailable)).resolves.toBe(false);
});

test("the notes choice is read and written through the shell's two commands", async () => {
  const call = vi.fn(async (command: string) => command === "notes_shown") as never;

  await expect(notesShown(call)).resolves.toBe(true);
  await showNotes(false, call);

  expect(call).toHaveBeenNthCalledWith(1, "notes_shown");
  expect(call).toHaveBeenNthCalledWith(2, "show_notes", { shown: false });
});

test("notes stay hidden where the shell cannot answer", async () => {
  const unavailable = vi.fn(async () => {
    throw new Error("invoke is unavailable");
  }) as never;
  const unanswered = vi.fn(async () => undefined) as never;

  await expect(notesShown(unavailable)).resolves.toBe(false);
  await expect(notesShown(unanswered)).resolves.toBe(false);
});

test("a choice made before the remembered one arrives is kept", async () => {
  let answer: (shown: boolean) => void = () => undefined;
  const call = vi.fn((command: string) =>
    command === "notes_shown" ? new Promise<boolean>((resolve) => (answer = resolve)) : Promise.resolve(),
  ) as never;
  const { result } = renderHook(() => useNotesShown(call));

  act(() => result.current[1](true));
  await act(async () => answer(false));

  expect(result.current[0]).toBe(true);
});

test("a write the shell refuses still changes the screen", async () => {
  const call = vi.fn(async (command: string) => {
    if (command === "show_notes") throw new Error("read-only config");
    return false;
  }) as never;
  const { result } = renderHook(() => useNotesShown(call));

  await act(async () => result.current[1](true));

  expect(result.current[0]).toBe(true);
});
