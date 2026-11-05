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

  it("a long value stays on one line, cut at its end, and a click still copies the whole of it (DEC-78)", async () => {
    const cookie = "session=" + "x".repeat(4096);
    render(
      <dl>
        <KeyValueRow name="set-cookie" value={cookie} />
      </dl>,
    );

    const value = screen.getByRole("button", { name: "copy set-cookie" });
    expect(value).toHaveTextContent(cookie);
    expect(value.className).toContain("truncate");
    expect(value.className).not.toContain("break-all");
    expect(value.closest("dd")!.className).toContain("min-w-0");

    fireEvent.click(value);
    await act(async () => {});
    expect(writeText).toHaveBeenCalledWith(cookie);
  });

  it("the name is never cut, and no more / less toggle exists — the whole value is open all's", () => {
    render(
      <dl>
        <KeyValueRow name="content-security-policy-report-only" value={"x".repeat(2048)} />
      </dl>,
    );

    const name = screen.getByText("content-security-policy-report-only");
    expect(name.className).toContain("whitespace-nowrap");
    expect(name.className).not.toContain("truncate");
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.queryByText("more")).not.toBeInTheDocument();
  });
});
