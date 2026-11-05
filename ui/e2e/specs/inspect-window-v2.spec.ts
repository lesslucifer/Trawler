import { browser, $, $$, expect } from "@wdio/globals";
import {
  MAIN,
  RESPONSE_PANE,
  SURFACE,
  capturedBody,
  copied,
  copyOnScreen,
  dataWindows,
  editCountReads,
  findReads,
  flowIdOf,
  fromMore,
  hit,
  onNewDataWindow,
  pick,
  ports,
  recordClipboard,
  rightClickOn,
  shellIsUp,
  showFlow,
} from "./data.ts";
import { clearTheCapture } from "./layout.ts";
import { addChips, removeEveryChip, rowsAre } from "./query.ts";

const WIRE = "/wire";
const COOKIE = `session=${"c".repeat(4090)}`;
const JWT =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0cmF3bGVyIiwiZXhwIjoxNzkzODUxMjAwfQ.bm90LWEtcmVhbC1zaWduYXR1cmU";
const LONG_QUERY = "?q=" + "u".repeat(2048);
const OPEN_ALL = (leg: "request" | "response") => `button[aria-label="open all ${leg} headers in data window"]`;
const PREVIEW = '[data-testid="data-preview"]';
const BEACON_QUIET_MS = 2_500;

async function flowDetail(path: string) {
  const id = await flowIdOf(path);
  return (await (await fetch(`${ports.api}/flows/${encodeURIComponent(id)}`)).json()) as {
    uri: string;
    request_headers: { name: string; value: string }[];
    response_headers: { name: string; value: string }[];
  };
}

async function oneLineAndCut(selector: string) {
  return browser.execute((sel: string) => {
    const box = document.querySelector(sel) as HTMLElement;
    const style = getComputedStyle(box);
    return {
      oneLine: box.getBoundingClientRect().height <= parseFloat(style.fontSize) * 2,
      cut: box.scrollWidth > box.clientWidth,
      ellipsis: style.textOverflow === "ellipsis" && style.whiteSpace === "nowrap",
    };
  }, selector);
}

const panelOverflowsSideways = () =>
  browser.execute(() => {
    const scroll = document.querySelector('[data-testid="headers-scroll"]') as HTMLElement;
    return scroll.scrollWidth > scroll.clientWidth || document.documentElement.scrollWidth > window.innerWidth;
  });

async function windowLines(): Promise<string[]> {
  return browser.execute((surface: string) => Array.from(document.querySelectorAll(`${surface} .cm-line`)).map((line) => line.textContent ?? ""), SURFACE);
}

async function closeEveryDataWindow() {
  for (const handle of await dataWindows()) {
    await browser.switchToWindow(handle);
    await browser.execute(() => {
      const tauri = (window as unknown as { __TAURI__: { window: { getCurrentWindow: () => { close: () => Promise<void> } } } }).__TAURI__;
      setTimeout(() => void tauri.window.getCurrentWindow().close(), 50);
    });
    await browser.waitUntil(async () => !(await browser.getWindowHandles()).includes(handle), {
      timeout: 15_000,
      timeoutMsg: `${handle} never closed`,
    });
  }
  await browser.switchToWindow(MAIN);
}

async function openBodyInWindow(known: string[], pane = RESPONSE_PANE) {
  const open = $(pane).$('button[aria-label="open in data window"]');
  await open.waitForExist({ timeout: 15_000 });
  await open.click();
  return onNewDataWindow(known);
}

const detected = async () => (await $('[data-testid="detected"]').getText()).replace(/^detected\s+/, "");

const beaconRequests = async () => (await (await fetch(`${ports.control}/beacon`)).json()) as string[];
const resetBeacon = () => fetch(`${ports.control}/beacon/reset`);

async function previewOf(path: string) {
  await resetBeacon();
  await browser.switchToWindow(MAIN);
  await showFlow(path, "response", "pretty");
  const opened = await openBodyInWindow(await dataWindows());
  await $("button=preview").click();
  await $(PREVIEW).waitForDisplayed({ timeout: 5_000 });
  return opened;
}

// E-INSPWIN2 end to end through the real windows: values that stay on one line
// and open whole, the bytes deciding the kind, a script's source reformatted and
// restored, an HTML preview that holds nothing, and find typed like the filter.
// It leaves the capture empty and no data window open, as the specs around it do.
describe("E-INSPWIN2 — inspect long values and read data by its bytes", () => {
  before(async () => {
    await shellIsUp();
    await resetBeacon();
  });

  after(async () => {
    await closeEveryDataWindow();
    await clearTheCapture();
  });

  describe("J1 · long values on one line", () => {
    let uri = "";
    let headersWindow = "";

    it("each long value is one line, cut at its end, and the panel does not scroll sideways", async () => {
      await showFlow(`${WIRE}${LONG_QUERY}`, "response", undefined, `authorization:Bearer ${JWT}`);
      const detail = await flowDetail(WIRE);
      uri = detail.uri;
      expect(uri.length).toBeGreaterThan(2048);

      await $('[aria-label="copy set-cookie"]').waitForDisplayed({ timeout: 5_000 });
      expect(await oneLineAndCut('[aria-label="copy set-cookie"]')).toEqual({ oneLine: true, cut: true, ellipsis: true });
      expect(await oneLineAndCut('[aria-label="copy uri"]')).toEqual({ oneLine: true, cut: true, ellipsis: true });
      expect(await panelOverflowsSideways()).toBe(false);
    });

    it("a click on a cut value copies the whole of it", async () => {
      await recordClipboard();
      expect(await copied(() => $('[aria-label="copy set-cookie"]').click())).toBe(COOKIE);
      expect(await copied(() => $('[aria-label="copy uri"]').click())).toBe(uri);
    });

    it("open all hands the leg's headers, every value whole and in wire order", async () => {
      await $(OPEN_ALL("response")).click();
      headersWindow = await onNewDataWindow([]);
      const { response_headers } = await flowDetail(WIRE);
      const wire = response_headers.map(({ name, value }) => `${name}: ${value}`);
      await browser.waitUntil(async () => (await windowLines()).join("\n") === wire.join("\n"), {
        timeout: 5_000,
        timeoutMsg: "the window never held every response header whole, in wire order",
      });
      expect((await windowLines()).some((line) => line === `set-cookie: ${COOKIE}`)).toBe(true);
    });

    it("the request's JWT decodes in the window to its header and payload", async () => {
      await browser.switchToWindow(MAIN);
      await $('[data-testid="detail"] header [role="radiogroup"]').$("button=request").click();
      await $(OPEN_ALL("request")).click();
      await onNewDataWindow([headersWindow]);
      await rightClickOn(JWT);
      await pick("decode");
      const peek = $('[data-testid="decode-peek"]');
      await peek.waitForDisplayed({ timeout: 5_000 });
      const reading = await peek.getText();
      expect(reading).toContain("JWT");
      expect(reading).toContain("HS256");
      expect(reading).toContain("trawler");
    });

    it("open on the URI hands the whole URI, and a second open all focuses the first window", async () => {
      await browser.switchToWindow(MAIN);
      const known = await dataWindows();
      await $('button[aria-label="open uri in data window"]').click();
      await onNewDataWindow(known);
      await recordClipboard();
      expect(await copyOnScreen()).toBe(uri);

      await browser.switchToWindow(MAIN);
      const before = await dataWindows();
      await $('[data-testid="detail"] header [role="radiogroup"]').$("button=response").click();
      await $(OPEN_ALL("response")).click();
      await browser.pause(1_500);
      expect((await dataWindows()).sort()).toEqual([...before].sort());
      await closeEveryDataWindow();
    });
  });

  describe("J2 · the bytes decide", () => {
    const servedAs = () => $('[data-testid="served-as"]');

    it("JSON served as text/plain is a tree in the pane, with its statement, and JSON in the window", async () => {
      await showFlow("/plain-json", "response", "pretty");
      await servedAs().waitForDisplayed({ timeout: 15_000 });
      expect(await servedAs().getText()).toBe("JSON · served as text/plain");
      expect(await $('[data-testid="body-text"]').isExisting()).toBe(false);

      await openBodyInWindow([]);
      expect(await detected()).toBe("read from its bytes · served as text/plain");
      expect(await $('[aria-label="treat the data as"]').getText()).toContain("JSON");
      await closeEveryDataWindow();
    });

    it("HTML served as application/octet-stream is html in the pane and the window", async () => {
      await showFlow("/octet-html", "response", "pretty");
      await servedAs().waitForDisplayed({ timeout: 15_000 });
      expect(await servedAs().getText()).toBe("HTML · served as application/octet-stream");

      await openBodyInWindow([]);
      expect(await detected()).toBe("read from its bytes · served as application/octet-stream");
      expect(await $("button=preview").isExisting()).toBe(true);
      await closeEveryDataWindow();
    });

    it("a text/plain body starting {oops stays text in the pane and the window", async () => {
      await showFlow("/oops", "response", "pretty");
      await $('[data-testid="body-text"]').waitForDisplayed({ timeout: 15_000 });
      expect(await servedAs().isExisting()).toBe(false);

      await openBodyInWindow([]);
      expect(await detected()).toBe("from text/plain");
      expect(await $("button=preview").isExisting()).toBe(false);
      await closeEveryDataWindow();
    });
  });

  describe("J3 · javascript", () => {
    let served = "";

    const lines = async () => Number(/(\d+) lines?/.exec(await $("footer").getText())?.[1]);

    async function reformatWhenLoaded() {
      await browser.waitUntil(
        async () => {
          await $('button[aria-label="more"]').click();
          await $('[role="menu"]').waitForDisplayed({ timeout: 5_000 });
          for (const item of await $$('[role="menu"] [role="menuitem"]')) {
            if ((await item.$("span.flex-1").getText()) === "reformat" && (await item.getAttribute("aria-disabled")) === null) {
              await item.click();
              return true;
            }
          }
          await browser.keys("Escape");
          return false;
        },
        { timeout: 15_000, interval: 500, timeoutMsg: "reformat never became available" },
      );
    }

    it("a minified script opens as javascript and find counts a seeded identifier", async () => {
      await showFlow("/app.js", "response", "pretty");
      served = await capturedBody("/app.js");
      await openBodyInWindow([]);
      expect(await detected()).toBe("from application/javascript");
      await findReads("needleAlpha", "1 of 2");
    });

    it("reformat makes more lines and one edit, undo makes it unmodified", async () => {
      const minified = await lines();
      await reformatWhenLoaded();
      await editCountReads("modified · 1 edit");
      await browser.waitUntil(async () => (await lines()) > minified, { timeout: 5_000, timeoutMsg: "reformat never added a line" });
      await fromMore("undo");
      await editCountReads(null);
    });

    it("reformat again, then restore original yields the served bytes", async () => {
      await reformatWhenLoaded();
      await editCountReads("modified · 1 edit");
      await fromMore("restore original");
      await editCountReads(null);
      await recordClipboard();
      expect(await copyOnScreen()).toBe(served);
      await closeEveryDataWindow();
    });
  });

  describe("J4 · the html preview holds nothing", () => {
    const linksIn = (markup: string) =>
      browser.execute((html: string) => {
        const page = new DOMParser().parseFromString(html, "text/html");
        return Array.from(page.querySelectorAll(":any-link")).map(
          (link) => `${link.localName}${link.getAttribute("target") === "_blank" ? " _blank" : ""}`,
        );
      }, markup);
    const previewSource = () => browser.execute(() => (document.querySelector('[data-testid="data-preview"]') as HTMLIFrameElement).srcdoc);

    it("control: the same page, unguarded, reaches the beacon by every road the preview closes", async () => {
      await hit("/page");
      await resetBeacon();
      const raw = await capturedBody("/page");
      await browser.execute((html: string) => {
        const frame = document.createElement("iframe");
        frame.id = "unguarded";
        frame.srcdoc = html;
        document.body.appendChild(frame);
      }, raw);
      await browser.pause(BEACON_QUIET_MS);
      await browser.execute(() => document.getElementById("unguarded")?.remove());
      const roads = (await beaconRequests()).map((url) => url.split("?")[0]);
      expect(roads).toEqual(expect.arrayContaining(["/img", "/css", "/script", "/refresh"]));
    });

    it("the preview of that page asks for none of it", async () => {
      await previewOf("/page");
      await browser.pause(BEACON_QUIET_MS);
      expect(await beaconRequests()).toEqual([]);
      await closeEveryDataWindow();
    });

    const LINK_TAG: Record<string, string> = { anchor: "a", math: "math", svg: "a" };
    for (const [by, tag] of Object.entries(LINK_TAG)) {
      it(`a ${by} link in the page is a link there and none in the preview, or one aimed at a context the sandbox withholds`, async () => {
        const path = `/link/${by}`;
        await previewOf(path);
        expect((await linksIn(await capturedBody(path))).map((link) => link.replace(" _blank", ""))).toContain(tag);
        const guarded = await linksIn(await previewSource());
        expect(guarded.every((link) => link === "a _blank")).toBe(true);
        await closeEveryDataWindow();
      });
    }
  });

  describe("J5 · find like the filter", () => {
    const findInput = () => $('input[role="combobox"][aria-label="find"]');
    const findChips = async () => {
      const labels: string[] = [];
      for (const chip of await $$('[data-testid="chip"]')) labels.push((await chip.$("button").getAttribute("aria-label")).replace(/^remove /, ""));
      return labels;
    };
    const chipsRead = async (expected: string[]) => {
      try {
        await browser.waitUntil(async () => JSON.stringify(await findChips()) === JSON.stringify(expected), { timeout: 5_000 });
      } catch {
        throw new Error(`find held [${(await findChips()).join(" | ")}] with draft “${await findInput().getValue()}”, not [${expected.join(" | ")}]`);
      }
    };
    const countReads = (count: string) =>
      browser.waitUntil(async () => (await $('[data-testid="find-count"]').getText()) === count, {
        timeout: 5_000,
        timeoutMsg: `find never counted ${count}`,
      });
    const suggested = async () => {
      const texts: string[] = [];
      for (const option of await $$('[role="listbox"][aria-label="suggestions"] [role="option"]')) texts.push((await option.getText()).trim());
      return texts;
    };

    it("key: is offered while typing, a chosen key makes a chip and counts its matches", async () => {
      await showFlow("/find-json", "response", "pretty");
      await openBodyInWindow([]);
      await findInput().click();
      await browser.keys(["k", "e"]);
      await browser.waitUntil(async () => (await suggested()).some((text) => text.startsWith("key:")), {
        timeout: 5_000,
        timeoutMsg: "typing ke never offered key:",
      });
      await browser.keys("ArrowDown");
      await browser.keys("Enter");
      await browser.waitUntil(async () => (await suggested()).some((text) => text.startsWith("key:alpha")), {
        timeout: 5_000,
        timeoutMsg: "key: never offered the document's keys",
      });
      await browser.keys(["a", "l", "p", "h", "a"]);
      await browser.keys(" ");
      await chipsRead(["key:alpha"]);
      await countReads("1 of 1");
    });

    it("a second chip sums with the first and Enter steps across both", async () => {
      await browser.keys('"user'.split(""));
      await browser.execute(() => document.execCommand("insertText", false, " "));
      await browser.keys('name" '.split(""));
      await chipsRead(["key:alpha", '"user name"']);
      await countReads("1 of 3");
      await browser.keys("Enter");
      await countReads("2 of 3");
    });

    it("a draft that does not parse makes no chip, and Backspace removes the last chip", async () => {
      await browser.keys("re:( ".split(""));
      await chipsRead(["key:alpha", '"user name"']);
      expect(await findInput().getValue()).toMatch(/^re:\(/);
      while ((await findInput().getValue()) !== "") await browser.keys("Backspace");
      await browser.keys("Backspace");
      await chipsRead(["key:alpha"]);
      await countReads("1 of 1");
    });

    it("switching the kind to text refuses the key: chip", async () => {
      await $('[aria-label="treat the data as"]').click();
      await $('[role="listbox"][aria-label="treat the data as"] [role="option"][aria-label="text"]').click();
      await browser.waitUntil(async () => (await $('[data-testid="chip"]').getAttribute("aria-invalid")) === "true", {
        timeout: 5_000,
        timeoutMsg: "the key: chip was never refused on text",
      });
      await closeEveryDataWindow();
    });

    it("the flow list's filter in the main window still makes chips and narrows the list", async () => {
      await fetch(`${ports.control}/hit?p=missing`);
      await addChips("status:4xx");
      await rowsAre([`${new URL(ports.origin).host}/missing`], "status:4xx");
      await removeEveryChip();
    });
  });
});
