import tseslint from "typescript-eslint";
import vocabulary from "./eslint-rules/vocabulary.js";

export default [
  { ignores: ["dist/**", "e2e/**", "src-tauri/**"] },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    plugins: { design: { rules: { vocabulary } } },
    rules: { "design/vocabulary": "error" },
  },
  // Tests assert on class names, including deliberate fragments (`text-method-`,
  // `bg-`) that prove a class is gone. Judging them would red the very
  // assertions that hold the vocabulary in place.
  {
    files: ["src/**/*.test.{ts,tsx}", "src/setup-tests.ts"],
    rules: { "design/vocabulary": "off" },
  },
];
