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
// The panel shows one leg at a time under one selector (DEC-31), so the pane
// for a leg exists only while that leg is selected.
const legSegment = (leg: "request" | "response") =>
  $('[data-testid="detail"] header [role="radiogroup"]').$(`button=${leg}`);
const selectLeg = async (leg: "request" | "response") => {
  await legSegment(leg).click();
  await $(`[data-testid="body-viewer"][data-leg="${leg}"]`).waitForExist({ timeout: 15_000 });
};
// WebDriver cannot read the OS clipboard, so the panel's clipboard is replaced
// with a recorder before a copy and read back after — the payload is asserted
// for real, not inferred from a label.
const recordClipboard = () =>
  browser.execute(() => {
    const w = window as unknown as { __copied?: string };
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (text: string) => ((w.__copied = text), Promise.resolve()) },
    });
  });
const lastCopied = () => browser.execute(() => (window as unknown as { __copied?: string }).__copied);

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

  it("the method filter offers the verbs rather than taking free text", async () => {
    await $('button[aria-label="method"]').click();
    await $('[role="option"]').waitForExist({ timeout: 15_000 });
    const labels: string[] = [];
    for (const option of await $$('[role="option"]')) {
      labels.push(await option.getAttribute("aria-label"));
    }
    expect(labels).toContain("GET");
    expect(labels).toContain("any method");
    // The exactness the design flagged: the verb is matched whole server-side,
    // so a partial verb was never a legal query and is no longer typeable.
    expect(labels).not.toContain("GE");
    await browser.keys("Escape");
  });

  it("a filter narrows the list", async () => {
    // The free-text field is the one key that stays free text: the server
    // substring-matches it, so an arbitrary value is a legal query.
    await $('[aria-label="filter"]').setValue("no-such-path.invalid");
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
    await $('[aria-label="filter"]').clearValue();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "the list never came back after clearing the filter",
    });
  });

  it("the host filter offers the hosts capture has seen, and choosing one narrows to it", async () => {
    await $('button[aria-label="host"]').click();
    await $('[role="option"]').waitForExist({ timeout: 15_000 });
    // The harness origin is localhost on an ephemeral port, so the option is
    // matched by prefix rather than by a port this spec cannot know.
    let captured: WebdriverIO.Element | undefined;
    for (const option of await $$('[role="option"]')) {
      const label = await option.getAttribute("aria-label");
      if (label?.startsWith("localhost:")) captured = option;
    }
    expect(captured).toBeDefined();

    await captured!.click();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "choosing the captured host emptied the list",
    });

    await $('button[aria-label="host"]').click();
    await $('[role="option"][aria-label="any host"]').click();
    await browser.waitUntil(async () => (await rowCount()) >= 1, {
      timeout: 15_000,
      timeoutMsg: "the list never came back after choosing any host",
    });
  });

  it("the two dropdowns and the status field share the header row without wrapping", async () => {
    const box = async (sel: string) =>
      browser.execute((s: string) => {
        const r = document.querySelector(s)!.getBoundingClientRect();
        return { top: Math.round(r.top), w: Math.round(r.width) };
      }, sel);

    const host = await box('button[aria-label="host"]');
    const method = await box('button[aria-label="method"]');
    const status = await box('input[aria-label="status"]');

    // One row: the controls share a top edge, so the header did not wrap.
    expect(host.top).toBe(method.top);
    expect(method.top).toBe(status.top);
    // Select's own 132px floor is respected rather than crushed by the caller's width.
    expect(host.w).toBeGreaterThanOrEqual(132);
    expect(method.w).toBeGreaterThanOrEqual(132);
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
// headers, timing, versions, the leg selector, body viewers (type dispatch, raw
// toggle, stated truncation), copy-as-curl. The brotli step proves the W1 decode
// chain surfaces in the inspector. The curl command is never rendered (W5's
// design), so its payload is read back from a recording clipboard stub. The
// fresh launch is onboarding (UC-ONBOARD); the first decrypted flow reveals the
// list, so this block reveals it once up front before selecting rows to inspect.
//
// The panel opens on the request leg, so every response-body assertion selects
// the response leg first; the pane's data-leg then scopes the selector.
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

  async function openResponseOf(path: string) {
    await openRowFor(path);
    await selectLeg("response");
  }

  it("opens the detail on the request leg with headers, timing, and versions", async () => {
    await openRowFor("/json");
    const detail = await $('[data-testid="detail"]').getText();
    expect(detail).toContain("/json");
    expect(detail).toContain("HTTP/1.1");
    expect(detail).toMatch(/\d+ ms/);
    expect(detail.toLowerCase()).toContain("accept");
    expect(detail.toLowerCase()).not.toContain("content-type");
    expect(await legSegment("request").getAttribute("aria-checked")).toBe("true");
  });

  // DEC-31: one selector, and both leg-scoped zones follow it while the flow
  // line does not.
  it("the leg selector switches the headers and the body together", async () => {
    const lineBefore = await $('[data-testid="flow-line"]').getText();
    await selectLeg("response");
    const detail = await $('[data-testid="detail"]').getText();
    expect(detail.toLowerCase()).toContain("content-type");
    expect(detail.toLowerCase()).not.toContain("accept");
    expect(await $$('[data-testid="body-viewer"]').length).toBe(1);
    expect(await $('[data-testid="flow-line"]').getText()).toBe(lineBefore);
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

  it("toggles to the served source as text", async () => {
    await responseMode("raw").click();
    await inResponsePane('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const raw = await inResponsePane('[data-testid="body-text"]').getText();
    expect(raw).toContain('{"greeting":"hello"');
    await responseMode("decoded").click();
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
  });

  it("copies a runnable curl for the request and confirms at the button", async () => {
    const detail = await $('[data-testid="detail"]').getText();
    expect(detail).not.toMatch(/curl /);
    // The flow line's one button. CopyButton nests its label, so a text selector
    // (direct text nodes only) cannot find it; and WebKit's getText includes the
    // visibility:hidden widest-label span, so the text is matched, not equated.
    const copyAsCurl = $('[data-testid="flow-line"] button');
    expect(await copyAsCurl.getText()).toContain("copy as curl");
    expect(await copyAsCurl.getText()).not.toContain("copied");
    await recordClipboard();
    await copyAsCurl.click();
    await browser.waitUntil(async () => (await lastCopied()) !== undefined, {
      timeout: 5_000,
      timeoutMsg: "copy as curl wrote nothing",
    });
    const curl = (await lastCopied())!;
    expect(curl).toMatch(/^curl /);
    expect(curl).toContain("/json");
    expect(curl).toContain("-H");
    expect(await copyAsCurl.getText()).toContain("copied");
  });

  it("Esc dismisses the panel", async () => {
    await browser.keys("Escape");
    await browser.waitUntil(async () => !(await $('[data-testid="detail"]').isExisting()), {
      timeout: 5_000,
      timeoutMsg: "Esc left the panel open",
    });
  });

  it("renders a brotli-compressed JSON body decoded (decode chain via the UI)", async () => {
    await openResponseOf("/br");
    await inResponsePane('[data-testid="body-json"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-json"]').getText();
    expect(shown).toContain("compressed");
    expect(shown).toContain('"brotli"');
  });

  // Raw's whole job for a compressed body: the stored bytes are gzip frames, and
  // a hex dump of those answers no question. Raw reverses the transport encoding
  // and stops, so what shows is the minified source the server actually sent.
  it("shows a compressed body's source in raw, not a hex dump of its frames", async () => {
    await openResponseOf("/br");
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
    await openResponseOf("/img");
    await inResponsePane('[data-testid="body-image"]').waitForExist({ timeout: 15_000 });
    const src = await inResponsePane('[data-testid="body-image"]').getAttribute("src");
    expect(src).toContain("blob:");
  });

  // The two contracts E-REDESIGN.W8 adds to UC-5.
  it("renders an XML body as XML, not as fall-through text", async () => {
    await openResponseOf("/xml");
    await inResponsePane('[data-testid="body-xml"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-xml"]').getText();
    expect(shown).toContain("order");
    expect(shown).toContain("item");
    expect(await inResponsePane('[data-testid="body-text"]').isExisting()).toBe(false);
  });

  it("states its truncation on a body past the render cap, rather than silently cutting", async () => {
    await openResponseOf("/big");
    await inResponsePane('[data-testid="body-text"]').waitForExist({ timeout: 15_000 });
    const shown = await inResponsePane('[data-testid="body-text"]').getText();
    expect(shown).toMatch(/showing .+ of .+ — save to see all/);
  });
});
