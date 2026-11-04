import { browser, $, $$, expect } from "@wdio/globals";
import {
  DEFAULTS,
  aFlowIsOpen,
  clearTheCapture,
  headersCeiling,
  hostHandle,
  paneSplitter,
  paneWidth,
  shellIsUp,
  valueOf,
  viewport,
} from "./layout.ts";

// A new app process launched over the corrupt layout.json the last one left. It
// hands the flows spec the capture empty and the window at its configured size.
describe("UC-4 — a saved layout the app cannot read", () => {
  before(async () => {
    await shellIsUp();
    await aFlowIsOpen();
  });

  after(async () => {
    await browser.setWindowRect(null, null, 1440, 800);
    await browser.waitUntil(async () => (await viewport()).width === 1440, {
      timeout: 5_000,
      timeoutMsg: "the window never returned to its configured width",
    });
    await clearTheCapture();
  });

  it("opens at the default layout and says nothing about it", async () => {
    expect((await valueOf(paneSplitter())).now).toBe(DEFAULTS.pane);
    expect(await paneWidth()).toBe(DEFAULTS.pane);
    expect((await valueOf(hostHandle())).now).toBe(DEFAULTS.columns.host);
    expect(await headersCeiling()).toBe("35%");
    expect(await $('[data-testid="band"]').isExisting()).toBe(false);
    expect(await $$('[data-tone="warn"]').length).toBe(0);
  });
});
