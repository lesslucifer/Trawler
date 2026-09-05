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

// Point the real Tauri window at the ephemeral core (runtime config, no rebuild)
// with a plain WebDriver eval — no Tauri invoke bridge.
async function pointAppAtCore() {
  // sessionStorage survives the reload; the fresh page reads it before React
  // mounts, so both the fetch and the SSE stream bind the ephemeral port.
  await browser.execute((api: string) => {
    window.sessionStorage.setItem("__TRAWLER_API__", api);
    location.reload();
  }, ports.api);
  await $("header").waitForExist({ timeout: 15_000 });
}

// UC-4 (watch) end to end: the list is the screen a developer leaves open while
// debugging. Every assertion here is on what E-REDESIGN.W4 built from W3's
// design — the reason phrase in the pill's accessible name, the two absences
// told apart, and the keyboard path the row did not have at all before. Each one
// fails against the pre-WU build, which is what makes them proof rather than
// green paint.
describe("UC-4 — watch live traffic in the native app", () => {
  before(async () => {
    await pointAppAtCore();
    await hit(); // a decrypted flow reveals the list (leaves onboarding, UC-ONBOARD)
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "list never revealed after the first decrypted flow",
    });
  });

  it("a proxied request lands as a row carrying every column", async () => {
    await hit("/json");
    await browser.waitUntil(
      async () => {
        for (const row of await $$('[data-testid="flow-row"]')) {
          if ((await row.getText()).includes("/json")) return true;
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: "no row for /json" },
    );

    const row = (await $$('[data-testid="flow-row"]')).at(-1)!;
    const text = await row.getText();
    expect(text).toMatch(/\d{2}:\d{2}:\d{2}/); // absolute time, for log correlation
    expect(text).toContain("GET");
    expect(text).toMatch(/\d+ ms/);
  });

  it("a status carries its reason phrase, so the tone is never the only channel", async () => {
    const pill = await $('[aria-label="200 OK"]');
    await pill.waitForExist({ timeout: 15_000 });
    expect(await pill.getText()).toContain("200");
  });

  it("a filter narrows the list", async () => {
    await $('[aria-label="host"]').setValue("no-such-host.invalid");
    await browser.waitUntil(async () => (await rowCount()) === 0, {
      timeout: 15_000,
      timeoutMsg: "the filter never narrowed the list",
    });
  });

  it("an unmatched filter says the query excludes the traffic, not that none exists", async () => {
    const empty = await $('[data-testid="empty"]');
    await empty.waitForExist({ timeout: 15_000 });
    const text = await empty.getText();
    expect(text).toContain("no flows match this filter");
    expect(text).not.toContain("no flows yet");
  });

  it("clearing the filter brings the rows back", async () => {
    await $('[aria-label="host"]').setValue(" ");
    await $('[aria-label="host"]').clearValue();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "the list never came back after clearing the filter",
    });
  });

  it("the keyboard selects a row and opens its detail — no pointer at all", async () => {
    // Focused without a click, so the assertion below is about the keys alone.
    await browser.execute(() => {
      (document.querySelector('[data-testid="flow-scroll"]') as HTMLElement).focus();
    });
    await browser.keys("ArrowDown");
    await browser.waitUntil(
      async () => (await $$('[data-testid="flow-row"][aria-selected="true"]')).length === 1,
      { timeout: 15_000, timeoutMsg: "ArrowDown never selected a row" },
    );

    await browser.keys("Enter");
    await $('[data-testid="detail"]').waitForExist({ timeout: 15_000 });
  });
});

// UC-5 (inspect) end to end: proxy a request, click its row, and inspect it —
// headers, timing, versions, body viewers (JSON tree, raw toggle), copy-as-curl.
// The brotli step proves the W1 decode chain surfaces in the inspector; the image
// step proves the type dispatch. Clipboard is unreliable under WebDriver, so curl
// is asserted as text in the DOM (recorded waiver in the epic plan). The fresh
// launch is onboarding (UC-ONBOARD); the first decrypted flow reveals the list, so
// this block reveals it once up front before selecting rows to inspect.
describe("UC-5 — inspect a captured flow in the native app", () => {
  before(async () => {
    await pointAppAtCore();
    await hit(); // a decrypted flow reveals the list (leaves onboarding, UC-ONBOARD)
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "list never revealed after the first decrypted flow",
    });
  });

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
