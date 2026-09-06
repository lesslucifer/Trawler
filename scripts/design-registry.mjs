/* Trawler — the component registry, as JSON, from the collection's own TSDoc
 *
 *   node scripts/design-registry.mjs
 *
 * The vocabulary a design run cites — tier, @use, @avoid, @states, @madeof for
 * every component — read from ui/src/components/ and emitted on demand. Nothing
 * is committed, so nothing can be stale, and the emitted index IS the folder:
 * the freshness gate and index-covers-folder both dissolve (E-DFLOW.W7 D3). Same
 * shape scripts/design-vocabulary.mjs uses for the token vocabulary.
 *
 * Membership is @tier. A file that carries one is in the vocabulary; a file that
 * carries none is out — the waiver, stated where it cannot drift from the thing
 * it waives. A block that HAS @tier and is missing the rest throws, because a
 * short roster emitted silently is the failure this oracle exists to prevent.
 */
// dirname + resolve, not new URL(…, import.meta.url): vite rewrites that pattern
// to an asset reference, and this file is imported by the collection's own test.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const COLLECTION = resolve(HERE, "../ui/src/components");
const TAGS = ["tier", "use", "avoid", "states", "madeof"];

function tagsOf(block) {
  const tags = {};
  let open = "summary";
  for (const line of block.split("\n")) {
    const text = line.replace(/^\s*\*?\s?/, "").trim();
    const tag = /^@(\w+)\s*(.*)$/.exec(text);
    if (tag) {
      open = tag[1];
      tags[open] = tag[2].trim();
    } else {
      tags[open] = `${tags[open] ?? ""} ${text}`.trim();
    }
  }
  return tags;
}

export function tierBlocks(source) {
  return [...source.matchAll(/\/\*\*([\s\S]*?)\*\//g)]
    .map((block) => block[1])
    .filter((body) => "tier" in tagsOf(body));
}

export function entry(source, name) {
  const declared = tierBlocks(source);
  if (declared.length === 0) return null;
  if (declared.length > 1) {
    throw new Error(
      `${name}: ${declared.length} TSDoc blocks declare @tier — a registry entry is one block`,
    );
  }

  const tags = tagsOf(declared[0]);
  if (!tags.summary) {
    throw new Error(`${name}: the @tier block opens with no summary line — the entry has no one-liner`);
  }
  const declares = TAGS.map((t) => `@${t}`).join(" ");
  for (const tag of TAGS) {
    if (!tags[tag]) {
      const state = tag in tags ? "empty" : "missing";
      throw new Error(`${name}: @${tag} is ${state} — a registry entry declares ${declares}`);
    }
  }
  if (!/^T\d\b/.test(tags.tier)) {
    throw new Error(`${name}: @tier reads "${tags.tier}" — it must open with a tier, T1 … T4`);
  }

  const kept = Object.fromEntries(TAGS.map((tag) => [tag, tags[tag]]));
  return { name, summary: tags.summary, ...kept };
}

export function registry() {
  // The collection's tests hold TSDoc fixtures as strings; reading them would
  // register the fixtures and throw on the malformed ones.
  const found = readdirSync(COLLECTION)
    .filter((file) => /\.tsx?$/.test(file) && !file.includes(".test."))
    .sort()
    .map((file) => entry(readFileSync(join(COLLECTION, file), "utf8"), file.replace(/\.tsx?$/, "")))
    .flatMap((component) => (component === null ? [] : [component]));

  // A redirect is a roster name the @avoid cites whole — the Button in
  // CopyButton is part of a name, not a pointer to another component.
  const cites = (avoid, name) => new RegExp(`\\b${name}\\b`).test(avoid);
  const names = found.map((component) => component.name);
  return Object.fromEntries(
    found.map((component) => [
      component.name,
      { ...component, redirects: names.filter((n) => cites(component.avoid, n)) },
    ]),
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.stdout.write(`${JSON.stringify(registry())}\n`);
}
