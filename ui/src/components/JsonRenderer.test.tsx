import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TREE_MAX } from "../body";
import { JsonRenderer } from "./JsonRenderer";

const PRETTY = '{\n  "id": "ord_8f3a91c2",\n  "total": 49.99,\n  "tags": [\n    "rush"\n  ]\n}';

describe("JsonRenderer", () => {
  it("renders the source in the editor surface, not a tree", () => {
    render(<JsonRenderer text={PRETTY} />);

    expect(screen.getByTestId("body-json")).toBeInTheDocument();
    expect(screen.queryByTestId("body-text")).not.toBeInTheDocument();
  });

  // The document, not the container: fold-gutter markers are chrome and would
  // otherwise be read as part of the payload.
  it("keeps the document character-exact, so a copy yields valid JSON", () => {
    const { container } = render(<JsonRenderer text={PRETTY} />);

    const shown = container.querySelector(".cm-content")?.textContent ?? "";
    expect(shown).toContain('"id": "ord_8f3a91c2"');
    expect(shown).toContain("{");
    expect(shown).toContain("}");
    expect(shown).toContain(",");
    expect(() => JSON.parse(shown)).not.toThrow();
  });

  it("offers the fold gutter that DEC-01 gave up and DEC-13 restored", () => {
    const { container } = render(<JsonRenderer text={PRETTY} />);
    expect(container.querySelector(".cm-foldGutter")).toBeInTheDocument();
  });

  it("falls back to text when the body will not parse — never a parse error screen", () => {
    render(<JsonRenderer text='{"broken":true,' />);

    expect(screen.getByTestId("body-text")).toHaveTextContent('{"broken":true,');
    expect(screen.queryByTestId("body-json")).not.toBeInTheDocument();
  });

  it("falls back to text over the cap — a 10 MiB document is not worth a surface", () => {
    const huge = `{"pad":"${"a".repeat(TREE_MAX)}"}`;
    render(<JsonRenderer text={huge} />);

    expect(screen.getByTestId("body-text")).toBeInTheDocument();
    expect(screen.queryByTestId("body-json")).not.toBeInTheDocument();
  });

  it("the fallback carries the family cut marker, not a bare truncation", () => {
    render(<JsonRenderer text={`{"pad":"${"a".repeat(TREE_MAX)}"}`} />);
    expect(screen.getByText(/⚠ showing .* of .* — save to see all/)).toBeInTheDocument();
  });
});
