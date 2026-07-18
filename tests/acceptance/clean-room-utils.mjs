const MUTABLE_OR_HEAVY_SEGMENTS = new Set(['node_modules', 'target', '.docs-demo-tmp'])
const ROOT_CONTROL_PLANE_SEGMENTS = new Set(['.git', '.fable-codex'])

export function shouldExcludeSourcePath(relative, { excludeReport = false } = {}) {
  const normalized = String(relative).replaceAll('\\', '/').replace(/^\.\//u, '')
  if (!normalized) return false
  const segments = normalized.split('/')
  if (ROOT_CONTROL_PLANE_SEGMENTS.has(segments[0])) return true
  if (
    segments.some(
      (segment) => MUTABLE_OR_HEAVY_SEGMENTS.has(segment) || segment.startsWith('.corpus-'),
    )
  ) {
    return true
  }
  if (normalized === 'docs/dist' || normalized.startsWith('docs/dist/')) return true
  if (
    normalized.startsWith('benchmarks/') &&
    segments.at(-1) === 'latest.json'
  ) {
    return true
  }
  return excludeReport && normalized === 'docs/acceptance/clean-room-report.json'
}

export function assertOwnerSourceUnchanged(initialSourceHash, finalSourceHash) {
  if (initialSourceHash !== finalSourceHash) {
    throw new Error(
      `owner source changed during clean-room verification (${initialSourceHash} -> ${finalSourceHash})`,
    )
  }
}
