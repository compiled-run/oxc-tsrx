import { parseSync } from "../../packages/toolchain/dist/parser.js";

export const meta = Object.freeze({
  name: "@oxc-tsrx/eslint-parser-prototype",
  version: "0.1.0",
});

function lineStarts(sourceText) {
  const starts = [0];
  for (let index = 0; index < sourceText.length; index += 1) {
    if (sourceText.charCodeAt(index) === 10) starts.push(index + 1);
  }
  return starts;
}

function positionAt(starts, offset) {
  let low = 0;
  let high = starts.length;
  while (low + 1 < high) {
    const middle = (low + high) >> 1;
    if (starts[middle] <= offset) low = middle;
    else high = middle;
  }
  return { line: low + 1, column: offset - starts[low] };
}

function syntaxError(sourceText, diagnostic) {
  const label = diagnostic.labels?.[0];
  const index = label?.start ?? 0;
  const starts = lineStarts(sourceText);
  const location = positionAt(starts, index);
  const error = new SyntaxError(diagnostic.message);
  error.index = index;
  error.lineNumber = location.line;
  error.column = location.column + 1;
  return error;
}

function prepareForEslint(program, comments, sourceText) {
  const starts = lineStarts(sourceText);
  const visitorKeySets = new Map();
  const seen = new Set();

  function visit(node) {
    if (node === null || typeof node !== "object" || seen.has(node)) return;
    seen.add(node);
    if (typeof node.type !== "string") return;

    const start = node.range?.[0] ?? node.start;
    const end = node.range?.[1] ?? node.end;
    if (Number.isInteger(start) && Number.isInteger(end)) {
      node.range ??= [start, end];
      node.loc = {
        start: positionAt(starts, start),
        end: positionAt(starts, end),
      };
    }

    let keys = visitorKeySets.get(node.type);
    if (!keys) {
      keys = new Set();
      visitorKeySets.set(node.type, keys);
    }

    for (const [key, value] of Object.entries(node)) {
      if (value && typeof value === "object" && typeof value.type === "string") {
        keys.add(key);
        visit(value);
      } else if (
        Array.isArray(value) &&
        value.some((item) => item && typeof item === "object" && typeof item.type === "string")
      ) {
        keys.add(key);
        for (const item of value) visit(item);
      }
    }
  }

  visit(program);
  program.comments = comments.map((comment) => ({
    type: comment.type,
    value: comment.value,
    range: [comment.start, comment.end],
    loc: {
      start: positionAt(starts, comment.start),
      end: positionAt(starts, comment.end),
    },
  }));

  // The v1 parser API does not expose OXC's token stream yet. An empty array is
  // sufficient for AST-only rules and makes the missing token capability
  // explicit: token-dependent rules are not supported by this prototype.
  program.tokens = [];

  return Object.fromEntries(
    [...visitorKeySets].map(([type, keys]) => [type, [...keys]]),
  );
}

export function parseForESLint(sourceText, options = {}) {
  const filePath = options.filePath ?? "input.tsrx";
  const result = parseSync(filePath, sourceText, {
    astType: "ts",
    lang: "tsrx",
    preserveParens: false,
    range: true,
    sourceType: options.sourceType === "script" ? "script" : "module",
  });
  if (result.errors.length > 0) throw syntaxError(sourceText, result.errors[0]);
  if (!result.program) throw new SyntaxError(`TSRX parser returned no Program for ${filePath}`);

  const visitorKeys = prepareForEslint(result.program, result.comments, sourceText);
  return {
    ast: result.program,
    visitorKeys,
    services: {
      isTsrx: true,
      parser: "oxc-tsrx/parser",
    },
  };
}

export function parse(sourceText, options) {
  return parseForESLint(sourceText, options).ast;
}

export default { meta, parse, parseForESLint };
