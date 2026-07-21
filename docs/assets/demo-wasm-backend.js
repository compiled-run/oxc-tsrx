// In-browser demo backend: routes the playground's API calls to the real
// lint, format, and projection engines compiled to WebAssembly (NAPI-RS,
// wasm32-wasip1-threads). Response shapes mirror docs/serve.mjs exactly so
// playground.js cannot tell the difference between this and the native server.

let enginePromise = null
const loadEngine = () =>
  (enginePromise ??= import(new URL('./demo-wasm/engine.js', import.meta.url)))

const parseJsonRequest = (body) => {
  if (typeof body === 'string' && body.trimStart().startsWith('{')) {
    try {
      const parsed = JSON.parse(body)
      if (typeof parsed.source === 'string') return parsed
    } catch {}
  }
  return { source: body }
}

async function lint(body) {
  const request = parseJsonRequest(body)
  const engine = await loadEngine()
  const startedAt = performance.now()
  const report = JSON.parse(engine.lint(request.source, JSON.stringify(request)))
  const elapsedMs = Math.round((performance.now() - startedAt) * 10) / 10
  if (report.error) return { error: report.error }
  return {
    diagnostics: (report.diagnostics ?? []).map((diagnostic) => ({
      rule: diagnostic.rule,
      code: diagnostic.code,
      severity: diagnostic.severity,
      message: diagnostic.message,
      labels: diagnostic.labels ?? [],
    })),
    parseCount: report.oxcTsrx?.parseCount ?? null,
    suppressed: report.oxcTsrx?.diagnosticsSuppressed ?? 0,
    ruleCount: report.number_of_rules ?? null,
    typeAware: false,
    elapsedMs,
  }
}

async function format(body) {
  const engine = await loadEngine()
  const startedAt = performance.now()
  const result = JSON.parse(engine.format(body))
  const elapsedMs = Math.round((performance.now() - startedAt) * 10) / 10
  if (result.error) return { error: result.error }
  return { formatted: result.formatted, elapsedMs }
}

async function project(body) {
  const engine = await loadEngine()
  return JSON.parse(engine.project(body, false))
}

export function createWasmBackend(getHighlighter) {
  if (typeof SharedArrayBuffer === 'undefined') {
    throw new Error('the in-browser engine needs cross-origin isolation')
  }
  const highlight = async (body) => {
    let source = body
    let lang = 'tsrx'
    if (typeof body === 'string' && body.trimStart().startsWith('{')) {
      try {
        const parsed = JSON.parse(body)
        if (typeof parsed.source === 'string') {
          source = parsed.source
          if (['tsx', 'tsrx', 'json'].includes(parsed.lang)) lang = parsed.lang
        }
      } catch {}
    }
    const highlighter = await getHighlighter()
    return { html: highlighter ? highlighter.highlight(source, lang) : null }
  }
  return async (endpoint, body) => {
    switch (endpoint) {
      case 'lint':
        return lint(body)
      case 'format':
        return format(body)
      case 'project':
        return project(body)
      case 'highlight':
        return highlight(body)
      case 'complete':
        // No TypeScript service in the browser: an empty list makes the
        // editor fall back to in-file word suggestions, like it does when
        // the grammar is mid-edit.
        return { entries: [] }
      case 'quickinfo':
        return { info: null }
      default:
        return { error: `unsupported in-browser endpoint: ${endpoint}` }
    }
  }
}
