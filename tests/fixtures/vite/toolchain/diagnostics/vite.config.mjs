import { defineConfig } from 'vite-plus';

export default defineConfig({
  lint: {
    rules: {
      'no-debugger': 'error',
    },
  },
  fmt: {
    semi: true,
    singleQuote: true,
  },
});
