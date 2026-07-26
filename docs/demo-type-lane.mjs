// Scaffolding the type-aware lint lane needs. tsgolint only reports on a file
// that sits inside an inferable TypeScript program, so every type-lane run
// writes a tsconfig plus a minimal JSX contract next to the source, and
// prefixes the source with a reference line. The prefix shifts every byte
// offset, so diagnostics have to be shifted back before they reach the editor.
//
// docs/serve.mjs (live runs) and docs/generate-type-error.mjs (the committed
// pre-generated report) both import this, so the published site's replayed
// diagnostics land on exactly the bytes a live run would underline.

export const JSX_CONTRACT = `declare namespace JSX {
  interface IntrinsicElements {
    [name: string]: unknown;
  }
}

declare module "react/jsx-runtime" {
  export const Fragment: unknown;
  export function jsx(type: unknown, properties: unknown): unknown;
  export function jsxs(type: unknown, properties: unknown): unknown;
}
`

export const DEMO_TSCONFIG = `{
  "compilerOptions": {
    "jsx": "preserve",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "noEmit": true,
    "strict": true,
    "target": "ESNext"
  },
  "include": ["demo.tsrx", "jsx.d.ts"]
}
`

export const TYPE_PREFIX = '/// <reference path="./jsx.d.ts" />\n'

export const TYPE_PREFIX_BYTES = Buffer.byteLength(TYPE_PREFIX)

// Maps raw report diagnostics onto the user's original bytes, dropping any
// whose labels fall inside the injected prefix.
export function normalizeDiagnostics(diagnostics, prefixBytes) {
  return (diagnostics ?? [])
    .map((diagnostic) => ({
      rule: diagnostic.rule,
      code: diagnostic.code,
      severity: diagnostic.severity,
      message: diagnostic.message,
      labels: (diagnostic.labels ?? []).map((label) => ({
        ...label,
        span: { ...label.span, offset: label.span.offset - prefixBytes },
      })),
    }))
    .filter((diagnostic) => diagnostic.labels.every((label) => label.span.offset >= 0))
}
