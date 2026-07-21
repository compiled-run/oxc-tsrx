import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalJson,
  semanticProjection,
  summarizeSamples,
} from "../../benchmarks/markless-dropin/lib.mjs";

test("benchmark canonical JSON sorts object keys without reordering arrays", () => {
  assert.equal(
    canonicalJson({ z: 1, a: { z: 2, a: 3 }, rows: [{ z: 4, a: 5 }, 6] }),
    '{"a":{"a":3,"z":2},"rows":[{"a":5,"z":4},6],"z":1}',
  );
});

test("benchmark sample summaries report whole-corpus elapsed time and throughput", () => {
  assert.deepEqual(summarizeSamples([10, 20], 5), {
    iterations: 2,
    parses: 10,
    elapsedMs: 30,
    meanWholeCorpusMs: 15,
    medianWholeCorpusMs: 15,
    filesPerSecond: 333.333,
  });
});

test("semantic checksums cover Markless-consumed outputs but ignore internal state-read noise", () => {
  const result = {
    semanticGraph: {
      componentEdges: [{ source: "./Child.tsrx" }],
      stateReads: [{ source: "TypeOnlyFalsePositive" }],
    },
    publicRenderPlan: { styleScopes: [{ id: "scope" }] },
    protocolView: { version: 1 },
    payloadScripts: [{ id: "payload" }],
    publicRenderModule: { csrModuleSource: "export default 1" },
    symbolModules: { modules: [] },
    runtimeDemandMap: { actions: [] },
  };
  assert.deepEqual(semanticProjection(result, [{ code: "diagnostic" }]), {
    diagnostics: [{ code: "diagnostic" }],
    componentEdges: [{ source: "./Child.tsrx" }],
    styleScopes: [{ id: "scope" }],
    protocolView: { version: 1 },
    payloadScripts: [{ id: "payload" }],
    publicRenderModule: { csrModuleSource: "export default 1" },
    symbolModules: { modules: [] },
    runtimeDemandMap: { actions: [] },
  });
  assert.doesNotMatch(canonicalJson(semanticProjection(result, [])), /TypeOnlyFalsePositive/u);
});
