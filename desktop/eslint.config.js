// The desktop's lint: the rules of hooks, with `exhaustive-deps` as an error —
// the one class of bug a type-check cannot see (an effect that never re-runs
// because a value it reads is not in its list). Every `eslint-disable-next-line
// react-hooks/exhaustive-deps` in the tree carries its reason on the line above.
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

export default tseslint.config(
  { ignores: ["dist/**", "src-tauri/**", "node_modules/**", "src/types.gen.ts"] },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
      globals: { ...globals.browser },
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
);
