function canonicalValue(value) {
  if (Array.isArray(value)) return value.map(canonicalValue);
  if (value === null || typeof value !== "object") return value;
  const output = {};
  for (const key of Object.keys(value).sort()) {
    if (value[key] !== undefined) output[key] = canonicalValue(value[key]);
  }
  return output;
}

export function canonicalJson(value) {
  return JSON.stringify(canonicalValue(value));
}

function rounded(value) {
  return Number(value.toFixed(3));
}

export function summarizeSamples(samples, fileCount) {
  if (!Array.isArray(samples) || samples.length === 0) {
    throw new TypeError("benchmark samples must be a non-empty array");
  }
  if (!Number.isInteger(fileCount) || fileCount < 1) {
    throw new TypeError("benchmark file count must be a positive integer");
  }
  const elapsedMs = samples.reduce((total, sample) => total + sample, 0);
  const sorted = [...samples].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  const median =
    sorted.length % 2 === 0
      ? (sorted[middle - 1] + sorted[middle]) / 2
      : sorted[middle];
  const parses = samples.length * fileCount;
  return {
    iterations: samples.length,
    parses,
    elapsedMs: rounded(elapsedMs),
    meanWholeCorpusMs: rounded(elapsedMs / samples.length),
    medianWholeCorpusMs: rounded(median),
    filesPerSecond: rounded(parses / (elapsedMs / 1_000)),
  };
}

export function semanticProjection(result, diagnostics) {
  return {
    diagnostics,
    componentEdges: result.semanticGraph.componentEdges,
    styleScopes: result.publicRenderPlan.styleScopes,
    protocolView: result.protocolView,
    payloadScripts: result.payloadScripts,
    publicRenderModule: result.publicRenderModule,
    symbolModules: result.symbolModules,
    runtimeDemandMap: result.runtimeDemandMap,
  };
}
