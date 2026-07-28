import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { CopyButton } from "./CopyButton";
import { CONFIRM_MS } from "./useConfirm";

const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(() => {
  vi.useFakeTimers();
  writeText.mockReset().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
});

afterEach(() => {
  vi.useRealTimers();
});

const settle = async () => {
  await act(async () => {});
};

describe("CopyButton", () => {
  it("labelled form: copies the payload and confirms, then reverts", async () => {
    render(<CopyButton label="copy curl" value="curl https://x" />);

    fireEvent.click(screen.getByRole("button", { name: /copy curl/ }));
    await settle();

    expect(writeText).toHaveBeenCalledWith("curl https://x");
    expect(screen.getByRole("button")).toHaveTextContent("copied");

    act(() => vi.advanceTimersByTime(CONFIRM_MS));
    expect(screen.getByRole("button")).toHaveTextContent("copy curl");
  });

  it("icon form (D2): the glyph is the whole face, label becomes the accessible name", async () => {
    render(<CopyButton label="copy body" value="{}" iconOnly />);

    const button = screen.getByRole("button", { name: "copy body" });
    expect(button).toHaveAttribute("title", "copy body");
    expect(button).toHaveTextContent("⧉");

    fireEvent.click(button);
    await settle();
    expect(button).toHaveTextContent("✓");
  });

  it("the icon form's hit area meets the agreed 24px size — not BodyViewer's 22px borrow", () => {
    render(<CopyButton label="copy body" value="{}" iconOnly />);

    expect(screen.getByRole("button")).toHaveStyle({ width: "24px", height: "24px" });
  });

  it("a failed write says so in warn tone, on the same timer", async () => {
    writeText.mockRejectedValue(new Error("denied"));
    render(<CopyButton label="copy curl" value="x" />);

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(screen.getByRole("button")).toHaveTextContent("failed");
    expect(screen.getByRole("button").className).toContain("text-warn");

    act(() => vi.advanceTimersByTime(CONFIRM_MS));
    expect(screen.getByRole("button")).toHaveTextContent("copy curl");
  });

  it("the confirm timer restarts on re-click", async () => {
    render(<CopyButton label="copy curl" value="x" />);
    const button = screen.getByRole("button");

    fireEvent.click(button);
    await settle();
    act(() => vi.advanceTimersByTime(CONFIRM_MS - 200));

    fireEvent.click(button);
    await settle();
    act(() => vi.advanceTimersByTime(CONFIRM_MS - 200));
    expect(button).toHaveTextContent("copied");

    act(() => vi.advanceTimersByTime(200));
    expect(button).toHaveTextContent("copy curl");
  });

  it("a payload change resets the confirm instantly — no stale 'copied' beside a new value", async () => {
    const { rerender } = render(<CopyButton label="copy curl" value="first" />);

    fireEvent.click(screen.getByRole("button"));
    await settle();
    expect(screen.getByRole("button")).toHaveTextContent("copied");

    rerender(<CopyButton label="copy curl" value="second" />);
    expect(screen.getByRole("button")).toHaveTextContent("copy curl");
  });

  it("getPayload defers the serialisation until the press", async () => {
    const getPayload = vi.fn(() => "lazy");
    render(<CopyButton label="copy all" getPayload={getPayload} />);

    expect(getPayload).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button"));
    await settle();

    expect(writeText).toHaveBeenCalledWith("lazy");
  });

  it("disabled copies nothing", () => {
    render(<CopyButton label="copy curl" value="x" disabled />);

    fireEvent.click(screen.getByRole("button"));
    expect(writeText).not.toHaveBeenCalled();
  });

  it("composes Button: the shell's chrome is inherited, not restated", () => {
    render(<CopyButton label="copy curl" value="x" />);

    const button = screen.getByRole("button");
    expect(button.className).toContain("text-ink-mute");
    expect(button.className).toContain("border-hairline");
    expect(button.className).toContain("focus-visible:outline-ink-mute");
  });
});
