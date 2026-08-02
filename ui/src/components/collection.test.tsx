import { beforeAll, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { json } from "@codemirror/lang-json";
import type { ReactElement } from "react";
import * as collection from "./index";

// The collection's own contract (E-DFLOW.W5), the part no per-component test can
// reach: the roster is a vocabulary, not a folder — every T1/T2 name is exported
// and mounts, every @madeof edge really composes instead of re-drawing, and every
// @avoid alternative resolves to something a caller can actually reach.

const ROSTER: Record<string, ReactElement> = {
  Button: <collection.Button onClick={() => {}}>send</collection.Button>,
  CopyButton: <collection.CopyButton value="v" label="copy" />,
  CopyText: <collection.CopyText value="v" label="uri" />,
  CutMarker: <collection.CutMarker shown={10} total={99} />,
  DurationText: <collection.DurationText value={12} />,
  FormRenderer: <collection.FormRenderer text="a=1" />,
  HexRenderer: <collection.HexRenderer bytes={new Uint8Array([1])} />,
  IconButton: <collection.IconButton glyph="close" onClick={() => {}} />,
  ImageRenderer: <collection.ImageRenderer bytes={new Uint8Array([1])} contentType="image/png" />,
  JsonRenderer: <collection.JsonRenderer text='{"a":1}' />,
  KeyValueRow: <collection.KeyValueRow name="host" value="example.com" />,
  MethodLabel: <collection.MethodLabel method="GET" />,
  Notice: <collection.Notice>nothing captured yet</collection.Notice>,
  Section: <collection.Section title="request">body</collection.Section>,
  SegmentedControl: (
    <collection.SegmentedControl
      options={[{ value: "raw", label: "raw" }]}
      value="raw"
      onChange={() => {}}
      ariaLabel="view"
    />
  ),
  Select: (
    <collection.Select
      options={[{ id: "macos", label: "macOS" }]}
      value="macos"
      onChange={() => {}}
      label="target"
    />
  ),
  SourceSurface: <collection.SourceSurface text='{"a":1}' language={json()} testId="surface" />,
  StatusLine: (
    <collection.StatusLine status={{ glyph: "✓", tone: "ok", message: "trusted" }} name="CA" />
  ),
  StatusPill: <collection.StatusPill status={200} />,
  StepList: <collection.StepList steps={["download the certificate"]} />,
  TextRenderer: <collection.TextRenderer text="plain" />,
  TimestampText: <collection.TimestampText timestamp="2026-07-27T10:00:00Z" />,
  TonedText: <collection.TonedText tone="ok">ok</collection.TonedText>,
  XmlRenderer: <collection.XmlRenderer text="<a/>" />,
};

// jsdom ships no object-URL support, which ImageRenderer's preview needs.
beforeAll(() => {
  URL.createObjectURL = vi.fn(() => "blob:collection");
  URL.revokeObjectURL = vi.fn();
});

describe("the collection — every T1/T2 component is exported and mounts standalone", () => {
  it("the barrel exports exactly the roster, no more and no less", () => {
    const exported = Object.entries(collection)
      .filter(([, v]) => typeof v === "function" && /^[A-Z]/.test(v.name))
      .map(([name]) => name);

    expect(exported.sort()).toEqual(Object.keys(ROSTER).sort());
  });

  it.each(Object.keys(ROSTER))("%s renders with no screen around it", (name) => {
    const { container } = render(ROSTER[name]);
    expect(container).not.toBeEmptyDOMElement();
  });
});

describe("@madeof composes — the parts are reused, never re-drawn", () => {
  // TonedText is the only span in the app that is font-mono + tabular-nums with a
  // tone class and nothing else; a re-drawn copy would miss one of the three.
  const tonedTexts = (root: HTMLElement) =>
    [...root.querySelectorAll("span.font-mono.tabular-nums")].filter((el) =>
      /text-(ok|warn|ink-mute)/.test(el.className),
    );

  it("StatusPill renders a TonedText (fork D4's split, not a private mono span)", () => {
    const { container } = render(<collection.StatusPill status={404} />);

    const toned = tonedTexts(container);
    expect(toned).toHaveLength(1);
    expect(toned[0]).toHaveTextContent("404");
    expect(toned[0].className).toContain("text-warn");
  });

  it("StatusLine renders a TonedText for its glyph", () => {
    const { container } = render(
      <collection.StatusLine status={{ glyph: "⚠", tone: "warn", message: "install failed" }} />,
    );

    const toned = tonedTexts(container);
    expect(toned).toHaveLength(1);
    expect(toned[0]).toHaveTextContent("⚠");
  });

  it("KeyValueRow's value cell is a CopyText, so the row inherits click-to-copy", () => {
    render(<collection.KeyValueRow name="cookie" value="sid=abc" />);

    expect(screen.getByRole("button", { name: /copy cookie/i })).toHaveTextContent("sid=abc");
  });

  it("FormRenderer composes KeyValueRow rather than drawing its own pair", () => {
    render(<collection.FormRenderer text="user=ada&role=admin" />);

    expect(screen.getByRole("button", { name: /copy user/i })).toHaveTextContent("ada");
    expect(screen.getByRole("button", { name: /copy role/i })).toHaveTextContent("admin");
  });

  it("CopyButton and IconButton are one <button> with Button's shell, not three elements", () => {
    const shell = (el: HTMLElement) => el.className.replace(/\s+/g, " ");

    render(
      <>
        <collection.Button onClick={() => {}}>send</collection.Button>
        <collection.CopyButton value="v" label="copy curl" />
        <collection.IconButton glyph="close" onClick={() => {}} />
      </>,
    );

    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(3);
    for (const button of buttons) {
      expect(button.tagName).toBe("BUTTON");
      expect(button.querySelector("button")).toBeNull();
      expect(shell(button)).toContain("focus-visible:outline");
      expect(shell(button)).toContain("disabled:opacity-disabled");
    }
  });

  it("JsonRenderer falls back through TextRenderer, the member it declares", () => {
    render(<collection.JsonRenderer text="{not json" />);

    expect(screen.getByTestId("body-text")).toHaveTextContent("{not json");
    expect(screen.queryByTestId("body-json")).toBeNull();
  });

  it("Notice's action slot takes a Button — the composition its @madeof names", () => {
    render(
      <collection.Notice tone="warn" action={<collection.Button onClick={() => {}}>retry</collection.Button>}>
        the body is no longer captured
      </collection.Notice>,
    );

    expect(screen.getByRole("button", { name: "retry" })).toBeInTheDocument();
  });
});

describe("@avoid alternatives are reachable — every redirect resolves to an export", () => {
  // Each pair is "<component>'s @avoid sends you to <name>"; the gap that went
  // unnoticed for four zones was a redirect with no destination in the barrel.
  const REDIRECTS: [string, string][] = [
    ["IconButton", "Button"],
    ["IconButton", "CopyButton"],
    ["Button", "CopyButton"],
    ["Button", "IconButton"],
    ["CopyButton", "CopyText"],
    ["CopyText", "CopyButton"],
    ["KeyValueRow", "CopyText"],
    ["Notice", "TonedText"],
    ["Notice", "StatusPill"],
    ["Notice", "StatusLine"],
    ["TonedText", "StatusPill"],
    ["TonedText", "Notice"],
    ["TonedText", "CopyText"],
    ["TonedText", "Button"],
    ["StatusPill", "TonedText"],
    ["StatusLine", "StatusPill"],
    ["StatusLine", "Notice"],
    ["StatusLine", "TonedText"],
    ["StepList", "KeyValueRow"],
    ["StepList", "Notice"],
    ["StepList", "CopyText"],
    ["StepList", "StatusLine"],
    ["Select", "SegmentedControl"],
    ["Select", "Notice"],
    ["Section", "Section"],
    ["SourceSurface", "TextRenderer"],
    ["FormRenderer", "KeyValueRow"],
    ["JsonRenderer", "TextRenderer"],
    ["XmlRenderer", "TextRenderer"],
  ];

  it.each(REDIRECTS)("%s's @avoid points at %s, which the barrel exports", (from, to) => {
    expect(collection[from as keyof typeof collection]).toBeTypeOf("function");
    expect(collection[to as keyof typeof collection]).toBeTypeOf("function");
  });

  it("every name a redirect cites is one the roster mounts", () => {
    for (const [from, to] of REDIRECTS) {
      expect(ROSTER).toHaveProperty(from);
      expect(ROSTER).toHaveProperty(to);
    }
  });
});
