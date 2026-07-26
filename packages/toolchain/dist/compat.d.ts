export type OxcTsrxCompatibilityState =
  | "missing"
  | "replaceable"
  | "active"
  | "stale"
  | "collision";

export interface OxcTsrxCompatibilitySlot {
  readonly name: "oxc-parser" | "oxlint" | "oxfmt";
  readonly capability: "parser" | "lint" | "format";
  readonly path: string;
  readonly state: OxcTsrxCompatibilityState;
  readonly replacedPackage?: {
    readonly name: string;
    readonly version: string;
  };
}

export interface OxcTsrxCompatibilityOptions {
  readonly projectRoot?: string;
  readonly userAgent?: string;
  readonly dryRun?: boolean;
}

export interface OxcTsrxCompatibilityStatus {
  readonly projectRoot: string;
  readonly packageManager: "npm" | "pnpm" | "yarn" | "bun" | "unknown";
  readonly providerVersion: string;
  readonly selectedFrom: "dependencies" | "devDependencies" | "optionalDependencies";
  readonly slots: readonly OxcTsrxCompatibilitySlot[];
}

export interface OxcTsrxSetupResult extends OxcTsrxCompatibilityStatus {
  readonly action: "preview" | "setup";
  readonly changed: readonly string[];
  readonly unchanged: readonly string[];
}

export interface OxcTsrxRemoveResult extends OxcTsrxCompatibilityStatus {
  readonly action: "preview-remove" | "remove";
  readonly removed: readonly string[];
}

export declare function findProjectRoot(start?: string): Promise<string>;
export declare function detectPackageManager(
  projectRoot: string,
  userAgent?: string,
): Promise<OxcTsrxCompatibilityStatus["packageManager"]>;
export declare function compatibilityStatus(
  options?: OxcTsrxCompatibilityOptions,
): Promise<OxcTsrxCompatibilityStatus>;
export declare function setupCompatibility(
  options?: OxcTsrxCompatibilityOptions,
): Promise<OxcTsrxSetupResult>;
export declare function removeCompatibility(
  options?: OxcTsrxCompatibilityOptions,
): Promise<OxcTsrxRemoveResult>;
