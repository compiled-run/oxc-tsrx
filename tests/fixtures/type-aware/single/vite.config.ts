export default {
  lint: {
    plugins: ["typescript"],
    rules: {
      "typescript/no-floating-promises": "error",
    },
    options: {
      typeAware: true,
    },
  },
};
