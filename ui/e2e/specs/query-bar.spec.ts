import { browser, $, expect } from "@wdio/globals";
import { readFileSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { PORTS_FILE } from "../wdio.conf.ts";
import { CONFIG_DIR } from "./layout.ts";
import { quitTheApp, savedViewPrefs, shellIsUp } from "./sticky.ts";
import {
  addChips,
  chips,
  chipsRead,
  draft,
  emptyTheDraft,
  focusDraft,
  highlighted,
  listings,
  listingsAfter,
  noListingWhile,
  press,
  recordListings,
  removeEveryChip,
  rows,
  rowsAre,
  suggestionTexts,
  type,
  urlsOf,
} from "./query.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as { api: string; control: string; origin: string };
const LOCAL = new URL(ports.origin).host;
const COMMAND = "";
const CLEAR_ASK_DISMISSED = resolve(CONFIG_DIR, "clear-ask-dismissed");

const entry = (method: string, url: string, status: number, type?: string) => ({
  startedDateTime: "2026-11-05T09:00:00.000Z",
  time: 12,
  request: { method, url, httpVersion: "HTTP/1.1", headers: [] },
  response: {
    status,
    httpVersion: "HTTP/1.1",
    headers: type ? [{ name: "content-type", value: type }] : [],
    content: { size: 0, mimeType: type ?? "" },
  },
});

const SEED = {
  log: {
    version: "1.2",
    entries: [
      entry("GET", "https://api.example.test/v1/login", 200, "application/json"),
      entry("POST", "https://api.example.test/v1/login/token", 201, "application/json"),
      entry("OPTIONS", "https://api.example.test/v1/login", 204),
      entry("GET", "http://auth.example.test/session", 404, "text/html"),
      entry("POST", "http://auth.example.test/token/refresh", 500, "application/json"),
    ],
  },
};

const ALL = [
  "api.example.test/v1/login",
  "api.example.test/v1/login/token",
  "api.example.test/v1/login",
  "auth.example.test/session",
  "auth.example.test/token/refresh",
  `${LOCAL}/missing`,
  `${LOCAL}/json`,
];

const coreRows = async (query: string) => {
  const listed = (await (await fetch(`${ports.api}/flows${query}`)).json()) as { uri: string }[];
  return listed.map((flow) => flow.uri.replace(/^https?:\/\//, "")).sort();
};

const exported = async (query: string) => {
  const har = (await (await fetch(`${ports.api}/flows/export${query}`)).json()) as {
    log: { entries: { request: { url: string } }[] };
  };
  return har.log.entries.map((e) => e.request.url.replace(/^https?:\/\//, "")).sort();
};

// UC-4's query bar end to end (E-QUERY, DEC-71): every filter is a chip, only a
// chip lists, and the window's query is the one export and clear select by. Ends
// in a real quit with chips saved, for the relaunched spec to find again.
describe("UC-4 — one bar to filter and search", () => {
  before(async () => {
    await shellIsUp();
    await fetch(`${ports.api}/flows`, { method: "DELETE" });
    const imported = await fetch(`${ports.api}/flows/import`, { method: "POST", body: JSON.stringify(SEED) });
    if (!imported.ok) throw new Error(`the core refused the seed: ${imported.status} ${await imported.text()}`);
    await fetch(`${ports.control}/hit?p=missing`);
    await fetch(`${ports.control}/hit?p=json`);
    await rowsAre(ALL, "the seed");
    await recordListings();
  });

  it("⌘F puts the cursor in the bar", async () => {
    await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
    await browser.keys([COMMAND, "f"]);
    await browser.keys(COMMAND);
    await browser.waitUntil(
      () => browser.execute(() => document.activeElement?.getAttribute("aria-label") === "filter"),
      { timeout: 5_000, timeoutMsg: "⌘F never focused the bar" },
    );
  });

  it("builds status:4xx from suggestions alone, and lists only at the chip", async () => {
    await noListingWhile(async () => {
      await focusDraft();
      await type("st");
      expect(await suggestionTexts()).toContain("status:");
      expect(await (await highlighted()).getText()).toContain("status:");
      await press("Tab");
      expect(await draft().getValue()).toBe("status:");
      await type("4");
      expect(await (await highlighted()).getText()).toContain("status:4xx");
    }, "typing status:4");
    await rowsAre(ALL, "while typing");

    const before = (await listings()).length;
    await press("Tab");
    await chipsRead(["status:4xx"]);
    expect(await listingsAfter(before, "the status chip")).toEqual(["?status=4xx"]);
    await rowsAre(["auth.example.test/session", `${LOCAL}/missing`], "status:4xx");
  });

  it("offers every host seen, its port included, and that host's chip lists its flows", async () => {
    await focusDraft();
    await type("host:");
    const offered = await suggestionTexts();
    for (const host of ["api.example.test", "auth.example.test", LOCAL]) expect(offered).toContain(`host:${host}`);

    await emptyTheDraft();
    await type("host:local");
    await press("Tab");
    await chipsRead(["status:4xx", `host:${LOCAL}`]);
    await rowsAre([`${LOCAL}/missing`], "a host with its port");
    await removeEveryChip();
    await rowsAre(ALL, "no chips");
  });

  it("repeated keys match any of them, a negated key hides, and keys combine", async () => {
    await addChips("host:api.example.test host:auth.example.test");
    await rowsAre(ALL.slice(0, 5), "two hosts");
    await addChips("-method:OPTIONS");
    await rowsAre(
      ["api.example.test/v1/login", "api.example.test/v1/login/token", "auth.example.test/session", "auth.example.test/token/refresh"],
      "the preflight hidden",
    );
    await addChips("type:json");
    await chipsRead(["host:api.example.test", "host:auth.example.test", "-method:OPTIONS", "type:json"]);
    await rowsAre(
      ["api.example.test/v1/login", "api.example.test/v1/login/token", "auth.example.test/token/refresh"],
      "JSON of the two hosts",
    );
    await removeEveryChip();
  });

  it("two text chips match flows whose URI holds both", async () => {
    await addChips("login token");
    await chipsRead(["login", "token"]);
    await rowsAre(["api.example.test/v1/login/token"], "login and token");
  });

  it("a misspelt key stays a draft that says why, and the list does not move", async () => {
    const shown = urlsOf(await rows());
    await noListingWhile(async () => {
      await focusDraft();
      await type("stauts:4xx");
      await press("Enter");
      const fault = $('[aria-label="filter"][aria-invalid="true"]');
      await fault.waitForExist({ timeout: 5_000 });
    }, "a misspelt key");
    expect(await $("body").getText()).toContain("unknown key “stauts” — did you mean status?");
    expect(await chips()).toEqual(["login", "token"]);
    expect(await draft().getValue()).toBe("stauts:4xx");
    expect(urlsOf(await rows())).toEqual(shown);
  });

  it("Backspace in the empty draft removes the last chip, and the list widens", async () => {
    await emptyTheDraft();
    await press("Backspace");
    await chipsRead(["login"]);
    await rowsAre(ALL.slice(0, 3), "login alone");
    await removeEveryChip();
  });

  it("selects what a hand-written query selects, and exports exactly what it lists", async () => {
    const before = (await listings()).length;
    await addChips("status:5xx -method:OPTIONS");
    await rowsAre(["auth.example.test/token/refresh"], "status:5xx without preflights");
    const sent = (await listingsAfter(before, "the two chips")).at(-1)!;
    expect(sent).toBe("?status=5xx&not_method=OPTIONS");

    const shown = urlsOf(await rows());
    expect(await coreRows("?status=5xx&not_method=OPTIONS")).toEqual(shown);
    expect(await exported(sent)).toEqual(shown);
  });

  it("clears only the listed flows, through its ask, and keeps the chips", async () => {
    rmSync(CLEAR_ASK_DISMISSED, { force: true });
    await $('button[aria-label="clear 1 flow"]').click();
    const ask = $('[data-testid="clear-ask-panel"]');
    await ask.waitForDisplayed({ timeout: 15_000 });
    await ask.$("button=clear").click();

    await $('[data-testid="empty"]').waitForDisplayed({ timeout: 15_000 });
    expect(await $('[data-testid="empty"]').getText()).toContain("All cleared");
    expect(await chips()).toEqual(["status:5xx", "-method:OPTIONS"]);
    expect(await coreRows("")).toEqual(ALL.filter((url) => url !== "auth.example.test/token/refresh").sort());
  });

  it("saves the chips as they are made, then quits", async () => {
    await removeEveryChip();
    await addChips(`host:${LOCAL} -no-such-path`);
    await chipsRead([`host:${LOCAL}`, "-no-such-path"]);
    await rowsAre([`${LOCAL}/missing`, `${LOCAL}/json`], "the chips to relaunch with");
    await browser.waitUntil(async () => (savedViewPrefs()?.chips as unknown[] | undefined)?.length === 2, {
      timeout: 5_000,
      timeoutMsg: "view-prefs.json never held the two chips",
    });
    await quitTheApp();
  });
});
