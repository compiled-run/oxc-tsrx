import { defineConfig } from 'vite-plus';

export default defineConfig({
  lint: {
    rules: {
      'no-var': 'error',
    },
  },
  fmt: {
    semi: true,
    singleQuote: true,
  },
});
