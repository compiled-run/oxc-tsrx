import {
  CAPABILITY_NAMES,
  discoverProviders,
  findProjectRoot,
  hasProviderErrors,
} from "./provider-resolve.js";

/**
 * Read-only audit of the language providers a project root activates.
 *
 * The report never writes a file, never mutates node_modules, and never runs a
 * dependency: it is the auditable counterpart to a host quietly executing a
 * provider it discovered.
 */
export async function collectProviderReport(options = {}) {
  const root = await findProjectRoot(options.projectRoot ?? process.cwd());
  const index = await discoverProviders({ root, throwOnError: false });
  return {
    schemaVersion: 1,
    ok: !hasProviderErrors(index),
    root: index.root,
    providers: index.providers,
    extensions: index.extensions,
    diagnostics: index.diagnostics,
  };
}

function capabilityLabel(capability) {
  if (capability.kind === "bin") return `bin ${capability.bin} -> ${capability.path}`;
  return `module ${capability.specifier}${capability.path === null ? " (unresolved)" : ""}`;
}

export function formatProviderReport(report) {
  const lines = [`project root: ${report.root}`];
  if (report.providers.length === 0) {
    lines.push("no language providers are declared by the direct dependencies");
  }
  for (const provider of report.providers) {
    lines.push(
      `${provider.name}${provider.version === null ? "" : `@${provider.version}`} (provider ${provider.id}, protocol ${provider.protocol})`,
    );
    for (const language of provider.languages) {
      lines.push(`  language ${language.id}: ${language.extensions.join(" ")}`);
      for (const capability of CAPABILITY_NAMES) {
        const target = language.capabilities[capability];
        if (target === undefined) continue;
        lines.push(`    ${capability}: ${capabilityLabel(target)}`);
      }
    }
  }
  const routed = Object.keys(report.extensions).sort();
  if (routed.length > 0) {
    lines.push(
      `routed extensions: ${routed
        .map((extension) => `${extension} -> ${report.extensions[extension].package}`)
        .join(", ")}`,
    );
  }
  for (const diagnostic of report.diagnostics) {
    lines.push(`${diagnostic.severity}: ${diagnostic.message}`);
  }
  return `${lines.join("\n")}\n`;
}
