import tsrxDemo from "./demo-lint-plugin.mjs";
import tsrxParser from "./tsrx-eslint-parser.mjs";

export default [
  {
    files: ["**/*.tsrx"],
    languageOptions: { parser: tsrxParser, sourceType: "module" },
    plugins: { "tsrx-demo": tsrxDemo },
    rules: {
      "tsrx-demo/no-tsrx-if": "warn",
      "tsrx-demo/require-keyed-for": "error",
    },
  },
];
