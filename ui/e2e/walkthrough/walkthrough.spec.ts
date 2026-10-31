import { browser, $ } from "@wdio/globals";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { findRun, type Chapter, type Patch, type Walk } from "./script.ts";
import {
  frontQuery,
  OUT_VAR,
  PORTS_FILE,
  RUN_VAR,
  type Ports,
} from "./walkthrough.conf.ts";

const { reel, launch } = findRun(process.env[RUN_VAR] ?? "");
const out = join(process.env[OUT_VAR] ?? "", reel.id);
const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as Ports;

// A frame is taken once what the chapter did has reached the screen: an act's
// IPC round-trip and the re-render it causes both land after the click returns.
const SETTLE_MS = 700;
const ANCHOR_MS = 30_000;

// Taller than the shipped window, so a screen with its guides open still fits
// one frame and a caption about its last line has that line to point at.
const FRAME = { width: 1440, height: 1200 };

type Filmed = { id: string; frame: string } | { id: string; skipped: string };
const filmed: Filmed[] = [];

function record(entry: Filmed) {
  filmed.push(entry);
  writeFileSync(
    join(out, `${launch.id}.json`),
    JSON.stringify(
      { reel: reel.id, launch: launch.id, chapters: filmed },
      null,
      2,
    ),
  );
}

async function control(path: string) {
  const answer = await fetch(`${ports.control}${path}`);
  if (!answer.ok) throw new Error(`harness ${path}: ${answer.status}`);
}

const buttonStarting = (prefix: string) =>
  $(`//button[starts-with(normalize-space(.), "${prefix}")]`);

const QUESTION = "which device's traffic do you want to see";
const picker = () => $(`[aria-label="${QUESTION}"][aria-haspopup="listbox"]`);

// The window re-reads this machine when it regains focus, and a read already in
// flight may have started before the patch it is meant to show, so the second
// focus starts one that cannot have.
async function rereadOnFocus() {
  for (let i = 0; i < 2; i++) {
    await browser.execute(() => window.dispatchEvent(new Event("focus")));
    await browser.pause(400);
  }
}

const walk: Walk = {
  async script(patch: Patch) {
    const failure = await browser.executeAsync(
      (p: Patch, done: (e: string | null) => void) => {
        (
          window as unknown as {
            __TAURI__: {
              core: { invoke: (c: string, a: unknown) => Promise<unknown> };
            };
          }
        ).__TAURI__.core
          .invoke("scenario_script", { patch: p })
          .then(
            () => done(null),
            (e: unknown) => done(String(e)),
          );
      },
      patch,
    );
    if (failure)
      throw new Error(
        `scenario_script refused ${JSON.stringify(patch)}: ${failure}`,
      );
  },
  async recheck() {
    await rereadOnFocus();
  },
  async openPicker() {
    if (await $('[role="listbox"]').isExisting()) return;
    await picker().click();
    await $('[role="listbox"]').waitForExist({ timeout: 5_000 });
  },
  async closePicker() {
    if (!(await $('[role="listbox"]').isExisting())) return;
    await browser.keys("Escape");
    await browser.waitUntil(
      async () => !(await $('[role="listbox"]').isExisting()),
      {
        timeout: 5_000,
        timeoutMsg: "the picker stayed open after Escape",
      },
    );
  },
  async choose(label: string) {
    await walk.openPicker();
    await $(`[role="option"][aria-label="${label}"]`).click();
    await browser.waitUntil(
      async () => !(await $('[role="listbox"]').isExisting()),
      {
        timeout: 5_000,
        timeoutMsg: `the picker stayed open after choosing ${label}`,
      },
    );
  },
  async click(prefix: string) {
    const target = buttonStarting(prefix);
    await target.waitForExist({
      timeout: 10_000,
      timeoutMsg: `no button starting "${prefix}"`,
    });
    await target.waitForEnabled({ timeout: 10_000 });
    await target.click();
  },
  async closeGuide(n: number) {
    await $(`[data-testid="task-${n}"] details[open] > summary`).click();
  },
  async openGuide(n: number) {
    await $(`[data-testid="task-${n}"] details:not([open]) > summary`).click();
  },
  async openHelp(n: number) {
    for (const help of await $$(
      `[data-testid="task-${n}"] button[aria-label^="help:"][aria-expanded="false"]`,
    ))
      await help.click();
  },
  async dot() {
    await $('[data-testid="status-dot"]').click();
  },
  async filter(text: string) {
    await $('[aria-label="filter"]').setValue(text);
  },
  async hit(opts = {}) {
    await control(opts.untrusted ? "/hit?untrusted=1" : "/hit");
  },
  async front(mode) {
    await control(`/front?${frontQuery(mode)}`);
  },
  async interfaces(list) {
    const answer = await fetch(`${ports.control}/front/reach`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(list),
    });
    if (!answer.ok) throw new Error(`harness /front/reach: ${answer.status}`);
  },
  async pause(ms: number) {
    await browser.pause(ms);
  },
};

async function offered(label: string): Promise<boolean> {
  await walk.openPicker();
  const there = await $(`[role="option"][aria-label="${label}"]`).isExisting();
  await walk.closePicker();
  return there;
}

// Any one of the anchors will do: each names a screen this chapter is about, and
// a chapter whose moment reads differently after a fix names both.
async function painted(chapter: Chapter) {
  let found: string | undefined;
  await browser.waitUntil(
    async () => {
      for (const selector of chapter.anchor)
        if (await $(selector).isExisting()) {
          found = selector;
          return true;
        }
      return false;
    },
    {
      timeout: ANCHOR_MS,
      timeoutMsg: `${reel.id} · ${launch.id} · ${chapter.id}: none of ${JSON.stringify(chapter.anchor)} painted, so there is no frame to take`,
    },
  );
  await $(found!).scrollIntoView({ block: "nearest" });
}

const frameName = (index: number, chapter: Chapter) =>
  `${launch.id}-${String(index + 1).padStart(2, "0")}-${chapter.id}.png`;

describe(`walkthrough ${reel.id} · ${launch.id}`, () => {
  let endedBecause: string | null = null;

  before(async () => {
    mkdirSync(out, { recursive: true });
    await browser.setWindowSize(FRAME.width, FRAME.height);
    await $("header").waitForExist({ timeout: ANCHOR_MS });
  });

  launch.chapters.forEach((chapter, index) => {
    it(chapter.id, async () => {
      if (
        endedBecause === null &&
        chapter.offeredAs &&
        !(await offered(chapter.offeredAs))
      )
        endedBecause = `"${chapter.offeredAs}" is not offered on ${reel.host}`;
      if (endedBecause !== null) {
        record({ id: chapter.id, skipped: endedBecause });
        return;
      }
      await chapter.act?.(walk);
      await browser.pause(SETTLE_MS);
      await painted(chapter);
      const frame = frameName(index, chapter);
      await browser.saveScreenshot(join(out, frame));
      record({ id: chapter.id, frame });
    });
  });
});
