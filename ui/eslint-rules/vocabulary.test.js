import { globSync, readFileSync } from "node:fs";
import { RuleTester } from "eslint";
import { describe, expect, it } from "vitest";
import resolveConfig from "tailwindcss/resolveConfig.js";
import contextUtils from "tailwindcss/lib/lib/setupContextUtils.js";
import tailwindConfig from "../tailwind.config.js";
import { emittableClasses } from "./classes.js";
import vocabulary from "./vocabulary.js";

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 2022,
    sourceType: "module",
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});

ruleTester.run("design/vocabulary", vocabulary, {
  valid: [
    // One case per false-positive family the scale-bearing scoping exists to
    // protect. A future edit that re-widens the rule reds one of these.
    { code: `const A = () => <i className="absolute flex grid truncate uppercase sr-only" />;` },
    { code: `const A = () => <i className="text-center border-none outline-none font-mono" />;` },
    { code: `const A = () => <i className="hover:text-ink focus-visible:outline-focus" />;` },
    { code: `const A = () => <i className="bg-canvas-soft/wash border-hairline/hairline" />;` },
    { code: `const A = () => <i className="text-body/loose" />;` },
    { code: `const A = () => <i className="p-0 m-0 gap-0 min-w-0 inset-0" />;` },
    { code: `const A = () => <i className="w-full max-w-full rounded-full ml-auto leading-none" />;` },
    { code: `const TONE = { ok: "text-ok" };\nconst A = ({ t }) => <i className={TONE[t]} />;` },
    { code: `const A = () => <i className="px-chrome-x py-chrome-y text-body z-popup" />;` },
    // The eight scales that carry no magnitude but are declared anyway, so the
    // rule can see into them at all (step 3 sweep).
    {
      code: `const A = () => <i className="animate-spin cursor-pointer flex-1 shrink-0 col-start-1 row-start-1 list-none transition-colors" />;`,
    },
    {
      code: `const A = () => (\n  <i\n    className={\n      // design-allow: grid-cols-[auto_1fr] — a two-column template shape, no size in it\n      "grid grid-cols-[auto_1fr]"\n    }\n  />\n);`,
    },
  ],
  invalid: [
    {
      code: `const A = () => <i className="text-xs" />;`,
      errors: [{ messageId: "offlist" }],
    },
    {
      code: `const A = () => <i className="text-[11px]" />;`,
      errors: [{ messageId: "arbitrary" }],
    },
    // A scale the config never declares inherits Tailwind's whole ladder, and
    // every member of it emits — so nothing could red inside it. `font-bold`
    // was legal this way until step ② read the config by hand. Legality is
    // membership of a DECLARED scale: `duration-500` draws from one the project
    // never names, `animate-bounce` from one it names without that member.
    {
      code: `const A = () => <i className="duration-500 animate-bounce font-bold" />;`,
      errors: [{ messageId: "offlist" }, { messageId: "offlist" }, { messageId: "offlist" }],
    },
    {
      code: `const A = () => <i className="bg-canvas-soft/30" />;`,
      errors: [{ messageId: "modifier" }],
    },
    // A modifier belongs to the scale its own base draws from, not to one global
    // step list: `text-` is a colour prefix and a size prefix at once, and
    // Tailwind emits nothing at all — not even the base — for the wrong pairing.
    {
      code: `const A = () => <i className="text-body/wash" />;`,
      errors: [{ messageId: "modifier" }],
    },
    // A base that carries no modifier scale at all is off-list outright: there is
    // no scale to have picked the wrong step from.
    {
      code: `const A = () => <i className="rounded-full/wash z-popup/wash" />;`,
      errors: [{ messageId: "offlist" }, { messageId: "offlist" }],
    },
    // Requirement 2: the class the WU exists to delete is reachable only through
    // the lookup map. A className-anchored reader reports this file as clean.
    {
      code: `const METHOD_COLOR = { GET: "text-method-get" };\nconst A = ({ v }) => <i className={METHOD_COLOR[v]} />;`,
      errors: [{ messageId: "offlist" }],
    },
    {
      code: `const A = ({ t }) => <i className={\`text-\${t}\`} />;`,
      errors: [{ messageId: "unresolvable" }],
    },
    // Identifier-following is by name across the whole file, so a name declared
    // twice must keep both declarations. Keeping only the last one reds nothing
    // and reports the file clean — the silence this rule exists to end.
    {
      code: `const A = () => {\n  const cls = "text-xs";\n  return <i className={cls} />;\n};\nconst B = () => {\n  const cls = "flex";\n  return <i className={cls} />;\n};`,
      errors: [{ messageId: "offlist" }],
    },
    {
      code: `const A = () => (\n  <i\n    className={\n      // design-allow: grid-cols-[auto_1fr]\n      "grid grid-cols-[auto_1fr]"\n    }\n  />\n);`,
      errors: [{ messageId: "pragmaMalformed" }, { messageId: "arbitrary" }],
    },
    {
      code: `const A = () => (\n  <i\n    className={\n      // design-allow: text-[11px] — the chrome size\n      "text-[11px]"\n    }\n  />\n);`,
      errors: [{ messageId: "pragmaOverSize" }, { messageId: "arbitrary" }],
    },
    {
      code: `const A = () => (\n  <i\n    className={\n      // design-allow: grid-cols-[auto_1fr] — a template shape\n      "flex"\n    }\n  />\n);`,
      errors: [{ messageId: "pragmaStale" }],
    },
  ],
});

const duplicates = (classes) => {
  const seen = new Set();
  return classes.filter((c) => (seen.has(c) ? true : (seen.add(c), false)));
};

const classListFor = (theme) =>
  contextUtils
    .createContext(resolveConfig({ ...tailwindConfig, theme: { ...tailwindConfig.theme, ...theme } }))
    .getClassList();

describe("the closed vocabulary itself", () => {
  // Requirement 4, mechanised: two scales sharing a class prefix must not share
  // a token name. `border-hairline` once set width AND colour, silently taking
  // 22 borders from 1px to 1.5px with tsc and 447 tests green.
  it("has no token name colliding across two scales that share a prefix", () => {
    expect(duplicates(emittableClasses)).toEqual([]);
  });

  it("catches the collision that closing the config introduced", () => {
    const collided = classListFor({
      borderWidth: { ...tailwindConfig.theme.borderWidth, hairline: "1.5px" },
    });
    expect(duplicates(collided)).toContain("border-hairline");
  });

  // colour-tokens-only on the code surface, where the lint rule cannot reach.
  // A component adapting a third-party styling API — CodeMirror's theme object,
  // a raw stylesheet — has no class to give it, so the literal is the sanctioned
  // form. It must still BE a palette value: SourceSurface's three DEC-11 tones
  // were literals, so deleting their tokens left the screen painting colours the
  // vocabulary no longer contained, and every check stayed green.
  // Hex only, which is this tree's idiom; an rgb() literal would slip past.
  it("writes no colour literal that is not a palette value", () => {
    const theme = resolveConfig(tailwindConfig).theme;
    const palette = new Set(
      [...Object.values(theme.colors), ...Object.values(theme.backgroundColor)]
        .filter((value) => typeof value === "string")
        .map((value) => value.toLowerCase()),
    );

    const stray = globSync("src/**/*.{ts,tsx,css}")
      .filter((file) => !/\.test\.tsx?$/.test(file))
      .flatMap((file) =>
        (readFileSync(file, "utf8").match(/#[0-9a-fA-F]{3,8}\b/g) ?? [])
          .filter((hex) => !palette.has(hex.toLowerCase()))
          .map((hex) => `${file}: ${hex}`),
      );

    expect(stray).toEqual([]);
  });
});
