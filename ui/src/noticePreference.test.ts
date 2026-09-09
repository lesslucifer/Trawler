import { expect, test, vi } from "vitest";
import { dismissNotice, noticeDismissed, NOTICE_KEY } from "./noticePreference";

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
