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

export interface ParserCapabilities {
  readonly apiVersion: 1;
  readonly languages: readonly Language[];
  readonly target: string;
  readonly nodeApi: number;
  readonly nodeEngine: "^20.19.0 || >=22.12.0";
  readonly oxcRevision: string;
  readonly lazy: true;
  readonly async: true;
  readonly editorRecovery: boolean;
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
