const extensions = Object.freeze([".tsrx"]);
const capabilities = Object.freeze(["parser", "lint", "format", "languageServer"]);

export const toolchain = Object.freeze({
  name: "oxc-tsrx",
  language: "tsrx",
  extensions,
  capabilities,
});
