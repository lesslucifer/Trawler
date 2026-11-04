import { $, $$, expect } from "@wdio/globals";
import { bodyModeIs, clearTheCapture, legIs, openRow, pickLeg, responseBody, shellIsUp } from "./sticky.ts";

// A new app process launched over the corrupt view-prefs.json the last one left.
// It hands the layout spec the capture empty, as replay left it.
describe("UC-5 — saved choices the app cannot read", () => {
  before(async () => {
    await shellIsUp();
    await $('[data-testid="flow-row"]').waitForExist({ timeout: 15_000 });
  });

  after(async () => {
    await clearTheCapture();
  });

  it("opens a flow on Request and pretty, and says nothing about it", async () => {
    await openRow(0);
    expect(await legIs("request")).toBe(true);

    await pickLeg("response");
    expect(await bodyModeIs("pretty")).toBe(true);
    await responseBody("json").waitForExist({ timeout: 15_000 });

    expect(await $('[data-testid="band"]').isExisting()).toBe(false);
    expect(await $$('[data-tone="warn"]').length).toBe(0);
  });
});
