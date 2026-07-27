import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { createTsrxCoreCompat } from "../../packages/tsrx-core-compat/facade.js";

function makeProgram() {
  return {
    type: "Program",
    start: 0,
    end: 21,
    sourceType: "module",
    hashbang: null,
    body: [],
  };
}

function makeNativeError(message = "Unexpected token") {
  return {
    severity: "Error",
    message,
    labels: [{ start: 13, end: 14, message: "expected an expression" }],
    helpMessage: null,
    codeframe: null,
  };
}

test("parseModule delegates to the TSRX parser with the exact OXC options", () => {
  const calls = [];
  const program = makeProgram();
  const api = createTsrxCoreCompat({
    parseSync(...args) {
      calls.push(args);
      return { program, comments: [], errors: [] };
    },
  });

  assert.equal(api.parseModule("export const answer = 42"), program);
  assert.deepEqual(calls, [
    [
      "module.tsrx",
      "export const answer = 42",
      { lang: "tsrx", sourceType: "module", astType: "js", preserveParens: false },
    ],
  ]);
  const marker = Object.getOwnPropertyDescriptor(
    calls[0][2],
    Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-eager"),
  );
  assert.deepEqual(marker, {
    value: true,
    writable: false,
    enumerable: false,
    configurable: false,
  });
});

test("comment and loose paths retain the complete lazy parser result", () => {
  const seen = [];
  const api = createTsrxCoreCompat({
    parseSync(_filename, _source, options) {
      seen.push(options);
      return { program: makeProgram(), comments: [], errors: [] };
    },
  });
  api.parseModule("export {}", "Comments.tsrx", { comments: [] });
  api.parseModule("export {}", "Loose.tsrx", { loose: true, errors: [] });

  const marker = Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-eager");
  assert.equal(seen.length, 2);
  assert.equal(seen[0][marker], undefined);
  assert.equal(seen[1][marker], undefined);
});

test("the private scalar success path is accepted as the concrete Program", () => {
  const program = makeProgram();
  const api = createTsrxCoreCompat({
    parseSync() {
      return program;
    },
  });
  assert.equal(api.parseModule("export const answer = 42"), program);
});

test("parseModule sends generated JavaScript through OXC's TypeScript-compatible semantic-error lane", () => {
  const calls = [];
  const program = makeProgram();
  const api = createTsrxCoreCompat({
    parseSync(...args) {
      calls.push(args);
      return { program, comments: [], errors: [] };
    },
  });

  assert.equal(api.parseModule("export { missing };", "generated.js"), program);
  assert.deepEqual(calls, [
    [
      "generated.js",
      "export { missing };",
      {
        lang: "ts",
        sourceType: "module",
        astType: "js",
        preserveParens: false,
        showSemanticErrors: true,
      },
    ],
  ]);
});

test("the successful no-comment hot path skips source scanning and comment materialization", () => {
  const program = makeProgram();
  let commentsGetterReads = 0;
  const source = {
    get length() {
      throw new Error("the compatibility layer scanned the source");
    },
    charCodeAt() {
      throw new Error("the compatibility layer scanned the source");
    },
  };
  const api = createTsrxCoreCompat({
    parseSync(_filename, observedSource) {
      assert.equal(observedSource, source);
      return {
        program,
        errors: [],
        get comments() {
          commentsGetterReads += 1;
          throw new Error("the compatibility layer materialized comments");
        },
      };
    },
  });

  assert.equal(api.parseModule(source), program);
  assert.equal(commentsGetterReads, 0);
});

test("an Unexpected-token rejection retries unquoted TSRX submodule sources exactly once", () => {
  const source =
    "module server { export const value = 1; }; import { value } from server; export { value } from server;";
  const firstIdentifierStart = source.indexOf("server;", source.indexOf("import"));
  const secondIdentifierStart = source.indexOf("server;", firstIdentifierStart + 1);
  const calls = [];
  const api = createTsrxCoreCompat({
    parseSync(filename, observedSource, options) {
      calls.push({ filename, source: observedSource, options });
      if (calls.length === 1) {
        return {
          program: null,
          comments: [],
          errors: [
            {
              severity: "Error",
              message: "Unexpected token",
              labels: [
                { start: firstIdentifierStart, end: firstIdentifierStart + "server".length },
              ],
            },
          ],
        };
      }

      assert.equal(observedSource.length, source.length);
      assert.doesNotMatch(observedSource, /from\s+server/u);
      const placeholderSpans = [...observedSource.matchAll(/from(?<literal>"[^"]*")/gu)].map(
        (match) => ({
          start: match.index + "from".length,
          end: match.index + "from".length + match.groups.literal.length,
          raw: match.groups.literal,
        }),
      );
      assert.equal(placeholderSpans.length, 2);
      return {
        errors: [],
        comments: [],
        program: {
          type: "Program",
          start: 0,
          end: source.length,
          sourceType: "module",
          hashbang: null,
          body: [
            { type: "TSModuleDeclaration", start: 0, end: 43 },
            {
              type: "ImportDeclaration",
              start: 45,
              end: 73,
              source: {
                type: "Literal",
                value: "placeholder",
                raw: placeholderSpans[0].raw,
                start: placeholderSpans[0].start,
                end: placeholderSpans[0].end,
              },
            },
            {
              type: "ExportNamedDeclaration",
              start: 74,
              end: source.length,
              source: {
                type: "Literal",
                value: "placeholder",
                raw: placeholderSpans[1].raw,
                start: placeholderSpans[1].start,
                end: placeholderSpans[1].end,
              },
            },
          ],
        },
      };
    },
  });

  const program = api.parseModule(source, "src/submodule.tsrx");

  assert.equal(calls.length, 2);
  assert.equal(program.body[0].type, "TSModuleDeclaration");
  assert.deepEqual(program.body[1].source, {
    type: "Identifier",
    name: "server",
    start: firstIdentifierStart,
    end: firstIdentifierStart + "server".length,
  });
  assert.deepEqual(program.body[2].source, {
    type: "Identifier",
    name: "server",
    start: secondIdentifierStart,
    end: secondIdentifierStart + "server".length,
  });
});

test("parseModule preserves an explicit filename and appends compatible comments", () => {
  const program = makeProgram();
  const comments = [];
  const api = createTsrxCoreCompat({
    parseSync(filename) {
      assert.equal(filename, "src/View.tsrx");
      return {
        program,
        errors: [],
        comments: [{ type: "Line", value: " hello", start: 0, end: 8 }],
      };
    },
  });

  assert.equal(
    api.parseModule("// hello\nconst x = 1", "src/View.tsrx", { comments }),
    program,
  );
  assert.deepEqual(comments, [
    {
      type: "Line",
      value: " hello",
      start: 0,
      end: 8,
      loc: {
        start: { line: 1, column: 0 },
        end: { line: 1, column: 8 },
      },
      context: null,
    },
  ]);
});

test("strict parsing throws the first returned diagnostic as a SyntaxError-like CompileError", () => {
  const api = createTsrxCoreCompat({
    parseSync() {
      return { program: makeProgram(), comments: [], errors: [makeNativeError()] };
    },
  });

  assert.throws(
    () => api.parseModule("const value = ;", "src/Broken.tsrx"),
    (error) => {
      assert.equal(error.name, "SyntaxError");
      assert.equal(error.message, "Unexpected token");
      assert.equal(error.code, undefined);
      assert.equal(error.pos, 13);
      assert.equal(error.raisedAt, 14);
      assert.equal(error.end, 14);
      assert.equal(error.fileName, "src/Broken.tsrx");
      assert.equal(error.type, "fatal");
      assert.deepEqual(error.loc, {
        start: { line: 1, column: 13 },
        end: { line: 1, column: 14 },
      });
      return true;
    },
  );
});

test("a stray greater-than diagnostic anchors on the extra token like @tsrx/core", () => {
  const source = "export function Controls() @{ <button>Save</button>> }";
  const closingBrace = source.lastIndexOf("}");
  const extraGreaterThan = source.indexOf("</button>>") + "</button>".length;
  const api = createTsrxCoreCompat({
    parseSync() {
      return {
        program: null,
        comments: [],
        errors: [
          {
            severity: "Error",
            message: "Unexpected token",
            labels: [{ start: closingBrace, end: closingBrace + 1 }],
          },
        ],
      };
    },
  });

  assert.throws(
    () => api.parseModule(source, "src/Controls.tsrx"),
    (error) => {
      assert.equal(error.pos, extraGreaterThan);
      assert.equal(error.raisedAt, extraGreaterThan + 1);
      assert.deepEqual(error.loc, {
        start: { line: 1, column: extraGreaterThan },
        end: { line: 1, column: extraGreaterThan + 1 },
      });
      return true;
    },
  );
});

test("dynamic-tag call diagnostics use @tsrx/core 0.1.32's user-facing message", () => {
  const candidateMessage =
    "TSRX dynamic tag 0 at source byte 56 must be an identifier, member, static string, or runtime expression without calls, construction, spreads, concatenation, interpolation, objects, or arrays";
  const referenceMessage =
    "Dynamic element names must be an identifier, member expression, static string, or runtime expression; calls, spreads, string concatenation, string interpolation, and static null, undefined, boolean, number, object, and array literals are not valid tag names.";
  const api = createTsrxCoreCompat({
    parseSync() {
      return {
        program: null,
        comments: [],
        errors: [makeNativeError(candidateMessage)],
      };
    },
  });

  assert.throws(
    () => api.parseModule("export function App() @{ <{tag()}/> }", "DynamicTagCall.tsrx"),
    (error) => error instanceof SyntaxError && error.message === referenceMessage,
  );
});

for (const mode of ["collect", "loose"]) {
  test(`${mode} mode appends returned diagnostics and returns an available program`, () => {
    const program = makeProgram();
    const errors = [];
    const api = createTsrxCoreCompat({
      parseSync() {
        return { program, comments: [], errors: [makeNativeError("Recoverable syntax error")] };
      },
    });

    const result = api.parseModule("const value = ;", "src/Broken.tsrx", {
      [mode]: true,
      errors,
    });

    assert.equal(result, program);
    assert.equal(errors.length, 1);
    assert.equal(errors[0].name, "SyntaxError");
    assert.equal(errors[0].message, "Recoverable syntax error");
    assert.equal(errors[0].type, "usage");
    assert.equal(errors[0].fileName, "src/Broken.tsrx");
  });
}

test("a missing native program is never exposed as a successful parse", () => {
  const errors = [];
  const api = createTsrxCoreCompat({
    parseSync() {
      return { program: null, comments: [], errors: [makeNativeError()] };
    },
  });

  assert.throws(
    () => api.parseModule("const value = ;", "src/Broken.tsrx", { collect: true, errors }),
    (error) => error instanceof SyntaxError && error.type === "fatal",
  );
  assert.equal(errors.length, 1);
  assert.equal(errors[0].type, "usage");
});

test("operational parser failures retain their identity", () => {
  const operational = Object.assign(new Error("native package is not installed"), {
    name: "ParserOperationalError",
    code: "ERR_TSRX_NATIVE_NOT_INSTALLED",
  });
  const api = createTsrxCoreCompat({
    parseSync() {
      throw operational;
    },
  });

  assert.throws(() => api.parseModule("export {}"), (error) => error === operational);
});

test("event helpers match @tsrx/core 0.1.32 edge behavior", () => {
  const api = createTsrxCoreCompat({ parseSync() {} });

  for (const name of ["onClick", "onA", "on-click", "on_click", "on$", "on1click"]) {
    assert.equal(api.isEventAttribute(name), true, name);
  }
  for (const name of ["", "on", "onclick", "class", "ONClick"]) {
    assert.equal(api.isEventAttribute(name), false, name);
  }

  assert.deepEqual(
    Object.fromEntries(
      [
        "onClick",
        "onClickCapture",
        "onCapture",
        "onClickcapture",
        "onGotPointerCapture",
        "onLostPointerCapture",
        "onGOTPOINTERCAPTURE",
        "onLOSTPOINTERCAPTURE",
        "onFooCaptureCapture",
      ].map((name) => [name, api.normalizeEventName(name)]),
    ),
    {
      onClick: "click",
      onClickCapture: "click",
      onCapture: "",
      onClickcapture: "clickcapture",
      onGotPointerCapture: "gotpointercapture",
      onLostPointerCapture: "lostpointercapture",
      onGOTPOINTERCAPTURE: "gotpointercapture",
      onLOSTPOINTERCAPTURE: "lostpointercapture",
      onFooCaptureCapture: "foocapture",
    },
  );
});

test("the package is a publishable, dependency-light compatibility facade", async () => {
  const manifestUrl = new URL("../../packages/tsrx-core-compat/package.json", import.meta.url);
  const manifest = JSON.parse(await readFile(manifestUrl, "utf8"));

  assert.equal(manifest.name, "@oxc-tsrx/tsrx-core-compat");
  assert.equal(manifest.version, "0.1.2");
  assert.equal(manifest.type, "module");
  assert.equal(manifest.sideEffects, false);
  assert.equal(manifest.dependencies["oxc-tsrx"], "0.1.2");
  assert.equal(manifest.dependencies["@tsrx/core"], undefined);
  assert.deepEqual(Object.keys(manifest.exports).sort(), [
    ".",
    "./package.json",
    "./types",
    "./types/estree",
  ]);
});
