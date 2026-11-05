import { browser, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";
import { clearTheCapture, savedViewPrefs, shellIsUp } from "./sticky.ts";
import { chips, chipsRead, removeEveryChip, rowsAre } from "./query.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { origin: string };
const LOCAL = new URL(ports.origin).host;

// A new app process: the query-bar spec quit the last one with two chips made.
// It leaves the bar empty, so the flows spec after it lists unfiltered.
describe("UC-4 — the chips survive a relaunch", () => {
  before(async () => {
    await shellIsUp();
  });

  it("opens with the chips back in the bar, and the list follows them", async () => {
    await chipsRead([`host:${LOCAL}`, "-no-such-path"]);
    await rowsAre([`${LOCAL}/missing`, `${LOCAL}/json`], "the restored chips");
  });

  it("removing them is saved too, and the bar is left empty", async () => {
    await removeEveryChip();
    expect(await chips()).toEqual([]);
    await browser.waitUntil(async () => (savedViewPrefs()?.chips as unknown[] | undefined)?.length === 0, {
      timeout: 5_000,
      timeoutMsg: "view-prefs.json kept chips the bar no longer holds",
    });
    await clearTheCapture();
  });
});
