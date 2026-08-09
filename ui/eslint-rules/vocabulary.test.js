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
    { code: `const A = () => <i className="p-0 m-0 gap-0 min-w-0 inset-0" />;` },
    { code: `const A = () => <i className="w-full max-w-full rounded-full ml-auto leading-none" />;` },
    { code: `const TONE = { ok: "text-ok" };\nconst A = ({ t }) => <i className={TONE[t]} />;` },
    { code: `const A = () => <i className="px-chrome-x py-chrome-y text-body z-popup" />;` },
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
    {
      code: `const A = () => <i className="bg-canvas-soft/30" />;`,
      errors: [{ messageId: "modifier" }],
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
});
