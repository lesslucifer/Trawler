import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { CopyText } from "./CopyText";
import { CONFIRM_MS } from "./useConfirm";

const writeText = vi.fn<(text: string) => Promise<void>>();

const collapseSelection = (collapsed: boolean) => {
  vi.spyOn(window, "getSelection").mockReturnValue({
    isCollapsed: collapsed,
  } as unknown as Selection);
};

beforeEach(() => {
  vi.useFakeTimers();
  writeText.mockReset().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
  collapseSelection(true);
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const settle = async () => {
  await act(async () => {});
};

const URI = "https://api.example.com/v2/orders/8f3a91c2/items";

describe("CopyText", () => {
  it("click copies the whole value verbatim — display and payload are one string", async () => {
    render(<CopyText value={URI} label="request URI" />);

    fireEvent.click(screen.getByText(URI));
    await settle();

    expect(writeText).toHaveBeenCalledWith(URI);
    expect(screen.getByText("copied")).toBeInTheDocument();
  });

  it("the confirm clears after ~2s", async () => {
    render(<CopyText value={URI} label="request URI" />);

    fireEvent.click(screen.getByText(URI));
    await settle();
    act(() => vi.advanceTimersByTime(CONFIRM_MS));

    expect(screen.queryByText("copied")).not.toBeInTheDocument();
  });

  it("the selection guard leaves a partial take native — no copy fires", () => {
    collapseSelection(false);
    render(<CopyText value={URI} label="request URI" />);

    fireEvent.click(screen.getByText(URI));

    expect(writeText).not.toHaveBeenCalled();
    expect(screen.queryByText("copied")).not.toBeInTheDocument();
  });

  it("Enter and Space copy when focused — keyboard parity", async () => {
    render(<CopyText value={URI} label="request URI" />);
    const target = screen.getByRole("button", { name: "copy request URI" });

    fireEvent.keyDown(target, { key: "Enter" });
    await settle();
    expect(writeText).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(target, { key: " " });
    await settle();
    expect(writeText).toHaveBeenCalledTimes(2);
  });

  it("a clipboard failure is a silent no-op — the chip only ever confirms a real write", async () => {
    writeText.mockRejectedValue(new Error("denied"));
    render(<CopyText value={URI} label="request URI" />);

    fireEvent.click(screen.getByText(URI));
    await settle();

    expect(screen.queryByText("copied")).not.toBeInTheDocument();
  });

  it("a value change resets the confirm instantly", async () => {
    const { rerender } = render(<CopyText value={URI} label="request URI" />);

    fireEvent.click(screen.getByText(URI));
    await settle();
    expect(screen.getByText("copied")).toBeInTheDocument();

    rerender(<CopyText value="https://other" label="request URI" />);
    expect(screen.queryByText("copied")).not.toBeInTheDocument();
  });

  it("keeps the I-beam and never truncates — it is selectable text, not a link", () => {
    render(<CopyText value={URI} label="request URI" />);

    const span = screen.getByText(URI);
    expect(span.className).toContain("cursor-text");
    expect(span.className).toContain("break-all");
    expect(span.className).toContain("decoration-dotted");
    expect(span.className).not.toContain("truncate");
  });
});
