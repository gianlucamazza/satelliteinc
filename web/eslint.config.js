import tseslint from "typescript-eslint";

export default tseslint.config(
  tseslint.configs.strictTypeChecked,
  tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // HUD strings interpolate numbers/booleans constantly; both render
      // unambiguously.
      "@typescript-eslint/restrict-template-expressions": [
        "error",
        { allowNumber: true, allowBoolean: true },
      ],
      // `!` is accepted only for elements statically present in index.html.
      "@typescript-eslint/no-non-null-assertion": "off",
    },
  },
  { ignores: ["dist/", "**/*.config.*"] },
);
