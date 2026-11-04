import { describe, expect, it } from "vitest";
import {
  amend,
  decodeOriginal,
  canRedo,
  canUndo,
  commit,
  editCount,
  isModified,
  redo,
  restoreOriginal,
  startHistory,
  undo,
  workingBytes,
  workingText,
  type History,
} from "./workingCopy";

const steps = (history: History) => history.steps.map((s) => s.name);

describe("one history of named steps", () => {
  it("starts at the original, unmodified, with nothing to undo or redo", () => {
    const history = startHistory("{}");
    expect(workingText(history)).toBe("{}");
    expect([isModified(history), canUndo(history), canRedo(history), editCount(history)]).toEqual([false, false, false, 0]);
  });

  it("each change is a step; undo and redo walk them", () => {
    let history = commit(startHistory("a"), "ab", "typed on line 1");
    history = commit(history, "abc", "typed on line 1");
    expect(editCount(history)).toBe(2);

    history = undo(history);
    expect(workingText(history)).toBe("ab");
    history = undo(history);
    expect(workingText(history)).toBe("a");
    expect(isModified(history)).toBe(false);
    expect(undo(history)).toBe(history);

    history = redo(redo(history));
    expect(workingText(history)).toBe("abc");
    expect(redo(history)).toBe(history);
  });

  it("a change that changes nothing adds no step", () => {
    const history = startHistory("a");
    expect(commit(history, "a", "typed")).toBe(history);
  });

  it("a change after an undo drops the steps that were undone", () => {
    const history = commit(undo(commit(commit(startHistory("a"), "b", "one"), "c", "two")), "d", "three");
    expect(steps(history)).toEqual(["original", "one", "three"]);
    expect(canRedo(history)).toBe(false);
  });

  it("an edit back to the original text reads unmodified, though it is a step", () => {
    const history = commit(commit(startHistory("a"), "b", "one"), "a", "two");
    expect(isModified(history)).toBe(false);
    expect(editCount(history)).toBe(2);
  });
});

describe("a step the surface is still typing into", () => {
  it("is amended in place, so one input group undoes as one step", () => {
    let history = commit(startHistory("a"), "ab", "typed on line 1");
    history = amend(history, "abc", "typed on line 1");
    expect(editCount(history)).toBe(1);
    expect(workingText(history)).toBe("abc");
    expect(workingText(undo(history))).toBe("a");
  });

  it("never rewrites the original or a step that was undone past", () => {
    expect(steps(amend(startHistory("a"), "ab", "typed"))).toEqual(["original", "typed"]);
    const undone = undo(commit(commit(startHistory("a"), "b", "one"), "c", "two"));
    const amended = amend(undone, "bx", "typed");
    expect(steps(amended)).toEqual(["original", "one", "typed"]);
    expect(workingText(amended)).toBe("bx");
  });
});

describe("restore original", () => {
  it("is a step of its own, so it undoes like any other", () => {
    const edited = commit(startHistory("orig"), "edited", "typed");
    const restored = restoreOriginal(edited);
    expect(workingText(restored)).toBe("orig");
    expect(steps(restored)).toEqual(["original", "typed", "restore original"]);
    expect(workingText(undo(restored))).toBe("edited");
  });

  it("after any sequence of steps the bytes delivered are the exact original bytes", () => {
    const original = new Uint8Array([0x7b, 0xff, 0x7d]);
    const decoded = new TextDecoder().decode(original);
    let history = startHistory(decoded);
    for (const text of ["{x}", "{xy}", "", "{é}"]) history = commit(history, text, "typed");
    history = undo(history);
    history = restoreOriginal(history);

    expect(workingBytes(history, original)).toBe(original);
  });

  it("an unmodified copy is the original bytes, a modified one its text encoded", () => {
    const original = new TextEncoder().encode("a");
    expect(workingBytes(startHistory("a"), original)).toBe(original);
    expect(workingBytes(commit(startHistory("a"), "é", "typed"), original)).toEqual(new TextEncoder().encode("é"));
  });
});

describe("the history is bounded", () => {
  it("past its budget the oldest edits go, the original never does, and the count still says how many", () => {
    const budget = 10;
    let history = startHistory("orig");
    for (const text of ["aaaa", "bbbb", "cccc", "dddd"]) history = commit(history, text, `set ${text}`, budget);
    expect(steps(history)).toEqual(["original", "set cccc", "set dddd"]);
    expect(editCount(history)).toBe(4);
    expect(workingText(history)).toBe("dddd");
    expect(workingText(restoreOriginal(history, budget))).toBe("orig");
    expect(workingText(undo(undo(history)))).toBe("orig");
  });

  it("the step being made is kept even when it alone is past the budget", () => {
    const history = commit(startHistory("a"), "x".repeat(50), "pasted", 10);
    expect(workingText(history)).toBe("x".repeat(50));
  });
});

describe("the original as text", () => {
  it("valid UTF-8 decodes whole, a byte-order mark kept so it re-encodes", () => {
    const bom = new Uint8Array([0xef, 0xbb, 0xbf, 0x61]);
    const decoded = decodeOriginal(bom);
    expect(decoded.whole).toBe(true);
    expect(Array.from(new TextEncoder().encode(decoded.text))).toEqual(Array.from(bom));
  });

  it("bytes that are not UTF-8 decode lossily and say so", () => {
    expect(decodeOriginal(new Uint8Array([0x61, 0xff]))).toEqual({ text: "a\ufffd", whole: false });
  });
});
