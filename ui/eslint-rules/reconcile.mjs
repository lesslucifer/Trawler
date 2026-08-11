// Requirement 5: the rule's own enumeration is an assumption until it is diffed
// against what Tailwind actually builds. Two independent oracles — the rule's
// getClassList() and the emitted stylesheet — must agree on every class this
// tree authors, or one of them is wrong.
import { globSync, readFileSync } from "node:fs";
import postcss from "postcss";
import tailwindcss from "tailwindcss";
import tseslint from "typescript-eslint";
import { arbitraryValue, classify, collectClasses, isScaleBearing } from "./classes.js";
import tailwindConfig from "../tailwind.config.js";

const sources = globSync("src/**/*.{ts,tsx}").filter((f) => !/\.test\.tsx?$/.test(f));

const authored = new Map();
for (const file of sources) {
  const code = readFileSync(file, "utf8");
  const { ast } = tseslint.parser.parseForESLint(code, {
    range: true,
    loc: true,
    ecmaFeatures: { jsx: file.endsWith("x") },
  });
  for (const { cls, partial } of collectClasses(ast)) {
    if (partial) continue;
    if (!authored.has(cls)) authored.set(cls, new Set());
    authored.get(cls).add(file);
  }
}

const css = await postcss([tailwindcss(tailwindConfig)]).process("@tailwind utilities;", {
  from: undefined,
});
const built = new Set(
  [...css.css.matchAll(/\.((?:\\.|[^\s{},:>~+[\]().])+)/g)].map((m) => m[1].replace(/\\/g, "")),
);

// Arbitrary values are the one place the two oracles are meant to disagree:
// Tailwind emits them whether or not a scale is closed, which is why the rule
// has to red them and why closing the config could never have been enough.
const disagreements = [];
const expected = [];
for (const cls of authored.keys()) {
  const legal = classify(cls).legal;
  const emits = built.has(cls);
  if (legal === emits) continue;
  const row = { cls, legal, emits, where: [...authored.get(cls)] };
  (!legal && emits && arbitraryValue(cls) !== null ? expected : disagreements).push(row);
}

const bearing = [...authored.keys()].filter((c) => isScaleBearing(c));
const structural = [...authored.keys()].filter((c) => !isScaleBearing(c));
const illegal = [...authored.keys()].filter((c) => !classify(c).legal);

console.log(`authored classes   ${authored.size} distinct across ${sources.length} files`);
console.log(`  scale-bearing    ${bearing.length}`);
console.log(`  structural       ${structural.length}`);
console.log(`built stylesheet   ${built.size} emitted classes`);
console.log(`classifier reds    ${illegal.length} distinct (before design-allow pragmas)`);
console.log(
  `\nreconciliation     ${disagreements.length === 0 ? "AGREE" : "DISAGREE"} on ${authored.size - expected.length} classes; ${expected.length} arbitrary values diverge by design`,
);
for (const d of disagreements) {
  console.log(
    `  ${d.cls}: rule says ${d.legal ? "legal" : "illegal"}, CSS ${d.emits ? "emits" : "does not emit"} it — ${d.where.join(", ")}`,
  );
}
if (process.argv.includes("--list-reds")) {
  console.log("\nreds:");
  for (const cls of illegal.sort()) {
    console.log(`  ${cls}  (${isScaleBearing(cls) ? "scale-bearing" : "structural"})  ${[...authored.get(cls)].length} file(s)`);
  }
}
process.exit(disagreements.length === 0 ? 0 : 1);
