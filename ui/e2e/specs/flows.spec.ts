import { browser, $, $$, expect } from "@wdio/globals";
import { readFileSync } from "node:fs";
import { PORTS_FILE } from "../wdio.conf.ts";

const ports = JSON.parse(readFileSync(PORTS_FILE, "utf8")) as {
  api: string;
  proxy: string;
  origin: string;
  control: string;
};

const rowCount = () => $$('[data-testid="flow-row"]').length;
const hit = (path?: string) => fetch(`${ports.control}/hit${path ? `?p=${path}` : ""}`);

// UC-4 end to end: the real Tauri window against a real ephemeral core. Point the
// app at the core's API port (runtime config, no rebuild) with a plain WebDriver
// eval — no Tauri invoke bridge — then prove the live flow list, filtering, and
// SSE-driven append.
async function pointAppAtCore() {
  // sessionStorage survives the reload; the fresh page reads it before React
  // mounts, so both the fetch and the SSE stream bind the ephemeral port.
  await browser.execute((api: string) => {
    window.sessionStorage.setItem("__TRAWLER_API__", api);
    location.reload();
  }, ports.api);
  await $("header").waitForExist({ timeout: 15_000 });
}

describe("UC-4 — watch live traffic in the native app", () => {
  before(async () => {
    await pointAppAtCore();
  });

  it("renders the base layout with an empty flow list", async () => {
    await expect($("header")).toBeExisting();
    await expect($('[aria-label="host"]')).toBeExisting();
    await expect($('[data-testid="empty"]')).toBeExisting();
  });

  it("shows a proxied request as a live flow row", async () => {
    await hit();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "flow row never appeared after a proxied request",
    });
    await expect($('[data-testid="flow-row"]')).toHaveText(expect.stringContaining("127.0.0.1"));
  });

  it("filters the row out on a non-matching host and back in when cleared", async () => {
    await $('[aria-label="host"]').setValue("no-such-host.test");
    await browser.waitUntil(async () => (await rowCount()) === 0, {
      timeout: 15_000,
      timeoutMsg: "non-matching filter did not empty the list",
    });
    await $('[aria-label="host"]').setValue("");
    await browser.waitUntil(async () => (await rowCount()) >= 1, { timeout: 15_000 });
  });

  it("appends a second row on a new request without a manual refresh", async () => {
    const before = await rowCount();
    await hit();
    await browser.waitUntil(async () => (await rowCount()) > before, {
      timeout: 15_000,
      timeoutMsg: "second proxied request did not append a row live",
    });
  });
});

// UC-5 end to end: proxy a request, click its row, and inspect it — headers,
// timing, versions, body viewers (JSON tree, raw toggle), copy-as-curl. The
// brotli step proves the W1 decode chain surfaces in the inspector; the image
// step proves the type dispatch. Clipboard is unreliable under WebDriver, so
// curl is asserted as text in the DOM (recorded waiver in the epic plan).
describe("UC-5 — inspect a captured flow in the native app", () => {
  async function openRowFor(path: string) {
    await hit(path);
    let row: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const candidate of await $$('[data-testid="flow-row"]')) {
          if ((await candidate.getText()).includes(path)) {
            row = candidate;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `no flow row for ${path}` },
    );
    await row!.click();
    await $('[data-testid="detail"]').waitForExist({ timeout: 15_000 });
  }

  it("opens the detail with headers, timing, and versions on row click", async () => {
    await openRowFor("/json");
    const detail = await $('[data-testid="detail"]').getText();
    expect(detail).toContain("/json");
    expect(detail).toContain("HTTP/1.1");
    expect(detail).toMatch(/\d+ ms/);
    expect(detail.toLowerCase()).toContain("content-type");
  });

  it("renders a JSON response body as a tree", async () => {
    await $('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
    const tree = await $('[data-testid="body-json"]').getText();
    expect(tree).toContain("greeting");
    expect(tree).toContain('"hello"');
    expect(tree).toContain("numbers");
  });

  it("toggles to the raw stored bytes as text", async () => {
    await $("button=raw").click();
    await $('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const raw = await $('[data-testid="body-text"]').getText();
    expect(raw).toContain('{"greeting":"hello"');
    await $("button=decoded").click();
    await $('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
  });

  it("shows a runnable curl for the request", async () => {
    const curl = await $('[data-testid="curl"]').getText();
    expect(curl).toMatch(/^curl /);
    expect(curl).toContain("/json");
    expect(curl).toContain("-H");
  });

  it("renders a brotli-compressed JSON body as a tree (decode chain via the UI)", async () => {
    await openRowFor("/br");
    await $('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
    const tree = await $('[data-testid="body-json"]').getText();
    expect(tree).toContain("compressed");
    expect(tree).toContain('"brotli"');
  });

  it("renders an image body as an <img>", async () => {
    await openRowFor("/img");
    await $('[data-testid="body-image"]').waitForExist({ timeout: 15_000 });
    const src = await $('[data-testid="body-image"]').getAttribute("src");
    expect(src).toContain("blob:");
  });
});
