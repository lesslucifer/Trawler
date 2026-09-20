import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { CommandBlock } from "./CommandBlock";

const TRUST =
  "sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain trawler-ca.pem";

const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(() => {
  vi.useFakeTimers();
  writeText.mockReset().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("CommandBlock", () => {
  it("renders the command verbatim, with nothing added to it", () => {
    const { container } = render(<CommandBlock value={TRUST} label="the trust command" />);

    const code = container.querySelector("code");
    expect(code).toHaveTextContent(TRUST, { normalizeWhitespace: false });
    expect(code?.textContent).toBe(TRUST);
  });

  it("the command is wire data in the mono family — the rule three hard rules read", () => {
    const { container } = render(<CommandBlock value={TRUST} label="the trust command" />);

    const code = container.querySelector("code")!;
    expect(code).toHaveAttribute("data-wire", "shell command");
    expect(code.className).toContain("font-mono");
  });

  it("wraps rather than truncating — a cut command is a broken command", () => {
    const { container } = render(<CommandBlock value={TRUST} label="the trust command" />);

    const code = container.querySelector("code")!;
    expect(code.className).toContain("break-all");
    expect(code.className).toContain("whitespace-pre-wrap");
    expect(code.className).not.toMatch(/truncate|overflow|text-ellipsis|whitespace-nowrap/);
  });

  it("sets itself apart from the prose above it with a surface of its own", () => {
    const { container } = render(<CommandBlock value={TRUST} label="the trust command" />);

    const block = container.firstElementChild!;
    expect(block.className).toContain("bg-canvas-soft");
    expect(block.className).toContain("border-hairline");
  });

  it("copies the command whole, through the one copy affordance in its corner", async () => {
    render(<CommandBlock value={TRUST} label="the trust command" />);

    const copy = screen.getByRole("button", { name: "the trust command" });
    fireEvent.click(copy);
    await act(async () => {});

    expect(writeText).toHaveBeenCalledWith(TRUST);
    expect(writeText).toHaveBeenCalledTimes(1);
  });

  it("the copy affordance is the icon form, which its adjacency licenses", () => {
    render(<CommandBlock value={TRUST} label="the trust command" />);

    const copy = screen.getByRole("button", { name: "the trust command" });
    expect(copy).toHaveAttribute("title", "the trust command");
    expect(copy.textContent).not.toContain("the trust command");
  });

  it("is not interactive itself — the block carries exactly one control", () => {
    render(<CommandBlock value={TRUST} label="the trust command" />);

    expect(screen.getAllByRole("button")).toHaveLength(1);
  });
});
