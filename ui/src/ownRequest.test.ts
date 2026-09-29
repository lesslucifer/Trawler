import { afterEach, describe, expect, it, vi } from "vitest";
import { makeRequest, REQUEST_HOST } from "./ownRequest";

describe("the request the app makes of its own (DEC-56)", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("goes to the named host over HTTPS, uncached, without asking to read the answer", async () => {
    const fetch = vi.fn(async () => new Response(null));
    vi.stubGlobal("fetch", fetch);

    await makeRequest();

    expect(REQUEST_HOST).toBe("example.com");
    expect(fetch).toHaveBeenCalledOnce();
    expect(fetch).toHaveBeenCalledWith("https://example.com/", { mode: "no-cors", cache: "no-store" });
  });

  it("fails when the request does not complete, so the screen can say so", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("Load failed"))));

    await expect(makeRequest()).rejects.toThrow("Load failed");
  });
});
