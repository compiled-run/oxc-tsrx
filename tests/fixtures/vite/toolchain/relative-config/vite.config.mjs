import { defineConfig } from "vite-plus";
import lintBase from "./lint-base.mjs";

export default defineConfig({
  lint: {
    extends: [lintBase],
    ignorePatterns: ["src/ignored.tsrx"],
    overrides: [
      {
        files: ["src/**/*.tsrx"],
        rules: {
          "no-console": "error",
        },
      },
      {
        files: ["src/**/*.tsx"],
        rules: {
          "no-console": "error",
        },
      },
    ],
  },
  fmt: {
    semi: true,
    singleQuote: true,
    ignorePatterns: ["src/ignored.tsrx"],
    overrides: [
      {
        files: ["src/**/*.tsrx"],
        options: {
          semi: false,
        },
      },
      {
        files: ["src/**/*.tsx"],
        options: {
          singleQuote: false,
        },
      },
    ],
  },
});
