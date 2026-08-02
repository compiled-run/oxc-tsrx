export type OxcProviderCapabilityName = "parse" | "lint" | "format" | "lsp";

export type OxcProviderDiagnosticCode =
  | "ancestor-provider"
  | "duplicate-id"
  | "extension-conflict"
  | "invalid-capability"
  | "invalid-provider"
  | "reserved-extension"
  | "unreadable-manifest"
  | "unresolved-capability"
  | "unsupported-protocol";

export interface OxcProviderDiagnostic {
  readonly severity: "error" | "warning";
  readonly code: OxcProviderDiagnosticCode;
  readonly message: string;
  readonly packages?: readonly string[];
  readonly extension?: string;
  readonly capability?: OxcProviderCapabilityName;
  readonly protocol?: number;
  readonly id?: string;
  readonly manifest?: string;
  readonly root?: string;
}

export interface OxcProviderBinCapability {
  readonly kind: "bin";
  readonly bin: string;
  readonly path: string;
}

export interface OxcProviderModuleCapability {
  readonly kind: "module";
  readonly subpath: string;
  readonly specifier: string;
  readonly path: string | null;
}

export type OxcProviderCapability = OxcProviderBinCapability | OxcProviderModuleCapability;

export type OxcProviderCapabilities = {
  readonly [key in OxcProviderCapabilityName]?: OxcProviderCapability;
};

export interface OxcProviderLanguage {
  readonly id: string;
  readonly extensions: readonly string[];
  readonly capabilities: OxcProviderCapabilities;
}

export interface OxcProvider {
  readonly name: string;
  readonly version: string | null;
  readonly root: string;
  readonly manifest: string;
  readonly protocol: number;
  readonly id: string;
  readonly languages: readonly OxcProviderLanguage[];
}

export interface OxcProviderExtension {
  readonly extension: string;
  readonly package: string;
  readonly providerId: string;
  readonly providerRoot: string;
  readonly language: string;
  readonly capabilities: OxcProviderCapabilities;
}

export interface OxcProviderIndex {
  readonly root: string;
  readonly providers: readonly OxcProvider[];
  readonly extensions: { readonly [extension: string]: OxcProviderExtension };
  readonly diagnostics: readonly OxcProviderDiagnostic[];
}

/**
 * Issuer-aware module resolution. A Plug'n'Play host passes its PnP API's
 * `resolveRequest`, which already has this exact `(request, issuer)` shape.
 */
export type OxcProviderResolve = (request: string, issuer: string) => string;

/**
 * Manifest reader. It must read through the same layer `resolve` answers from.
 *
 * Under Plug'n'Play there is no `node_modules`: packages stay zipped in
 * `.yarn/cache` and `.pnp.cjs` answers with a path *into* the archive, which an
 * ordinary `fs.readFile` cannot open (it fails with `ENOTDIR`). A host that
 * injects the PnP resolver but keeps reading with an ordinary `fs` resolves
 * every manifest and reads none of them.
 */
export type OxcProviderReadFile = (
  path: string,
  encoding: "utf8",
) => string | Promise<string | Uint8Array>;

/**
 * Host obligation: `resolve` and `readFile` travel together. Injecting one
 * without the other is a host fault, not a protocol option.
 *
 * A Plug'n'Play host must supply both the PnP `resolveRequest` **and** a reader
 * backed by the PnP filesystem layer, which in practice means running under the
 * PnP runtime (`--require .pnp.cjs`, or a `yarn node` launcher) so `fs` is
 * patched to see inside the zip. An injected resolver alone is not enough.
 *
 * A host that gets this wrong is told so. A dependency manifest that resolves
 * and then cannot be read or parsed produces an `unreadable-manifest` warning
 * naming the package and the manifest path, so a Plug'n'Play host with an
 * unpatched reader sees one warning per direct dependency instead of an empty
 * index and silence. A dependency that does not resolve at all stays quiet,
 * because that only means it is not installed.
 */
export interface OxcProviderDiscoveryOptions {
  readonly root?: string;
  readonly resolve?: OxcProviderResolve;
  readonly readFile?: OxcProviderReadFile;
  readonly protocols?: readonly number[];
  readonly throwOnError?: boolean;
  readonly inspectAncestors?: boolean;
}

export type OxcResolvedCapability = OxcProviderCapability & {
  readonly package: string;
  readonly providerId: string;
  readonly providerRoot: string;
  readonly language: string;
  readonly extension: string;
  readonly capability: OxcProviderCapabilityName;
};

export declare const PROTOCOL_VERSION: 1;
export declare const SUPPORTED_PROTOCOLS: readonly number[];
export declare const CAPABILITY_NAMES: readonly OxcProviderCapabilityName[];
export declare const DEPENDENCY_FIELDS: readonly string[];
export declare const RESERVED_EXTENSIONS: readonly string[];

export declare class ProviderProtocolError extends Error {
  readonly diagnostics: readonly OxcProviderDiagnostic[];
  constructor(diagnostics: readonly OxcProviderDiagnostic[]);
}

export declare function dependencyNames(manifest: unknown): string[];
export declare function providerDeclaration(manifest: unknown): Record<string, unknown> | null;
export declare function extensionOf(filePath: string): string | null;
export declare function isReservedExtension(extension: string): boolean;
export declare function findProjectRoot(
  start?: string,
  options?: { readonly readFile?: OxcProviderReadFile },
): Promise<string>;
export declare function discoverProviders(
  options?: OxcProviderDiscoveryOptions,
): Promise<OxcProviderIndex>;
export declare function providerExtensions(index: OxcProviderIndex): string[];
export declare function hasProviderErrors(index: OxcProviderIndex): boolean;
export declare function resolveCapability(
  index: OxcProviderIndex,
  filePath: string,
  capability: OxcProviderCapabilityName,
): OxcResolvedCapability | null;
