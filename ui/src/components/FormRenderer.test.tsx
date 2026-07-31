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
});
