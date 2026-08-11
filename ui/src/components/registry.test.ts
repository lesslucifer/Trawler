import { describe, expect, it } from "vitest";
import { entry } from "../../../scripts/design-registry.mjs";

// The oracle's contract (E-DFLOW.W7 ①): the collection's TSDoc IS the registry,
// so what the parser accepts, waives and refuses is the vocabulary's boundary.

const LINES = [
  "Widget — one sentence about it.",
  "@tier T1 — no business logic",
  "@anatomy a box",
  "@use anywhere a box is wanted",
  "@avoid nowhere in particular",
  "@states rest",
  "@madeof —",
];

const block = (lines: string[]) => `/**\n${lines.map((l) => ` * ${l}`).join("\n")}\n */\n`;
const without = (tag: string) => LINES.filter((l) => !l.startsWith(`@${tag}`));
const replacing = (tag: string, line: string) => LINES.map((l) => (l.startsWith(`@${tag}`) ? line : l));

describe("membership is @tier — the waiver stated where it cannot drift", () => {
  it("reads the whole entry from the block", () => {
    expect(entry(block(LINES), "Widget")).toEqual({
      name: "Widget",
      summary: "Widget — one sentence about it.",
      tier: "T1 — no business logic",
      use: "anywhere a box is wanted",
      avoid: "nowhere in particular",
      states: "rest",
      madeof: "—",
    });
  });

  it("a file with no TSDoc block at all is simply not in the vocabulary", () => {
    expect(entry("export const CONFIRM_MS = 2000;\n", "useConfirm")).toBeNull();
  });

  it("a block that declares no @tier is not in the vocabulary either", () => {
    expect(entry(`${block(["a helper, not a component"])}export function h() {}`, "h")).toBeNull();
  });

  it("folds a tag wrapped over several lines into one value", () => {
    const wrapped = block(["Widget — one.", "@tier T1 — no", "  business logic", "@use a", "@avoid b", "@states c", "@madeof d"]);

    expect(entry(wrapped, "Widget")).toMatchObject({ tier: "T1 — no business logic" });
  });
});

describe("a malformed block fails loudly — a short roster is the failure to prevent", () => {
  it.each(["use", "avoid", "states", "madeof"])("@%s missing is refused, not defaulted", (tag) => {
    expect(() => entry(block(without(tag)), "Widget")).toThrow(new RegExp(`Widget: @${tag} is missing`));
  });

  it("an empty tag body is refused too — declared is not the same as stated", () => {
    expect(() => entry(block(replacing("states", "@states")), "Widget")).toThrow(/@states is empty/);
  });

  it("a @tier that names no tier is refused", () => {
    expect(() => entry(block(replacing("tier", "@tier a primitive")), "Widget")).toThrow(
      /@tier reads "a primitive"/,
    );
  });

  it("no summary line means the entry has no one-liner", () => {
    expect(() => entry(block(LINES.slice(1)), "Widget")).toThrow(/no summary line/);
  });

  it("two @tier blocks in one file is ambiguous, not first-wins", () => {
    expect(() => entry(block(LINES) + block(LINES), "Widget")).toThrow(/2 TSDoc blocks declare @tier/);
  });
});
