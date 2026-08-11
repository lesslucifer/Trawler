/* Trawler — the closed vocabulary, as JSON, for the Tier-1 design predicates
 *
 *   node scripts/design-vocabulary.mjs        (or: via cargo xtask verify-doc)
 *
 * Five hard rules judge a design run's classes against ui/tailwind.config.js.
 * Rust cannot read an ES module, so the same oracle the lint rule uses —
 * Tailwind's own getClassList() over the closed config — is emitted here once
 * and parsed there. One source, two surfaces (DEC-17).
 *
 * A rule needs more than the legal list: it must know which classes are ITS
 * business. That mapping is derived, never transcribed — a scale's classes are
 * the ones that stop being emitted when the scale is emptied, and a scale's
 * prefixes are what remains of those classes once the token name is taken off.
 * Adding a config key therefore widens the vocabulary without touching this file.
 */
// Tailwind is a devDependency of ui/, and pnpm does not hoist — the same reason
// build-specimens.mjs reaches for ui/node_modules/.bin/esbuild by path.
import resolveConfig from "../ui/node_modules/tailwindcss/resolveConfig.js";
import cfg from "../ui/tailwind.config.js";
import { closedContext, emittableClasses, modifierSteps } from "../ui/eslint-rules/classes.js";

// Which scales each hard rule governs. The one hand-written table here, and it
// is a statement about the RULEBOOK, not about Tailwind: `spacing-scale` owns
// maxWidth because the rulebook says magnitudes come from one ladder.
const GOVERNS = {
  "colour-tokens-only": ["colors", "backgroundColor"],
  "spacing-scale": ["spacing", "maxWidth"],
  "type-tokens-only": ["fontSize", "fontFamily", "fontWeight", "lineHeight", "letterSpacing"],
  "radius-tokens": ["borderRadius"],
};

const resolved = resolveConfig(cfg).theme;
const legal = new Set(emittableClasses);

function classesOf(scale) {
  const without = new Set(closedContext({ [scale]: {} }).getClassList());
  return [...legal].filter((cls) => !without.has(cls));
}

function prefixesOf(scale) {
  const keys = Object.keys(resolved[scale] ?? {});
  const found = new Set();
  for (const cls of classesOf(scale)) {
    const key = keys
      .filter((k) => cls === k || cls.endsWith(`-${k}`))
      .sort((a, b) => b.length - a.length)[0];
    if (key !== undefined && cls !== key) found.add(cls.slice(0, cls.length - key.length));
  }
  return [...found];
}

const rules = {};
for (const [slug, scales] of Object.entries(GOVERNS)) {
  rules[slug] = [...new Set(scales.flatMap(prefixesOf))].sort();
}

// A design run's mockup may own CSS the vocabulary cannot reach as a class —
// assembly rules under the `.a-` prefix. Those declarations are judged on their
// VALUE, so each rule also carries the literal magnitudes its scales define.
const values = {};
for (const [slug, scales] of Object.entries(GOVERNS)) {
  values[slug] = [
    ...new Set(
      scales.flatMap((scale) => Object.values(resolved[scale] ?? {})).flatMap(flatten),
    ),
  ].sort();
}

function flatten(value) {
  if (typeof value === "string") return [value.trim()];
  if (Array.isArray(value)) return [value.join(", ")].concat(value.map((v) => `${v}`.trim()));
  if (value && typeof value === "object") return Object.values(value).flatMap(flatten);
  return [];
}

// A `/step` draws from the scale its own base draws from — `text-ink/wash` dims
// a colour, `text-body/loose` sets a line height, and the wrong pairing emits
// nothing at all. Only the bases that carry steps are listed.
const modifiers = {};
for (const [name, steps] of modifierSteps) {
  if (steps.size) modifiers[name] = [...steps];
}

// Measured, not tuned: the palette's warmest neutral spans 28 (ink-body #c9c0ad)
// and its faintest chromatic token spans 47 (syn-str #9ec99a), so any floor in
// that gap separates them and 40 sits in the middle of it.
const CHROMA_FLOOR = 40;

function isChromatic(value) {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value.trim());
  if (!hex) return false;
  const raw = hex[1];
  const pair = (i) =>
    raw.length === 3 ? parseInt(raw[i] + raw[i], 16) : parseInt(raw.slice(i * 2, i * 2 + 2), 16);
  const [r, g, b] = [pair(0), pair(1), pair(2)];
  return Math.max(r, g, b) - Math.min(r, g, b) > CHROMA_FLOOR;
}

// The chromatic set is whatever the config says it is. A colour is chromatic
// when its channels differ — the palette's warm greys are near-neutral by
// construction, so nothing here needs a list of "the accent ones" to drift.
const chromatic = Object.entries(resolved.colors ?? {})
  .filter(([, value]) => typeof value === "string" && isChromatic(value))
  .map(([name]) => name);

process.stdout.write(
  `${JSON.stringify({
    legal: [...legal].sort(),
    rules,
    values,
    modifiers,
    chromatic: chromatic.sort(),
    // The predicates judge inline values the config never saw, against the same
    // floor the set above was measured with - so it crosses the bridge too.
    chroma_floor: CHROMA_FLOOR,
  })}\n`,
);
