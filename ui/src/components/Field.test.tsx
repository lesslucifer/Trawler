import { describe, expect, it, vi } from "vitest";
import type { ChangeEvent } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { Field } from "./Field";
import { Select } from "./Select";

describe("Field", () => {
  it("reports every keystroke and delays nothing (B1)", () => {
    const seen: string[] = [];
    const onChange = vi.fn((event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => seen.push(event.target.value));
    const { rerender } = render(<Field value="" onChange={onChange} aria-label="host" />);

    const input = screen.getByRole("textbox", { name: "host" });
    for (const value of ["a", "ap", "api"]) {
      fireEvent.change(input, { target: { value } });
      rerender(<Field value={value} onChange={onChange} aria-label="host" />);
    }

    expect(onChange).toHaveBeenCalledTimes(3);
    expect(seen).toEqual(["a", "ap", "api"]);
  });

  it("holds no value of its own — what the caller passes is what renders", () => {
    const { rerender } = render(<Field value="api." onChange={() => {}} aria-label="host" />);
    const input = screen.getByRole("textbox", { name: "host" });
    expect(input).toHaveValue("api.");

    fireEvent.change(input, { target: { value: "ignored" } });
    expect(input).toHaveValue("api.");

    rerender(<Field value="api.example" onChange={() => {}} aria-label="host" />);
    expect(input).toHaveValue("api.example");
  });

  it("never judges the value — no invalid state exists for any input (B2)", () => {
    render(<Field value="4o4" onChange={() => {}} aria-label="status" inputMode="numeric" />);

    const input = screen.getByRole("textbox", { name: "status" });
    expect(input).not.toHaveAttribute("aria-invalid");
    expect(input.className).not.toContain("warn");
    expect(input).toHaveAttribute("inputmode", "numeric");
  });

  it("cannot be built nameless — the placeholder is a hint, never the name (B3)", () => {
    render(<Field value="" onChange={() => {}} aria-label="filter" placeholder="filter — free text" />);

    const input = screen.getByRole("textbox", { name: "filter" });
    expect(input).toHaveAttribute("placeholder", "filter — free text");
    expect(screen.queryByRole("textbox", { name: "filter — free text" })).toBeNull();
  });

  it("sets no width of its own — the caller's class is the only width (B4, driver ②)", () => {
    render(<Field value="" onChange={() => {}} aria-label="host" className="w-field-lg" />);

    const input = screen.getByRole("textbox", { name: "host" });
    expect(input.className).toContain("w-field-lg");
    expect(input.className).not.toMatch(/(^|\s)w-(?!field-lg)/);
    expect(input.className).not.toContain("flex-1");
  });

  it("is Select's box, class for class — not the legacy px-3/py-chrome-y (F2, DEC-18)", () => {
    render(
      <>
        <Field value="" onChange={() => {}} aria-label="host" />
        <Select options={[{ id: "macos", label: "macOS" }]} value="macos" onChange={() => {}} label="target" />
      </>,
    );

    const field = screen.getByRole("textbox", { name: "host" });
    const trigger = screen.getByRole("button", { name: "target" });

    for (const shared of ["min-h-control", "rounded-button", "border-hairline", "px-control-x", "py-control-y", "text-control"]) {
      expect(field.className).toContain(shared);
      expect(trigger.className).toContain(shared);
    }
    expect(field.className).not.toContain("px-3");
    expect(field.className).not.toContain("py-chrome-y");
  });

  it("hover and focus land on the same one channel — a border lift, never a ring (driver ④)", () => {
    render(<Field value="" onChange={() => {}} aria-label="host" />);

    const input = screen.getByRole("textbox", { name: "host" });
    expect(input.className).toContain("hover:border-ink-mute");
    expect(input.className).toContain("focus:border-ink-mute");
    expect(input.className).toContain("outline-none");
    expect(input.className).not.toContain("ring");
  });

  it("disabled dims and leaves the tab order, the one state with no tenant", () => {
    render(<Field value="" onChange={() => {}} aria-label="host" disabled />);

    const input = screen.getByRole("textbox", { name: "host" });
    expect(input).toBeDisabled();
    expect(input.className).toContain("disabled:opacity-disabled");
  });

  it("multi-line is a textarea in the same box, class for class (DEC-59)", () => {
    render(
      <>
        <Field value="" onChange={() => {}} aria-label="host" />
        <Field value="" onChange={() => {}} aria-label="body" multiline />
      </>,
    );

    const line = screen.getByRole("textbox", { name: "host" });
    const lines = screen.getByRole("textbox", { name: "body" });
    expect(lines.tagName).toBe("TEXTAREA");
    expect(line.tagName).toBe("INPUT");
    for (const shared of line.className.split(/\s+/).filter(Boolean)) {
      expect(lines.className).toContain(shared);
    }
    expect(lines.className).toContain("resize-none");
  });

  it("multi-line takes its height from the caller, as it takes its width", () => {
    render(<Field value="" onChange={() => {}} aria-label="body" multiline className="h-body-max w-full" />);

    const box = screen.getByRole("textbox", { name: "body" });
    expect(box.className).toContain("h-body-max");
    expect(box.className).not.toMatch(/(^|\s)h-(?!body-max)/);
  });

  it("multi-line reports every keystroke and keeps its line breaks", () => {
    const seen: string[] = [];
    render(<Field value="" onChange={(e) => seen.push(e.target.value)} aria-label="body" multiline />);

    fireEvent.change(screen.getByRole("textbox", { name: "body" }), { target: { value: '{\n  "a": 1\n}' } });

    expect(seen).toEqual(['{\n  "a": 1\n}']);
  });

  it("verbatim swaps the family to mono and changes nothing else", () => {
    render(
      <>
        <Field value="" onChange={() => {}} aria-label="chrome" />
        <Field value="" onChange={() => {}} aria-label="wire" verbatim />
      </>,
    );

    const chrome = screen.getByRole("textbox", { name: "chrome" }).className.split(/\s+/);
    const wire = screen.getByRole("textbox", { name: "wire" }).className.split(/\s+/);
    expect(chrome).toContain("font-ui");
    expect(wire).toContain("font-mono");
    expect(wire).not.toContain("font-ui");
    expect(wire.filter((c) => c !== "font-mono")).toEqual(chrome.filter((c) => c !== "font-ui"));
  });

  it("read-only keeps the box focusable and dims it like disabled", () => {
    render(<Field value="curl x" onChange={() => {}} aria-label="command" multiline readOnly />);

    const box = screen.getByRole("textbox", { name: "command" });
    expect(box).toHaveAttribute("readonly");
    expect(box).not.toBeDisabled();
    box.focus();
    expect(box).toHaveFocus();
    expect(box.className).toContain("read-only:opacity-disabled");
  });

  it("neither form judges its value — no invalid, no edited, no tone (DEC-59)", () => {
    render(
      <>
        <Field value="htp:/x" onChange={() => {}} aria-label="url" verbatim />
        <Field value="{" onChange={() => {}} aria-label="body" multiline verbatim />
      </>,
    );

    for (const name of ["url", "body"]) {
      const box = screen.getByRole("textbox", { name });
      expect(box).not.toHaveAttribute("aria-invalid");
      expect(box.className).not.toMatch(/warn|ok|accent|edited/);
    }
  });
});
