import assert from "node:assert/strict";
import test from "node:test";

import { parseSync } from "../../packages/toolchain/dist/parser.js";
import { createTsrxCoreCompat } from "../../packages/tsrx-core-compat/dist/facade.js";

const api = createTsrxCoreCompat({ parseSync });

const RECOVERABLE_SHAPE_MESSAGE =
  "unsupported TSRX parser shape: failed TSRX result has no authored diagnostic";

function exportedFunction(program, name = "App") {
  const wrapper = program.body.find((node) => node.declaration?.id?.name === name);
  assert.equal(wrapper?.type, "ExportNamedDeclaration");
  return wrapper.declaration;
}

test("generated JavaScript preserves @tsrx/core's undefined-export early error", () => {
  assert.throws(
    () => api.parseModule("export { missing };", "generated.js"),
    (error) =>
      error instanceof SyntaxError &&
      error.message === "Export 'missing' is not defined" &&
      error.fileName === "generated.js" &&
      error.type === "fatal",
  );
});

test("generated JavaScript accepts TypeScript embedded in Markless handler snippets", () => {
  const source =
    "const prompt = document.querySelector<HTMLInputElement>('.prompt'); export { prompt };";
  const program = api.parseModule(source, "generated.js");

  assert.equal(program.type, "Program");
  assert.deepEqual(program.body.map((node) => node.type), [
    "VariableDeclaration",
    "ExportNamedDeclaration",
  ]);
  const initializer = program.body[0].declarations[0].init;
  assert.equal(initializer.type, "CallExpression");
  assert.equal(initializer.callee.type, "MemberExpression");
  assert.equal(initializer.callee.property.name, "querySelector");
});

test("loose parsing recovers from the native no-authored-diagnostic shape failure", () => {
  const source = "export function App() @{ @ }";
  const calls = [];
  const recoveredProgram = {
    type: "Program",
    start: 0,
    end: source.length,
    sourceType: "module",
    hashbang: null,
    body: [],
  };
  const recoveringApi = createTsrxCoreCompat({
    parseSync(_filename, observedSource) {
      calls.push(observedSource);
      if (calls.length === 1) {
        throw Object.assign(new Error(RECOVERABLE_SHAPE_MESSAGE), {
          name: "ParserOperationalError",
          code: "ERR_TSRX_INVALID_ARGUMENT",
        });
      }
      assert.notEqual(observedSource, source);
      return { program: recoveredProgram, comments: [], errors: [] };
    },
  });

  assert.equal(
    recoveringApi.parseModule(source, "shape-failure.tsrx", { loose: true, errors: [] }),
    recoveredProgram,
  );
  assert.equal(calls.length, 2);
});

test("loose recovery flattens adjacent authored render roots into the reference code-block shape", () => {
  const source = "export function App() @{ <div/> <img/> <button/> }";
  const errors = [];
  const program = api.parseModule(source, "adjacent.tsrx", { loose: true, errors });
  const block = exportedFunction(program).body;

  assert.deepEqual(block.body.map((node) => node.type), ["JSXElement", "JSXElement"]);
  assert.equal(block.render.type, "JSXElement");
  assert.equal(block.render.openingElement.name.name, "button");
  assert.deepEqual(errors, []);
});

test("loose recovery keeps a run of incomplete constructs usable without changing strict parsing", () => {
  const source = `export function App({ value, values }) @{
	@
	@if (value) { <span>{value}</span> } @
	@for (const item of values) { <span>{item}</span> } @
	@switch (value) { @case 'x': { <span>x</span> } @ }
	@try { <span>{value}</span> } @
	const expression = value + @;
}
@`;
  const program = api.parseModule(source, "constructs.tsrx", { loose: true, errors: [] });
  const block = exportedFunction(program).body;

  assert.deepEqual(block.body.map((node) => node.type), [
    "JSXIfExpression",
    "JSXForExpression",
    "JSXSwitchExpression",
  ]);
  assert.equal(block.render.type, "JSXTryExpression");
});

test("loose recovery restores an incomplete JSX element for native closing-tag requests", () => {
  const source = "export function App() @{\n\t<div>\n}";
  const program = api.parseModule(source, "closing.tsrx", { loose: true, errors: [] });
  const element = exportedFunction(program).body.render;

  assert.equal(element.type, "JSXElement");
  assert.equal(element.closingElement, null);
  assert.equal(element.unclosed, true);
  assert.equal(element.end, element.openingElement.end);
});

test("loose completion ASTs ignore whitespace between a control sibling and its placeholder", () => {
  const source =
    "export function App({ value }) @{ <div>@if (value) { <span/> } {__markless_at__}<b/></div> }";
  const program = api.parseModule(source, "placeholder-child.tsrx", {
    loose: true,
    errors: [],
  });
  const children = exportedFunction(program).body.render.children;
  const placeholderIndex = children.findIndex(
    (node) => node.type === "JSXExpressionContainer" && node.expression?.name === "__markless_at__",
  );

  assert.ok(placeholderIndex > 0);
  assert.equal(children[placeholderIndex - 1].type, "JSXIfExpression");
});

test("loose completion ASTs can classify a statement immediately after a control root", () => {
  const source =
    "export function App({ value }) @{ @if (value) { <span/> } __markless_at__ }";
  const program = api.parseModule(source, "placeholder-statement.tsrx", {
    loose: true,
    errors: [],
  });
  const block = exportedFunction(program).body;
  const placeholder = block.body.at(-1);

  assert.equal(block.body.at(-2).type, "JSXIfExpression");
  assert.equal(placeholder.type, "ExpressionStatement");
  assert.equal(placeholder.expression.name, "__markless_at__");
  assert.equal(block.render, null);
});

test("loose completion ASTs can classify a statement immediately after a try root", () => {
  const source =
    "export function App({ value }) @{ @try { <span>{value}</span> } @pending {} __markless_at__; }";
  const program = api.parseModule(source, "placeholder-after-try.tsrx", {
    loose: true,
    errors: [],
  });
  const block = exportedFunction(program).body;
  const placeholder = block.body.at(-1);

  assert.equal(block.body.at(-2).type, "JSXTryExpression");
  assert.equal(placeholder.type, "ExpressionStatement");
  assert.equal(placeholder.expression.name, "__markless_at__");
  assert.equal(block.render, null);
});
