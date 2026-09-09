import { describe, expect, it } from "vitest";
import { tierBlocks } from "../../../scripts/design-registry.mjs";
import { refusals } from "../../../scripts/design-citations.mjs";

// The citation oracle's contract (E-RFINISH.W2): a docstring is prose, so what
// the check calls a citation — and what it lets pass as an English word — is the
// whole of its judgement. The collection's own test asserts the tree is clean;
// these fixtures assert the boundary the tree cannot demonstrate, because a
// clean tree shows no refusal at all.

const TREE = {
  names: new Set(["KeyValueRow", "ReactNode"]),
  packages: new Set(["codemirror"]),
  exists: (path: string) => path === "ui/src/flow.ts" || path === "decode.rs",
};

const refused = (text: string) => refusals(text, TREE);

describe("what counts as a citation — the hump is the whole test", () => {
  it("a two-hump name is a citation and is looked up", () => {
    expect(refused("built from KeyValueRow")).toEqual([]);
    expect(refused("built from GhostTable")).toEqual(["GhostTable"]);
  });

  it.each(["Inter", "Pass", "Enter", "Space", "Blob", "Date", "Warp"])(
    "%s is prose, not a name the tree owes a destination",
    (word) => {
      expect(refused(`the label is ${word} at 12px`)).toEqual([]);
    },
  );

  it.each(["JSON", "WCAG", "ASCII", "STANDING", "UNLESS", "NEVER"])(
    "%s is shouted or an initialism, and no lookup is attempted",
    (word) => {
      expect(refused(`${word} — and nothing else`)).toEqual([]);
    },
  );

  it("a name inside a path is judged on both halves, so a dead file names itself twice", () => {
    expect(refused("see GhostTable.tsx")).toEqual(["GhostTable.tsx", "GhostTable"]);
  });

  it("the same citation twice in one block is one refusal", () => {
    expect(refused("GhostTable here and GhostTable there")).toEqual(["GhostTable"]);
  });
});

describe("what a name may resolve to — four sources, all read off the tree", () => {
  it("an identifier the tree exports or a module it holds", () => {
    expect(refused("the row is a KeyValueRow")).toEqual([]);
  });

  it("a binding the tree imports — the citation is owned elsewhere but reachable here", () => {
    expect(refused("one ReactNode slot per step")).toEqual([]);
  });

  it("a dependency, matched past its scope and punctuation", () => {
    expect(refused("a read-only CodeMirror 6 surface")).toEqual([]);
  });

  it("nothing else — an allowlist is what this oracle exists to not have", () => {
    expect(refused("a read-only MonacoEditor surface")).toEqual(["MonacoEditor"]);
  });
});

describe("paths — a citation names a file the repo holds, at any depth", () => {
  it("a tracked path resolves", () => {
    expect(refused("the tone union lives in ui/src/flow.ts")).toEqual([]);
  });

  it("a bare basename resolves too — prose names a module the way a reader says it", () => {
    expect(refused("the API pretty-prints server-side, decode.rs")).toEqual([]);
  });

  it("a path the repo does not hold is refused", () => {
    expect(refused("(ghost-kit.tsx)")).toEqual(["ghost-kit.tsx"]);
  });

  it("a :N pin is judged on its path — the line is not the part a check can know", () => {
    expect(refused("at inherited size (ui/src/flow.ts:3)")).toEqual([]);
    expect(refused("at inherited size (ghost-kit.tsx:25)")).toEqual(["ghost-kit.tsx:25"]);
  });
});

describe("only the @tier block is asked about", () => {
  const prose = "/**\n * a helper, not a component — see GhostTable\n */\n";
  const declared = "/**\n * Widget — one sentence.\n * @tier T1 — a box\n * @use beside a GhostTable\n */\n";

  it("a comment that declares no tier carries no citations into the check", () => {
    const [block] = tierBlocks(prose + declared);

    expect(refused(block)).toEqual(["GhostTable"]);
    expect(block).not.toContain("a helper, not a component");
  });
});
