import { browser, $, expect } from "@wdio/globals";
import {
  DEFAULTS,
  SET_BEFORE_THE_QUIT,
  aFlowIsOpen,
  doubleClick,
  dragTo,
  headersCeiling,
  headersSplitter,
  hostHandle,
  paneSplitter,
  paneWidth,
  press,
  quitTheApp,
  savedLayout,
  savedLayoutBecomes,
  shellIsUp,
  valueOf,
  viewport,
} from "./layout.ts";

const columnEdges = () =>
  browser.execute(() => {
    const lefts = (parent: Element | null) =>
      Array.from(parent?.children ?? []).map((cell) => Math.round(cell.getBoundingClientRect().left));
    const row = document.querySelector('[data-testid="flow-row"]');
    return {
      labels: lefts(document.querySelector('[data-testid="column-labels"]')),
      cells: lefts(row),
      path: row!.querySelector('[data-wire="path"]')!.getBoundingClientRect().width,
    };
  });

const emitResetLayout = () =>
  browser.execute(() =>
    (window as unknown as { __TAURI__: { event: { emit: (name: string) => Promise<void> } } }).__TAURI__.event.emit(
      "layout-reset",
    ),
  );

// UC-4 / UC-5's user-sized layout through the real window, ending in a real quit:
// the next spec file is a new app process that must find all of it again.
describe("UC-4 / UC-5 — the user sizes the layout", () => {
  before(async () => {
    await shellIsUp();
    await aFlowIsOpen();
  });

  it("stops the list/pane split at the pane's bounds, whichever way it is dragged", async () => {
    const { width, height } = await viewport();
    await dragTo(paneSplitter(), 2, height / 2);
    const widest = await valueOf(paneSplitter());
    expect(widest.now).toBe(widest.max);
    expect(await paneWidth()).toBe(widest.max);

    await dragTo(paneSplitter(), width - 2, height / 2);
    const narrowest = await valueOf(paneSplitter());
    expect(narrowest.now).toBe(384);
    expect(await paneWidth()).toBe(384);
  });

  it("stops the headers/body split at its shares, and saves the share it was dropped at", async () => {
    const { height } = await viewport();
    const at = await headersSplitter().getLocation();
    await dragTo(headersSplitter(), at.x + 20, height - 2);
    await browser.waitUntil(async () => savedLayout()?.headersShare === 0.6, {
      timeout: 5_000,
      timeoutMsg: "the headers share dropped at the bottom was not saved as 60 %",
    });
    expect(await headersCeiling()).toBe("60%");

    await dragTo(headersSplitter(), at.x + 20, 2);
    await browser.waitUntil(async () => savedLayout()?.headersShare === 0.15, {
      timeout: 5_000,
      timeoutMsg: "the headers share dropped at the top was not saved as 15 %",
    });
    expect(await headersCeiling()).toBe("15%");
  });

  it("stops a column at its own bounds, keeps the path's floor and its labels over their cells", async () => {
    const { width, height } = await viewport();
    const y = (await hostHandle().getLocation()).y + 4;
    await dragTo(hostHandle(), width - 2, y);
    expect((await valueOf(hostHandle())).now).toBe(480);
    let edges = await columnEdges();
    expect(edges.labels).toEqual(edges.cells);
    expect(edges.path).toBeGreaterThanOrEqual(128);

    await dragTo(hostHandle(), 2, Math.min(y, height - 2));
    expect((await valueOf(hostHandle())).now).toBe(96);
    edges = await columnEdges();
    expect(edges.labels).toEqual(edges.cells);

    await doubleClick(hostHandle());
    expect((await valueOf(hostHandle())).now).toBe(180);
  });

  it("steps the pane by keyboard, jumps to its bounds, and a double-click returns its default", async () => {
    await doubleClick(paneSplitter());
    expect((await valueOf(paneSplitter())).now).toBe(520);

    await press(paneSplitter(), "ArrowLeft");
    expect((await valueOf(paneSplitter())).now).toBe(528);
    await press(paneSplitter(), "ArrowRight");
    expect((await valueOf(paneSplitter())).now).toBe(520);

    await press(paneSplitter(), "End");
    const end = await valueOf(paneSplitter());
    expect(end.now).toBe(end.max);
    await press(paneSplitter(), "Home");
    expect((await valueOf(paneSplitter())).now).toBe(384);

    await doubleClick(paneSplitter());
    expect((await valueOf(paneSplitter())).now).toBe(520);
  });

  it("returns every default on View ▸ Reset Layout and saves them", async () => {
    await press(paneSplitter(), "Home");
    await press(hostHandle(), "ArrowRight", 2);
    await press(headersSplitter(), "End");
    await savedLayoutBecomes({ ...DEFAULTS, pane: 384, headersShare: 0.6, columns: { ...DEFAULTS.columns, host: 196 } });

    await emitResetLayout();

    await savedLayoutBecomes(DEFAULTS);
    expect((await valueOf(paneSplitter())).now).toBe(520);
    expect((await valueOf(hostHandle())).now).toBe(180);
    expect(await headersCeiling()).toBe("35%");
  });

  it("saves the pane, the headers share, a column and the window, then quits", async () => {
    const { window: size, pane, headersShare, host } = SET_BEFORE_THE_QUIT;
    await browser.setWindowRect(null, null, size.width, size.height);
    await browser.waitUntil(async () => (await viewport()).width === size.width, {
      timeout: 5_000,
      timeoutMsg: "the window never took the width it was set to",
    });

    await press(paneSplitter(), "ArrowLeft", (pane - DEFAULTS.pane) / 8);
    await press(hostHandle(), "ArrowRight", (host - DEFAULTS.columns.host) / 8);
    await press(headersSplitter(), "End");
    await savedLayoutBecomes({ ...DEFAULTS, pane, headersShare, columns: { ...DEFAULTS.columns, host } });
    expect(await $('[data-testid="detail-pane"]').isDisplayed()).toBe(true);

    await quitTheApp();
  });
});
