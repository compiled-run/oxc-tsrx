function* walk(value) {
  if (Array.isArray(value)) {
    for (const item of value) yield* walk(item);
  } else if (value && typeof value === "object") {
    if (typeof value.type === "string") yield value;
    for (const child of Object.values(value)) yield* walk(child);
  }
}

/**
 * A Vite plugin consuming the raw authored AST retained by
 * `tsrxParserService`. It emits a normal Vite warning for demo purposes.
 */
export function tsrxDemoLint(parser, options = {}) {
  return {
    name: "demo:tsrx-ast-lint",
    enforce: "pre",

    transform(sourceText, id) {
      if (id.startsWith("\0") || !id.split("?", 1)[0].endsWith(".tsrx")) return null;
      const result = parser.parse(id, sourceText);
      for (const node of walk(result.program)) {
        if (node.type !== "JSXIfExpression") continue;
        options.onFinding?.({ id, node, sourceText });
        this.warn({
          code: "TSRX_DEMO_NO_IF",
          message: "Demo Vite lint saw the authored TSRX @if AST node.",
          pos: node.start,
        });
      }
      return null;
    },
  };
}
