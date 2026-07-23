import { parseSync } from "../../packages/parser/index.js";

const TSRX_ID = /\.tsrx(?:$|\?)/u;
export const TSRX_PARSER_SERVICE = Symbol.for("@oxc-tsrx/vite/parser-service");

function cleanId(id) {
  return id.split("?", 1)[0];
}

function isAuthoredTsrxId(id) {
  return !id.startsWith("\0") && cleanId(id).endsWith(".tsrx");
}

function parserFailure(id, errors) {
  const first = errors[0];
  const label = first?.labels?.[0];
  const error = new SyntaxError(`${id}: ${first?.message ?? "TSRX parse failed"}`);
  if (label) {
    error.index = label.start;
    error.pos = label.start;
  }
  return error;
}

/**
 * Parse raw `.tsrx` before a framework's Vite transform and retain the result
 * for other plugins in the same Vite process.
 */
export function tsrxParserService(options = {}) {
  const cache = new Map();
  const parserOptions = Object.freeze({
    ...options.parserOptions,
    lang: "tsrx",
    range: true,
  });

  const api = Object.freeze({
    name: "@oxc-tsrx/parser",
    version: 1,
    [TSRX_PARSER_SERVICE]: true,

    parse(id, sourceText) {
      const path = cleanId(id);
      const cached = cache.get(path);
      if (cached?.sourceText === sourceText) return cached.result;

      const result = parseSync(path, sourceText, parserOptions);
      if (result.errors.length > 0) throw parserFailure(path, result.errors);
      cache.set(path, { sourceText, result });
      options.onParse?.({ id: path, sourceText, result });
      return result;
    },

    get(id) {
      return cache.get(cleanId(id))?.result ?? null;
    },

    invalidate(id) {
      cache.delete(cleanId(id));
    },

    clear() {
      cache.clear();
    },
  });

  return {
    name: "@oxc-tsrx/vite-parser-service",
    enforce: "pre",
    api,

    transform: {
      order: "pre",
      filter: { id: TSRX_ID },
      handler(sourceText, id) {
        if (!isAuthoredTsrxId(id)) return null;
        try {
          api.parse(id, sourceText);
        } catch (error) {
          this.error({
            message: error.message,
            pos: Number.isInteger(error.pos) ? error.pos : undefined,
          });
        }
        return null;
      },
    },

    handleHotUpdate(context) {
      if (isAuthoredTsrxId(context.file)) api.invalidate(context.file);
    },

    closeBundle() {
      api.clear();
    },
  };
}

/**
 * Put the parser service and parser-aware plugins immediately before an
 * existing TSRX framework plugin. Vite flattens the returned preset.
 */
export function withTsrxParser(frameworkPlugin, createPlugins, options) {
  const parserPlugin = tsrxParserService(options);
  const consumers = createPlugins?.(parserPlugin.api);
  return [
    parserPlugin,
    ...(Array.isArray(consumers) ? consumers : consumers ? [consumers] : []),
    frameworkPlugin,
  ];
}
