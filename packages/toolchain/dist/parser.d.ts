import type { Program } from "@oxc-project/types";

export * from "@oxc-project/types";

export type Language = "js" | "jsx" | "ts" | "tsx" | "dts" | "tsrx";
export type SourceType = "script" | "module" | "commonjs" | "unambiguous";
export type AstType = "js" | "ts";

export interface ParserOptions {
  lang?: Language;
  sourceType?: SourceType | undefined;
  astType?: AstType;
  range?: boolean;
  preserveParens?: boolean;
  showSemanticErrors?: boolean;
  recovery?: "none" | "editor";
}

export interface Span {
  start: number;
  end: number;
}

export interface ValueSpan extends Span {
  value: string;
}

export interface Comment extends Span {
  type: "Line" | "Block";
  value: string;
}

export interface ErrorLabel extends Span {
  message: string | null;
}

export declare const enum Severity {
  Error = "Error",
  Warning = "Warning",
  Advice = "Advice",
}

export interface OxcError {
  severity: Severity;
  message: string;
  labels: ErrorLabel[];
  helpMessage: string | null;
  codeframe: string | null;
}

export declare const enum ImportNameKind {
  Name = "Name",
  NamespaceObject = "NamespaceObject",
  Default = "Default",
}

export interface ImportName {
  kind: ImportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}

export interface StaticImportEntry {
  importName: ImportName;
  localName: ValueSpan;
  isType: boolean;
}

export interface StaticImport extends Span {
  moduleRequest: ValueSpan;
  entries: StaticImportEntry[];
}

export declare const enum ExportImportNameKind {
  Name = "Name",
  All = "All",
  AllButDefault = "AllButDefault",
  None = "None",
}

export declare const enum ExportExportNameKind {
  Name = "Name",
  Default = "Default",
  None = "None",
}

export declare const enum ExportLocalNameKind {
  Name = "Name",
  Default = "Default",
  None = "None",
}

export interface ExportImportName {
  kind: ExportImportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}

export interface ExportExportName {
  kind: ExportExportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}

export interface ExportLocalName {
  kind: ExportLocalNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}

export interface StaticExportEntry extends Span {
  moduleRequest: ValueSpan | null;
  importName: ExportImportName;
  exportName: ExportExportName;
  localName: ExportLocalName;
  isType: boolean;
}

export interface StaticExport extends Span {
  entries: StaticExportEntry[];
}

export interface DynamicImport extends Span {
  moduleRequest: Span;
}

export interface EcmaScriptModule {
  hasModuleSyntax: boolean;
  staticImports: StaticImport[];
  staticExports: StaticExport[];
  dynamicImports: DynamicImport[];
  importMetas: Span[];
}

export interface ParseResult {
  readonly program: Program | null;
  readonly module: EcmaScriptModule | null;
  readonly comments: Comment[];
  readonly errors: OxcError[];
}

/**
 * What the installed native build supports. Every flag here answers a question
 * about *this build*, never about the language or the AST in general: the
 * canonical build and the compatibility build set them differently, and each
 * flag is the discriminator for one option the two builds disagree about.
 */
export interface ParserCapabilities {
  readonly apiVersion: 1;
  readonly languages: readonly Language[];
  readonly target: string;
  readonly nodeApi: number;
  readonly nodeEngine: "^20.19.0 || >=22.12.0";
  readonly oxcRevision: string;
  readonly lazy: true;
  readonly async: true;
  /**
   * Whether `recovery: "editor"` is available. `false` means that one option is
   * refused with `ERR_TSRX_CAPABILITY_RECOVERY`; a file that does not parse
   * still reports every error it found in `result.errors`, as it always does.
   */
  readonly editorRecovery: boolean;
  /**
   * Whether the CSS inside a `<style>` element is materialized down to its
   * individual components. **`false` does not mean there is no CSS tree.** The
   * canonical build always gives a `<style>` element a `css` string and a
   * `StyleSheet` child holding a `Rule` per rule, each with a `SelectorList`
   * prelude of `ComplexSelector` nodes and a `Block`, all carrying exact
   * offsets relative to the CSS payload. That is enough to scope selectors, and
   * consumers do it today.
   *
   * What `false` withholds is the level below that: the `ComplexSelector` and
   * `Block` children arrays are empty, so there are no compound-selector parts
   * and no per-declaration nodes. Read those out of the `source`/`css` text, or
   * hand it to a CSS parser. The compatibility build reports `true` and
   * materializes them.
   */
  readonly cssMaterialization: false;
  readonly rawTransfer: boolean;
}

export type ParserOperationalErrorCode =
  | "ERR_TSRX_INVALID_ARGUMENT"
  | "ERR_TSRX_UNSUPPORTED_TARGET"
  | "ERR_TSRX_NATIVE_NOT_INSTALLED"
  | "ERR_TSRX_NATIVE_INTEGRITY"
  | "ERR_TSRX_NATIVE_VERSION"
  | "ERR_TSRX_CAPABILITY_RECOVERY"
  | "ERR_TSRX_CAPABILITY_CSS"
  | "ERR_TSRX_CAPABILITY_RAW_TRANSFER"
  | "ERR_TSRX_RESOURCE_EXHAUSTED"
  | "ERR_TSRX_CANCELLED";

export class ParserOperationalError extends Error {
  readonly name: "ParserOperationalError";
  readonly code: ParserOperationalErrorCode;
  constructor(code: ParserOperationalErrorCode, message: string, options?: ErrorOptions);
}

export const capabilities: Readonly<ParserCapabilities>;

export function parseSync(
  filename: string,
  sourceText: string,
  options?: Readonly<ParserOptions> | undefined | null,
): ParseResult;

export function parse(
  filename: string,
  sourceText: string,
  options?: Readonly<ParserOptions> | undefined | null,
): Promise<ParseResult>;
