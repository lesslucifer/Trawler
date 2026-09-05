import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { KeyValueRow } from "./KeyValueRow";

const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
  vi.spyOn(window, "getSelection").mockReturnValue({ isCollapsed: true } as unknown as Selection);
});

afterEach(() => vi.restoreAllMocks());

describe("KeyValueRow", () => {
  it("renders the name muted and inert, the value as content", () => {
    render(
      <dl>
        <KeyValueRow name="content-type" value="application/json" />
      </dl>,
    );

    const name = screen.getByText("content-type");
    expect(name.tagName).toBe("DT");
    expect(name.className).toContain("text-ink-mute");
    expect(name.className).toContain("whitespace-nowrap");
    expect(screen.getByText("application/json")).toBeInTheDocument();
  });

  it("the value cell carries CopyText's affordance — the conflict the sheet exposed", async () => {
    render(
      <dl>
        <KeyValueRow name="content-type" value="application/json" />
      </dl>,
    );

    const value = screen.getByRole("button", { name: "copy content-type" });
    fireEvent.click(value);
    await act(async () => {});

    expect(writeText).toHaveBeenCalledWith("application/json");
    expect(screen.getByText("copied")).toBeInTheDocument();
  });

  it("the name is not itself a copy target — only the value is", () => {
    render(
      <dl>
        <KeyValueRow name="content-type" value="application/json" />
      </dl>,
    );

    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("participates in the parent grid rather than imposing its own layout", () => {
    const { container } = render(
      <dl>
        <KeyValueRow name="a" value="b" />
      </dl>,
    );

    expect(container.querySelector("dl > div")?.className).toBe("contents");
  });

  it("a long value wraps, never truncates", () => {
    const long = "x".repeat(300);
    render(
      <dl>
        <KeyValueRow name="cookie" value={long} />
      </dl>,
    );

    expect(screen.getByText(long).className).toContain("break-all");
  });

  describe("collapsible", () => {
    const cookie = "session=" + "x".repeat(900);
    // jsdom lays nothing out: every <dd> is declared 60px tall with 200px of content.
    const overflowing = (on: boolean) => {
      if (!on) {
        delete (HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
        delete (HTMLElement.prototype as { clientHeight?: number }).clientHeight;
        return;
      }
      Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
        configurable: true,
        get(this: HTMLElement) {
          return this.tagName === "DD" ? 200 : 0;
        },
      });
      Object.defineProperty(HTMLElement.prototype, "clientHeight", {
        configurable: true,
        get(this: HTMLElement) {
          return this.tagName === "DD" ? 60 : 0;
        },
      });
    };
    afterEach(() => overflowing(false));

    it("is off by default — a long value wraps in full and the only button is the value", () => {
      overflowing(true);
      render(
        <dl>
          <KeyValueRow name="set-cookie" value={cookie} />
        </dl>,
      );
      expect(screen.getAllByRole("button")).toHaveLength(1);
      expect(screen.getByText(cookie).closest("dd")!.className).toBe("");
    });

    it("clamps an overflowing value to three lines, keeps the whole value copyable, and expands on one act", async () => {
      overflowing(true);
      render(
        <dl>
          <KeyValueRow name="set-cookie" value={cookie} collapsible />
        </dl>,
      );
      const cell = screen.getByText(cookie).closest("dd")!;
      expect(cell.className).toContain("max-h-value-clamp");
      expect(cell.className).toContain("overflow-hidden");

      fireEvent.click(screen.getByRole("button", { name: "copy set-cookie" }));
      await act(async () => {});
      expect(writeText).toHaveBeenCalledWith(cookie);

      const more = screen.getByRole("button", { name: "more of set-cookie" });
      expect(more.closest("dt")).not.toBeNull();
      fireEvent.click(more);
      expect(cell.className).not.toContain("max-h-value-clamp");
      fireEvent.click(screen.getByRole("button", { name: "less of set-cookie" }));
      expect(cell.className).toContain("max-h-value-clamp");
    });

    it("offers no toggle when the value fits — the clamp hides lines, and there are none to hide", () => {
      render(
        <dl>
          <KeyValueRow name="content-type" value="application/json" collapsible />
        </dl>,
      );
      expect(screen.getAllByRole("button")).toHaveLength(1);
    });

    it("collapses again when the value changes under it", () => {
      overflowing(true);
      const { rerender } = render(
        <dl>
          <KeyValueRow name="set-cookie" value={cookie} collapsible />
        </dl>,
      );
      fireEvent.click(screen.getByRole("button", { name: "more of set-cookie" }));
      expect(screen.getByRole("button", { name: "less of set-cookie" })).toBeInTheDocument();

      rerender(
        <dl>
          <KeyValueRow name="set-cookie" value={cookie + "; Secure"} collapsible />
        </dl>,
      );
      expect(screen.getByRole("button", { name: "more of set-cookie" })).toBeInTheDocument();
      expect(screen.getByText(cookie + "; Secure").closest("dd")!.className).toContain("max-h-value-clamp");
    });
  });
});
