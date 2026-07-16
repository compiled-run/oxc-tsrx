import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { format as upstreamFormat, jsTextToDoc as upstreamJsTextToDoc } from "oxfmt-current";
import { resolveNativeBinary, runCaptured } from "@oxc-tsrx/runtime";

export { defineConfig } from "oxfmt-current";

function isTsrx(fileName) {
  return fileName.split("?")[0].endsWith(".tsrx");
}

function serializeOptions(options) {
  if (!options || Object.keys(options).length === 0) return null;
  let json;
  try {
    json = JSON.stringify(options);
  } catch (error) {
    throw new TypeError(`TSRX formatter options must be JSON-serializable: ${error}`);
  }
  if (json === undefined) throw new TypeError("TSRX formatter options must be JSON-serializable");
  return json;
}

export async function format(fileName, sourceText, options) {
  if (typeof fileName !== "string") throw new TypeError("`fileName` must be a string");
  if (typeof sourceText !== "string") throw new TypeError("`sourceText` must be a string");
  if (!isTsrx(fileName)) return upstreamFormat(fileName, sourceText, options);

  const serialized = serializeOptions(options);
  let directory = null;
  const args = [`--stdin-filepath=${fileName}`];
  try {
    if (serialized !== null) {
      directory = await mkdtemp(join(tmpdir(), "oxfmt-tsrx-api-"));
      const config = join(directory, ".oxfmtrc.json");
      await writeFile(config, serialized);
      args.unshift(`--config=${config}`);
    }
    const result = await runCaptured(resolveNativeBinary("format"), args, { input: sourceText });
    if (result.status !== 0) {
      throw new Error(result.stderr.trim() || `native TSRX formatter exited ${result.status}`);
    }
    return { code: result.stdout, errors: [] };
  } finally {
    if (directory !== null) await rm(directory, { recursive: true, force: true });
  }
}

export async function jsTextToDoc(sourceExt, sourceText, optionsJson, parentContext) {
  if (sourceExt !== "tsrx" && sourceExt !== ".tsrx") {
    return upstreamJsTextToDoc(sourceExt, sourceText, optionsJson, parentContext);
  }
  const options = optionsJson ? JSON.parse(optionsJson) : undefined;
  return format(`snippet.${sourceExt.replace(/^\./, "")}`, sourceText, options);
}
