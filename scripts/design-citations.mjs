/* Trawler — every name a registry docstring cites, checked against the tree
 *
 *   node scripts/design-citations.mjs        (exits 1 and names the findings)
 *
 * DEC-16 makes a built component's TSDoc its retired design file, so a docstring
 * naming a deleted component is not a stale comment — it is the design record
 * lying. DEC-30 was minted after exactly that: two entries redirected to
 * components that did not exist, and both redirects read as guidance and
 * resolved to "hand-roll it". This is that failure, mechanised.
 *
 * A docstring is prose, so the check turns on token shape. Two shapes are
 * citations: a PATH, and a two-hump name — the hump is what separates
 * HeaderTable and StepStatusView from Inter, Pass, JSON and the shouted
 * STANDING. Everything else in the block is English and is not asked about.
 *
 * What a name may resolve to is DERIVED, never transcribed — the same rule
 * design-vocabulary.mjs states for its own mapping. Adding a component, a type
 * or a dependency widens what a docstring may cite without touching this file,
 * and that is the property that keeps the check from growing an allowlist.
 */
// dirname + resolve, not new URL(…, import.meta.url): vite rewrites that pattern
// to an asset reference, and this file is imported by the collection's own test.
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { tierBlocks } from "./design-registry.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const SOURCES = resolve(ROOT, "ui/src");
const COLLECTION = resolve(SOURCES, "components");

const NAME = /\b[A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+\b/g;
const PATH = /\b[\w./-]*\w\.(?:tsx?|mjs|js|json|html|rs)\b(?::\d+)?/g;

const fold = (text) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

function sourceFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((found) => {
    const path = join(dir, found.name);
    if (found.isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(found.name) ? [path] : [];
  });
}

function declared(source) {
  const found = new Set();
  const named =
    /export\s+(?:default\s+)?(?:async\s+)?(?:function|const|let|var|class|type|interface|enum)\s+([A-Za-z_$][\w$]*)/g;
  for (const [, name] of source.matchAll(named)) found.add(name);
  for (const [, list] of source.matchAll(/export\s*(?:type\s*)?\{([^}]*)\}/g)) {
    for (const name of identifiers(list)) found.add(name);
  }
  return found;
}

function imported(source) {
  const found = new Set();
  for (const [, clause] of source.matchAll(/import\s+([\s\S]*?)\s+from\s+["']/g)) {
    for (const name of identifiers(clause)) found.add(name);
  }
  return found;
}

// One splitter for both clause shapes: braces are punctuation, `type` and `as`
// are noise, and what a binding is called locally is the name a docstring cites.
function identifiers(clause) {
  return clause
    .replace(/[{}]/g, " ")
    .split(",")
    .map((part) => part.trim().replace(/^type\s+/, "").split(/\s+as\s+/).pop().trim())
    .filter((name) => /^[A-Za-z_$][\w$]*$/.test(name));
}

export function reachable() {
  const names = new Set();
  for (const file of sourceFiles(SOURCES)) {
    const source = readFileSync(file, "utf8");
    for (const name of declared(source)) names.add(name);
    for (const name of imported(source)) names.add(name);
    if (!file.includes(".test.")) names.add(file.replace(/.*\/|\.tsx?$/g, ""));
  }

  const manifest = JSON.parse(readFileSync(resolve(ROOT, "ui/package.json"), "utf8"));
  const packages = new Set(
    Object.keys({ ...manifest.dependencies, ...manifest.devDependencies })
      .flatMap((dep) => dep.replace(/^@/, "").split("/"))
      .map(fold),
  );

  // git, not a directory walk: "the tree it ships with" is exactly what is
  // tracked, and no deny-list of build outputs can drift out of date. A prose
  // citation names a module the way a reader would say it aloud — `decode.rs`,
  // not `src/api/decode.rs` — so a bare basename resolves as readily as a path.
  const tracked = new Set(
    execFileSync("git", ["-C", ROOT, "ls-files"], { encoding: "utf8" })
      .split("\n")
      .filter(Boolean)
      .flatMap((path) => [path, path.replace(/.*\//, "")]),
  );
  return { names, packages, exists: (path) => tracked.has(path) };
}

export function refusals(block, { names, packages, exists }) {
  const found = [];
  for (const [citation] of block.matchAll(PATH)) {
    if (!exists(citation.split(":")[0])) found.push(citation);
  }
  for (const [citation] of block.matchAll(NAME)) {
    if (!names.has(citation) && !packages.has(fold(citation))) found.push(citation);
  }
  return [...new Set(found)];
}

export function unresolved() {
  const tree = reachable();
  const found = {};
  for (const file of readdirSync(COLLECTION).sort()) {
    if (!/\.tsx?$/.test(file) || file.includes(".test.")) continue;
    const [block] = tierBlocks(readFileSync(join(COLLECTION, file), "utf8"));
    if (block === undefined) continue;
    const refused = refusals(block, tree);
    if (refused.length > 0) found[file.replace(/\.tsx?$/, "")] = refused;
  }
  return found;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const found = unresolved();
  for (const [component, citations] of Object.entries(found)) {
    process.stdout.write(`${component}: ${citations.join(", ")}\n`);
  }
  process.exit(Object.keys(found).length === 0 ? 0 : 1);
}
