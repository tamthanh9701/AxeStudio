// @ts-check
import js from "@eslint/js"
import tseslint from "typescript-eslint"

export default tseslint.config(
  {
    ignores: ["**/dist/**", "**/node_modules/**", "**/target/**", "vendor/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      // Contract: UI không được tự định nghiĩa lại kiểu đã sinh từ Rust.
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["**/bindings/src/generated*"],
              message: "Import từ @als/bindings, không import thẳng file sinh tự động.",
            },
          ],
        },
      ],
    },
  },
  {
    // Script build/CI chạy trong Node — cần globals ngoài browser.
    files: ["scripts/**/*.mjs", "*.config.js", "*.config.ts"],
    languageOptions: {
      globals: {
        console: "readonly",
        process: "readonly",
      },
    },
  },
)
