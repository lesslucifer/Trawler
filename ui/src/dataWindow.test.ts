import { afterEach, expect, test, vi } from "vitest";
import {
  claimOnce,
  dataWindowLabel,
  isDataWindow,
  framePayload,
  openDataWindow,
  openInBrowser,
  PAYLOAD_REGISTRY,
  takeDataPayload,
  takeInBrowser,
  unframePayload,
  type DataPayload,
} from "./dataWindow";

const payload = (key: string, bytes = [0, 255, 7]): DataPayload => ({
  key,
  title: `Response body · ${key}`,
  bytes: new Uint8Array(bytes),
  typeHint: "application/json",
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

test("a label is the shell's: the prefix and the key's UTF-8 in lowercase hex", () => {
  expect(dataWindowLabel("ab")).toBe("data-6162");
  expect(dataWindowLabel("")).toBe("data-");
  expect(dataWindowLabel("é/1")).toBe("data-c3a92f31");
});

test("a framed payload reads back as itself, with or without a type hint", () => {
  const bare: DataPayload = { key: "k", title: "t", bytes: new Uint8Array([1, 2]) };
  expect(unframePayload(framePayload(payload("k")))).toEqual(payload("k"));
  expect(unframePayload(framePayload(bare))).toEqual(bare);
});

test("the frame is a big-endian caption length, the caption as JSON, then the bytes", () => {
  const framed = framePayload(payload("k", [66, 79, 68, 89]));
  const length = new DataView(framed.buffer).getUint32(0);
  const caption = JSON.parse(new TextDecoder().decode(framed.subarray(4, 4 + length)));

  expect(caption).toEqual({ key: "k", title: "Response body · k", typeHint: "application/json" });
  expect(Array.from(framed.subarray(4 + length))).toEqual([66, 79, 68, 89]);
});

test("a frame that is short, overlong or not a caption reads as nothing", () => {
  expect(unframePayload(new Uint8Array([0, 0]))).toBeNull();
  expect(unframePayload(new Uint8Array([0, 0, 0, 9, 123]))).toBeNull();
  expect(unframePayload(new Uint8Array([0, 0, 0, 1, 120]))).toBeNull();
  expect(unframePayload(new Uint8Array([0, 0, 0, 2, 123, 125]))).toBeNull();
});

test("in the shell, opening sends the framed payload as the raw body", async () => {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
  const call = vi.fn(async () => undefined) as never;

  await openDataWindow(payload("k"), call);

  expect(call).toHaveBeenCalledWith("open_data_window", framePayload(payload("k")));
});

test("in the shell, a claim reads the raw answer and an empty one is nothing", async () => {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
  const framed = framePayload(payload("k"));
  const answering = vi.fn(async () => framed.buffer) as never;
  const empty = vi.fn(async () => new ArrayBuffer(0)) as never;

  await expect(takeDataPayload(answering)).resolves.toEqual(payload("k"));
  await expect(takeDataPayload(empty)).resolves.toBeNull();
  expect(answering).toHaveBeenCalledWith("take_data_payload");
});

function browserHost() {
  const children: Array<{ name: string; closed: boolean; focus: ReturnType<typeof vi.fn>; opener: unknown }> = [];
  const host = {
    location: { pathname: "/" },
    open: vi.fn((_url: string, name: string) => {
      const child = { name, closed: false, focus: vi.fn(), opener: host };
      children.push(child);
      return child;
    }),
  } as unknown as Window & { [PAYLOAD_REGISTRY]?: Map<string, DataPayload> };
  return { host, children };
}

test("in a browser, the payload waits on the opener under the window's name and is claimed once", () => {
  const { host, children } = browserHost();

  openInBrowser(payload("browser-once"), host);
  const child = children[0] as unknown as Window;

  expect(host.open).toHaveBeenCalledWith("/", dataWindowLabel("browser-once"));
  expect(takeInBrowser(child)).toEqual(payload("browser-once"));
  expect(takeInBrowser(child)).toBeNull();
});

test("in a browser, opening a key whose window is open focuses it and parks nothing new", () => {
  const { host, children } = browserHost();

  openInBrowser(payload("browser-focus", [1]), host);
  openInBrowser(payload("browser-focus", [2]), host);

  expect(host.open).toHaveBeenCalledTimes(1);
  expect(children[0].focus).toHaveBeenCalledTimes(1);
  expect(takeInBrowser(children[0] as unknown as Window)?.bytes).toEqual(new Uint8Array([1]));
});

test("in a browser, a closed window's key opens a new one", () => {
  const { host, children } = browserHost();

  openInBrowser(payload("browser-closed", [1]), host);
  children[0].closed = true;
  openInBrowser(payload("browser-closed", [2]), host);

  expect(host.open).toHaveBeenCalledTimes(2);
  expect(takeInBrowser(children[1] as unknown as Window)?.bytes).toEqual(new Uint8Array([2]));
});

test("in a browser, a blocked popup leaves nothing parked on the opener", () => {
  const { host } = browserHost();
  (host.open as ReturnType<typeof vi.fn>).mockReturnValueOnce(null);

  openInBrowser(payload("browser-blocked"), host);

  expect(host[PAYLOAD_REGISTRY]?.has(dataWindowLabel("browser-blocked")) ?? false).toBe(false);
});

test("in a browser, a window that is not a data window claims nothing", () => {
  expect(takeInBrowser({ name: "", opener: null } as unknown as Window)).toBeNull();
  expect(takeInBrowser({ name: "data-6b", opener: null } as unknown as Window)).toBeNull();
});

test("a window is a data window by its label in the shell, by its name in a browser", () => {
  expect(isDataWindow({ name: "data-6b" } as Window)).toBe(true);
  expect(isDataWindow({ name: "" } as Window)).toBe(false);
  const shell = { __TAURI_INTERNALS__: { metadata: { currentWindow: { label: "data-6b" } } }, name: "" };
  Object.assign(window, { __TAURI_INTERNALS__: shell.__TAURI_INTERNALS__ });
  expect(isDataWindow(shell as unknown as Window)).toBe(true);
  Object.assign(window, { __TAURI_INTERNALS__: { metadata: { currentWindow: { label: "main" } } } });
  expect(isDataWindow(window)).toBe(false);
});

test("a screen that mounts twice shares the one claim instead of claiming null the second time", async () => {
  let left: DataPayload | null = payload("k");
  const take = vi.fn(async () => {
    const claimed = left;
    left = null;
    return claimed;
  });

  const [first, second] = await Promise.all([claimOnce(take), claimOnce(take)]);

  expect(take).toHaveBeenCalledOnce();
  expect(first).toEqual(payload("k"));
  expect(second).toBe(first);
});
