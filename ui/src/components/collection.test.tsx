import { beforeAll, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { json } from "@codemirror/lang-json";
import type { ReactElement } from "react";
import * as collection from "./index";
import { registry } from "../../../scripts/design-registry.mjs";
import { reachable, refusals, unresolved } from "../../../scripts/design-citations.mjs";

// The collection's own contract (E-DFLOW.W5), the part no per-component test can
// reach: the roster is a vocabulary, not a folder — every T1/T2 name is exported
// and mounts, every @madeof edge really composes instead of re-drawing, and every
// @avoid alternative resolves to something a caller can actually reach.
//
// The vocabulary is read from the components' own TSDoc (E-DFLOW.W7 ①), so the
// names and the redirects below are derived. What stays hand-written is the one
// thing no TSDoc can carry: the props each component needs to mount.

const REGISTRY = registry();
const NAMES = Object.keys(REGISTRY).sort();

const SPECIMENS: Record<string, ReactElement> = {
  Button: <collection.Button onClick={() => {}}>send</collection.Button>,
  Checkbox: (
    <collection.Checkbox checked={false} onChange={() => {}}>
      don't show this again
    </collection.Checkbox>
  ),
  CommandBlock: <collection.CommandBlock value="certutil -addstore -f ROOT trawler-ca.pem" label="the trust command" />,
  CopyButton: <collection.CopyButton value="v" label="copy" />,
  CopyText: <collection.CopyText value="v" label="uri" />,
  CutMarker: <collection.CutMarker shown={10} total={99} />,
  Disclosure: <collection.Disclosure summary="set it up manually">steps</collection.Disclosure>,
  DurationText: <collection.DurationText value={12} />,
  Field: <collection.Field value="" onChange={() => {}} aria-label="host" />,
  FormRenderer: <collection.FormRenderer text="a=1" />,
  HexRenderer: <collection.HexRenderer bytes={new Uint8Array([1])} />,
  IconButton: <collection.IconButton glyph="close" onClick={() => {}} />,
  ImageRenderer: <collection.ImageRenderer bytes={new Uint8Array([1])} contentType="image/png" />,
  JsonRenderer: <collection.JsonRenderer text='{"a":1}' />,
  KeyValueRow: <collection.KeyValueRow name="host" value="example.com" />,
  MethodLabel: <collection.MethodLabel method="GET" />,
  QrCode: <collection.QrCode value="http://192.168.1.24:7421/ca.cer" label="the certificate URL" />,
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
  Spinner: <collection.Spinner />,
  StatusDot: <collection.StatusDot status="connected" opens="setup" onClick={() => {}} />,
  StatusLine: (
    <collection.StatusLine
      status={{ glyph: "✓", tone: "ok", short: "trusted" }}
      name="CA"
    />
  ),
  StatusPill: <collection.StatusPill status={200} />,
  StepList: <collection.StepList steps={["download the certificate"]} />,
  TaskStep: <collection.TaskStep n={1} title="Trust Trawler's certificate" />,
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
  it("the barrel exports exactly what the TSDoc declares, no more and no less", () => {
    const exported = Object.entries(collection)
      .filter(([, v]) => typeof v === "function" && /^[A-Z]/.test(v.name))
      .map(([name]) => name);

    expect(exported.sort()).toEqual(NAMES);
  });

  it("every name in the vocabulary has a specimen here", () => {
    expect(Object.keys(SPECIMENS).sort()).toEqual(NAMES);
  });

  it.each(Object.keys(SPECIMENS))("%s renders with no screen around it", (name) => {
    const { container } = render(SPECIMENS[name]);
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
      <collection.StatusLine
        status={{ glyph: "⚠", tone: "warn", short: "install failed" }}
      />,
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

  it("CopyButton, IconButton and StatusDot are one <button> with Button's shell, not four elements", () => {
    const shell = (el: HTMLElement) => el.className.replace(/\s+/g, " ");

    render(
      <>
        <collection.Button onClick={() => {}}>send</collection.Button>
        <collection.CopyButton value="v" label="copy curl" />
        <collection.IconButton glyph="close" onClick={() => {}} />
        <collection.StatusDot status="broken" opens="setup" onClick={() => {}} />
      </>,
    );

    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(4);
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
  // Each pair is "<component>'s @avoid sends you to <name>", read out of the
  // TSDoc; the gap that went unnoticed for four zones was a redirect with no
  // destination in the barrel, and the barrel is the independent half here.
  const REDIRECTS: [string, string][] = NAMES.flatMap((from) =>
    REGISTRY[from].redirects.map((to: string): [string, string] => [from, to]),
  );

  it.each(REDIRECTS)("%s's @avoid points at %s, which the barrel exports", (from, to) => {
    expect(collection[from as keyof typeof collection]).toBeTypeOf("function");
    expect(collection[to as keyof typeof collection]).toBeTypeOf("function");
  });

  it("a name is cited whole — the CopyButton in Notice's @avoid is no Button redirect", () => {
    expect(REGISTRY.Notice.avoid).toContain("CopyButton");
    expect(REGISTRY.Notice.redirects).toContain("CopyButton");
    expect(REGISTRY.Notice.redirects).not.toContain("Button");
  });
});

describe("every name a docstring cites is reachable, not only its @avoid redirects", () => {
  // The redirects above are the subset the barrel can answer for. DEC-16 makes
  // the whole block the retired design file, so an @anatomy pinned to a deleted
  // file and a @use naming a deleted component are the same defect as a redirect
  // with no destination (DEC-30) — and the tree is the only judge of all three.

  it("no component's TSDoc names a file, module or type the tree cannot reach", () => {
    expect(unresolved()).toEqual({});
  });

  it("the tree is what resolves them — a name nothing owns is refused", () => {
    expect(refusals("built from a GhostTable", reachable())).toEqual(["GhostTable"]);
  });
});

describe("the barrel is what the app reaches — the registry, not the legacy kit", () => {
  // "./components" is ambiguous whenever a components.tsx sits beside the
  // components/ directory: bundlers resolve the file over the directory's index,
  // so a screen importing the bare form silently gets the other one. The kit that
  // made it ambiguous is deleted, and this keeps the spelling from coming back.
  const screens = import.meta.glob("../*.tsx", { query: "?raw", import: "default", eager: true });

  it("no screen imports through the ambiguous bare specifier", () => {
    const offenders = Object.entries(screens)
      .filter(([path]) => !path.includes(".test."))
      .filter(([, source]) => /from "\.\/components"/.test(source as string))
      .map(([path]) => path);

    expect(offenders).toEqual([]);
  });
});
