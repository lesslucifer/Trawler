import { browser, $, $$ } from "@wdio/globals";

export const draft = () => $('input[role="combobox"][aria-label="filter"]');
export const suggestions = () => $$('[role="listbox"][aria-label="suggestions"] [role="option"]');
export const highlighted = () => $('[role="listbox"][aria-label="suggestions"] [role="option"][aria-selected="true"]');
export const chipElements = () => $$('[data-testid="chip"]');

export async function focusDraft() {
  await draft().click();
}

export async function type(keys: string) {
  await browser.keys(keys.split(""));
}

export async function press(key: "Tab" | "Enter" | "Backspace" | "Escape" | " ") {
  await browser.keys(key);
}

export async function chips(): Promise<string[]> {
  const labels: string[] = [];
  for (const chip of await chipElements()) {
    const label = await chip.$("button").getAttribute("aria-label");
    labels.push(label.replace(/^remove /, ""));
  }
  return labels;
}

export async function chipsRead(expected: string[]) {
  let held: string[] = [];
  await browser
    .waitUntil(
      async () => {
        held = await chips();
        return JSON.stringify(held) === JSON.stringify(expected);
      },
      { timeout: 15_000 },
    )
    .catch(async () => {
      const typed = await draft().getValue();
      throw new Error(`the bar held [${held.join(" | ")}] with draft “${typed}”, not [${expected.join(" | ")}]`);
    });
}

export async function suggestionTexts(): Promise<string[]> {
  const texts: string[] = [];
  for (const option of await suggestions()) texts.push((await option.$("span").getText()).trim());
  return texts;
}

export async function addChips(text: string) {
  await focusDraft();
  await type(`${text} `);
}

export async function emptyTheDraft() {
  await focusDraft();
  while (((await draft().getValue()) ?? "") !== "") await press("Backspace");
}

export async function removeEveryChip() {
  await emptyTheDraft();
  while ((await chipElements()).length > 0) {
    const before = (await chipElements()).length;
    await press("Backspace");
    await browser.waitUntil(async () => (await chipElements()).length < before, {
      timeout: 5_000,
      timeoutMsg: "Backspace in the empty draft left the last chip",
    });
  }
}

export interface Row {
  method: string;
  host: string;
  path: string;
  status: string;
}

export const rows = () =>
  browser.execute(() =>
    Array.from(document.querySelectorAll('[data-testid="flow-row"]')).map((row) => {
      const cell = (wire: string) => row.querySelector(`[data-wire="${wire}"]`)?.textContent?.trim() ?? "";
      return { method: cell("method"), host: cell("host"), path: cell("path"), status: cell("status") };
    }),
  );

export const urlsOf = (listed: Row[]) => listed.map((row) => `${row.host}${row.path}`).sort();

export async function rowsAre(expected: string[], what: string) {
  const want = [...expected].sort();
  await browser.waitUntil(async () => JSON.stringify(urlsOf(await rows())) === JSON.stringify(want), {
    timeout: 15_000,
    timeoutMsg: `${what}: the list never read ${want.join(", ")}`,
  });
}

// The window's own requests are the only proof that a keystroke listed nothing, so
// its fetch is wrapped once and every listing it sends is kept in order.
export async function recordListings() {
  await browser.execute(() => {
    const w = window as unknown as { __listings?: string[]; __unwrappedFetch?: typeof fetch };
    w.__listings = [];
    if (w.__unwrappedFetch) return;
    const original = window.fetch.bind(window);
    w.__unwrappedFetch = original;
    window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
      const listing = url.pathname === "/flows" && (init?.method ?? "GET") === "GET";
      if (listing && !url.searchParams.has("limit")) w.__listings!.push(url.search);
      return original(input, init);
    };
  });
}

export const listings = () =>
  browser.execute(() => (window as unknown as { __listings?: string[] }).__listings ?? []);

export async function listingsAfter(count: number, what: string): Promise<string[]> {
  await browser.waitUntil(async () => (await listings()).length > count, {
    timeout: 15_000,
    timeoutMsg: `${what}: the window never listed`,
  });
  return (await listings()).slice(count);
}

export async function noListingWhile(act: () => Promise<void>, what: string) {
  const before = (await listings()).length;
  await act();
  await browser.pause(750);
  const after = await listings();
  if (after.length !== before) throw new Error(`${what} listed ${after.slice(before).join(" · ")}`);
}
