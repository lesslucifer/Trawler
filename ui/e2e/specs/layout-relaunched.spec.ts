import { browser, expect } from "@wdio/globals";
import {
  SET_BEFORE_THE_QUIT,
  aFlowIsOpen,
  corruptTheSavedLayout,
  headersCeiling,
  hostHandle,
  paneSplitter,
  paneWidth,
  quitTheApp,
  shellIsUp,
  valueOf,
} from "./layout.ts";

// A new app process: the layout spec quit the last one after sizing everything.
describe("UC-4 / UC-5 — the layout survives a relaunch", () => {
  before(async () => {
    await shellIsUp();
    await aFlowIsOpen();
  });

  it("reopens the window, the pane, the headers share and the column as they were left", async () => {
    const { window: size, pane, host } = SET_BEFORE_THE_QUIT;
    const rect = await browser.getWindowRect();
    expect({ width: rect.width, height: rect.height }).toEqual(size);
    expect((await valueOf(paneSplitter())).now).toBe(pane);
    expect(await paneWidth()).toBe(pane);
    expect((await valueOf(hostHandle())).now).toBe(host);
    expect(await headersCeiling()).toBe("60%");
  });

  it("quits with a corrupt layout.json left for the next launch", async () => {
    corruptTheSavedLayout();
    await quitTheApp();
  });
});
