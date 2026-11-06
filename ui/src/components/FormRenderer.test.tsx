import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { TEXT_MAX } from "../body";
import { FormRenderer } from "./FormRenderer";

const BODY = "grant_type=authorization_code&client_id=app_8f3a91c2&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback";

describe("FormRenderer", () => {
  it("decodes the pairs — the user reads values, not percent escapes", () => {
    render(<FormRenderer text={BODY} />);

    expect(screen.getByText("grant_type")).toBeInTheDocument();
    expect(screen.getByText("authorization_code")).toBeInTheDocument();
    expect(screen.getByText("https://example.com/callback")).toBeInTheDocument();
  });

  it("decodes + as a space, as URL encoding means it", () => {
    render(<FormRenderer text="q=two+words" />);
    expect(screen.getByText("two words")).toBeInTheDocument();
  });

  it("a URI reads as its parts, then its query's pairs, not as one key up to the first =", () => {
    render(<FormRenderer text="https://cdn.example/v/a.mp4?_nc_cat=106&efg=eyJ2In0%3D" label="text/uri-list" />);
    expect(screen.getByText("scheme")).toBeInTheDocument();
    expect(screen.getByText("cdn.example")).toBeInTheDocument();
    expect(screen.getByText("/v/a.mp4")).toBeInTheDocument();
    expect(screen.getByText("_nc_cat")).toBeInTheDocument();
    expect(screen.getByText("106")).toBeInTheDocument();
    expect(screen.getByText("eyJ2In0=")).toBeInTheDocument();
  });

  it("name: value lines read one row per line, values verbatim", () => {
    render(<FormRenderer text={"cache-control: max-age=1209600, no-transform\ntiming-allow-origin: *"} label="text/rfc822-headers" />);
    expect(screen.getByText("cache-control")).toBeInTheDocument();
    expect(screen.getByText("max-age=1209600, no-transform")).toBeInTheDocument();
    expect(screen.getByText("*")).toBeInTheDocument();
  });

  it("a URL-encoded body is read as pairs whatever it looks like, with the window's decoding", () => {
    render(<FormRenderer text={'/a="x"&note: y=<b>&bad=%E4&k=v'} label="application/x-www-form-urlencoded" />);
    expect(screen.getByText("/a")).toBeInTheDocument();
    expect(screen.getByText('"x"')).toBeInTheDocument();
    expect(screen.getByText("note: y")).toBeInTheDocument();
    expect(screen.getByText("%E4")).toBeInTheDocument();
  });

  it("a first header line over the cap still shows its name, with no partial value", () => {
    render(<FormRenderer text={`x-huge: ${"h".repeat(TEXT_MAX + 10)}\nnext: 1`} label="text/rfc822-headers" />);
    expect(screen.getByText("x-huge")).toBeInTheDocument();
    expect(screen.queryByText(/^h+$/)).toBeNull();
  });

  it("long name: value lines are cut on a line, never mid-value", () => {
    const line = `x-pad: ${"p".repeat(1000)}`;
    const text = Array.from({ length: Math.ceil(TEXT_MAX / line.length) + 2 }, () => line).join("\n");
    render(<FormRenderer text={text} label="text/rfc822-headers" />);
    const values = screen.getAllByText(/^p+$/);
    expect(values.length).toBeLessThan(text.split("\n").length);
    expect(values.every((value) => value.textContent === "p".repeat(1000))).toBe(true);
  });

  it("composes KeyValueRow — the value cell's copy affordance is not re-drawn here", () => {
    render(<FormRenderer text="code=SplxlOBeZQQ" />);

    const value = screen.getByLabelText("copy code");
    expect(value.className).toContain("decoration-dotted");
  });

  it("clicking a value copies that value whole", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });

    render(<FormRenderer text="code=SplxlOBeZQQ" />);
    fireEvent.click(screen.getByLabelText("copy code"));

    expect(writeText).toHaveBeenCalledWith("SplxlOBeZQQ");
    vi.unstubAllGlobals();
  });

  it("supplies the two-column grid KeyValueRow's display:contents needs", () => {
    render(<FormRenderer text={BODY} />);
    expect(screen.getByTestId("body-form").className).toContain("grid-cols-[auto_1fr]");
  });

  it("renders a repeated key once per occurrence — the wire may repeat it", () => {
    render(<FormRenderer text="tag=a&tag=b" />);

    expect(screen.getAllByText("tag")).toHaveLength(2);
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByText("b")).toBeInTheDocument();
  });

  it("caps an over-large body and states the cut", () => {
    render(<FormRenderer text={`k=${"v".repeat(TEXT_MAX)}`} />);
    expect(screen.getByText(/⚠ showing .* of .* — save to see all/)).toBeInTheDocument();
  });

  it("shows no marker when every pair fits", () => {
    render(<FormRenderer text={BODY} />);
    expect(screen.queryByText(/showing/)).not.toBeInTheDocument();
  });

  it("never decodes a half-escape into the value it copies — the cut is a render cap, not a payload cap", () => {
    const head = "pad=";
    const tail = "&price=%E2%82%AC99";
    const straddling = `${head}${"v".repeat(TEXT_MAX - head.length - "&price=%E2".length)}${tail}`;
    render(<FormRenderer text={straddling} />);

    for (const cell of screen.getByTestId("body-form").querySelectorAll("dd")) {
      expect(cell.textContent).not.toContain("�");
      expect(cell.textContent).not.toMatch(/%[0-9A-Fa-f]?$/);
    }
  });

  // A key longer than the cap leaves no boundary to keep, so the pane shows
  // nothing and says so. "showing 0 B" is a true statement with a way out; a
  // marker whose two sizes matched would be the false one.
  it("states an honest 0 B rather than rendering a pair it cannot cut safely", () => {
    render(<FormRenderer text={`${"k".repeat(TEXT_MAX + 10)}=v`} />);

    const [shown, total] = screen.getByText(/⚠ showing/).textContent!.match(/showing (.+) of (.+) —/)!.slice(1);
    expect(shown).not.toBe(total);
    expect(screen.getByTestId("body-form").querySelectorAll("dd")).toHaveLength(0);
  });
});
