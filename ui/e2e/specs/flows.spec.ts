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

const RESPONSE_PANE = '[data-testid="body-viewer"][data-leg="response"]';
const inResponsePane = (selector: string) => $(`${RESPONSE_PANE} ${selector}`);
const responseMode = (label: "decoded" | "raw") =>
  $(`${RESPONSE_PANE} [role="radiogroup"]`).$(`button=${label}`);

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

// UC-5 (inspect) end to end: proxy a request, click its row, and inspect it —
// headers, timing, versions, body viewers (type dispatch, raw toggle, stated
// truncation), copy-as-curl. The brotli step proves the W1 decode chain surfaces
// in the inspector. Clipboard is unreliable under WebDriver, so curl is asserted
// as text in the DOM (recorded waiver in the epic plan). The fresh launch is
// onboarding (UC-ONBOARD); the first decrypted flow reveals the list, so this
// block reveals it once up front before selecting rows to inspect.
//
// The panel mounts one body pane per leg, so every body assertion is scoped to
// its leg's section — a bare testid would match the request pane too.
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

  it("renders a JSON response body as copy-exact source", async () => {
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-json"]').getText();
    expect(shown).toContain('"greeting"');
    expect(shown).toContain('"hello"');
    // The braces and quotes DEC-01's rejected tree form dropped — what makes the
    // rendered document valid JSON when copied.
    expect(shown).toContain("{");
    expect(shown).toContain("}");
  });

  it("toggles to the raw stored bytes as text", async () => {
    await responseMode("raw").click();
    await inResponsePane('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const raw = await inResponsePane('[data-testid="body-text"]').getText();
    expect(raw).toContain('{"greeting":"hello"');
    await responseMode("decoded").click();
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
  });

  it("shows a runnable curl for the request", async () => {
    const curl = await $('[data-testid="curl"]').getText();
    expect(curl).toMatch(/^curl /);
    expect(curl).toContain("/json");
    expect(curl).toContain("-H");
  });

  it("renders a brotli-compressed JSON body decoded (decode chain via the UI)", async () => {
    await openRowFor("/br");
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-json"]').getText();
    expect(shown).toContain("compressed");
    expect(shown).toContain('"brotli"');
  });

  // Raw's whole job for a compressed body: the stored bytes are gzip frames, and
  // a hex dump of those answers no question. Raw reverses the transport encoding
  // and stops, so what shows is the minified source the server actually sent.
  it("shows a compressed body's source in raw, not a hex dump of its frames", async () => {
    await openRowFor("/br");
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });

    await responseMode("raw").click();
    await inResponsePane('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const raw = await inResponsePane('[data-testid="body-text"]').getText();
    expect(raw).toContain('"compressed":"brotli"');
    expect(await inResponsePane('[data-testid="body-hex"]').isExisting()).toBe(false);

    // Raw is the source, not a rendering: the decoded view indents, raw does not.
    expect(raw).not.toContain("\n  ");
    await responseMode("decoded").click();
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
  });

  it("renders an image body as an <img>", async () => {
    await openRowFor("/img");
    await inResponsePane('[data-testid="body-image"]').waitForExist({ timeout: 15_000 });
    const src = await inResponsePane('[data-testid="body-image"]').getAttribute("src");
    expect(src).toContain("blob:");
  });

  // The two contracts E-REDESIGN.W8 adds to UC-5.
  it("renders an XML body as XML, not as fall-through text", async () => {
    await openRowFor("/xml");
    await inResponsePane('[data-testid="body-xml"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-xml"]').getText();
    expect(shown).toContain("order");
    expect(shown).toContain("item");
    expect(await inResponsePane('[data-testid="body-text"]').isExisting()).toBe(false);
  });

  it("states its truncation on a body past the render cap, rather than silently cutting", async () => {
    await openRowFor("/big");
    await inResponsePane('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-text"]').getText();
    expect(shown).toMatch(/showing .+ of .+ — save to see all/);
  });

  it("shows both legs at once, each under its own heading", async () => {
    const detail = await $('[data-testid="detail"]').getText();
    expect(detail).toContain("Request body");
    expect(detail).toContain("Response body");
    expect(await $$('[data-testid="body-viewer"]').length).toBe(2);
  });
});
