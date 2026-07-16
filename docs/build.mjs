// Static docs site generator: markdown in docs/ -> HTML in docs/dist/.
// Plain JavaScript, no framework. Run with: node docs/build.mjs
import { cp, lstat, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Marked } from 'marked'
import {
  benchmarkHeadings,
  benchmarksSectionsHtml,
  comparativeChartHtml,
  homeBenchmarksHtml,
  latestReportDates,
} from './benchmarks-data.mjs'
import { getDocsHighlighter, highlightWith } from './highlight.mjs'
import config from './site.config.mjs'

const docsDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(docsDir, '..')
const defaultOutDir = path.join(docsDir, 'dist')
const outDir = process.env.OXC_TSRX_DOCS_OUT_DIR
  ? path.resolve(process.env.OXC_TSRX_DOCS_OUT_DIR)
  : defaultOutDir
const base = config.base ?? '/'

const withBase = (href) =>
  href.startsWith('/') ? base.replace(/\/$/, '') + href : href

const escapeHtml = (text) =>
  String(text)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')

function isSameOrAncestor(candidate, target) {
  const relative = path.relative(candidate, target)
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative))
}

async function resolveThroughExistingAncestor(candidate) {
  let existing = candidate
  for (;;) {
    try {
      const canonical = await realpath(existing)
      return path.resolve(canonical, path.relative(existing, candidate))
    } catch (error) {
      if (error.code !== 'ENOENT') throw error
      const parent = path.dirname(existing)
      if (parent === existing) throw error
      existing = parent
    }
  }
}

async function validateOutputDirectory() {
  if (
    outDir === path.parse(outDir).root ||
    isSameOrAncestor(outDir, repoRoot) ||
    outDir === docsDir
  ) {
    throw new Error(`refusing destructive docs output directory: ${outDir}`)
  }
  let metadata = null
  try {
    metadata = await lstat(outDir)
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  if (metadata?.isSymbolicLink()) throw new Error(`refusing symlink docs output directory: ${outDir}`)
  if (outDir === defaultOutDir) return

  const tempRoot = path.resolve(tmpdir())
  const relative = path.relative(tempRoot, outDir)
  if (
    relative.startsWith('..') ||
    path.isAbsolute(relative) ||
    !path.basename(outDir).startsWith('oxc-tsrx-')
  ) {
    throw new Error(`custom docs output must be an oxc-tsrx-* directory under ${tempRoot}`)
  }
  const canonicalTempRoot = await realpath(tempRoot)
  const canonicalOutDir = await resolveThroughExistingAncestor(outDir)
  const expectedCanonicalOutDir = path.resolve(canonicalTempRoot, relative)
  const canonicalRelative = path.relative(canonicalTempRoot, canonicalOutDir)
  if (
    canonicalOutDir !== expectedCanonicalOutDir ||
    canonicalRelative.startsWith('..') ||
    path.isAbsolute(canonicalRelative)
  ) {
    throw new Error(
      `custom docs output resolves outside the trusted temporary directory: ${outDir}`,
    )
  }
  if (metadata && !metadata.isDirectory()) throw new Error(`docs output is not a directory: ${outDir}`)
  if (metadata && (await readdir(outDir)).length > 0) {
    throw new Error(`refusing nonempty custom docs output directory: ${outDir}`)
  }
}

function slugify(text) {
  return text
    .toLowerCase()
    .replace(/<[^>]*>/g, '')
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .trim()
    .replace(/\s+/g, '-')
}

function makeSlugger() {
  const seen = new Map()
  return (text) => {
    const slug = slugify(text) || 'section'
    const count = seen.get(slug) ?? 0
    seen.set(slug, count + 1)
    return count === 0 ? slug : `${slug}-${count}`
  }
}

function parseFrontmatter(source) {
  const match = /^---\n([\s\S]*?)\n---\n?/.exec(source)
  if (!match) return { data: {}, body: source }
  const data = {}
  for (const line of match[1].split('\n')) {
    const separator = line.indexOf(':')
    if (separator > 0) {
      data[line.slice(0, separator).trim()] = line.slice(separator + 1).trim()
    }
  }
  return { data, body: source.slice(match[0].length) }
}

const highlighter = await getDocsHighlighter()
const highlightHtml = (code, lang) => highlightWith(highlighter, code, lang)

// Read the pinned OXC revision from the adapter crate so the footer badge can
// never disagree with the code.
const adapterSource = await readFile(
  path.join(docsDir, '..', 'crates', 'oxc_adapter', 'src', 'lib.rs'),
  'utf8',
)
const oxcRevision = /OXC_REVISION: &str = "([0-9a-f]{40})"/.exec(adapterSource)?.[1] ?? 'unknown'
const reportDate = (await latestReportDates()).toISOString().slice(0, 10)
const footerBadge = `<p class="footer-badge">Pinned OXC <code>${oxcRevision.slice(0, 12)}</code> · latest benchmark report ${reportDate}</p>`

// Editor-style hover docs for TSRX constructs in code examples, mirroring the
// quick-info experience of the Markless VS Code extension.
const TSRX_DOCS = {
  '@{': ['Function body', 'A function body that mixes statements and JSX.'],
  '@if': ['Conditional', 'Renders when the condition is truthy.'],
  '@else': ['Fallback', 'Runs when @if fails; chain with @else if.'],
  '@for': ['Loop', 'Renders once per item. Supports index i and key expr.'],
  '@empty': ['Loop fallback', 'Renders when the loop has nothing to show.'],
  '@switch': ['Match', 'Picks the @case that matches a value.'],
  '@case': ['Branch', 'Written as @case value: { … }.'],
  '@default': ['Fallback', 'Renders when no @case matches.'],
  '@try': ['Async boundary', 'Awaited content, with loading and error branches.'],
  '@pending': ['Loading', 'Shown while @try content loads.'],
  '@catch': ['Error', 'Handles @try failures; (error; reset) supported.'],
}

function addTsrxHovers(html) {
  // Chained form first: shiki tokenizes "@else if" as "@else" + " if".
  html = html.replace(
    /(>)(@else)(<\/span>)(<span[^>]*>)(\s*if\b)/g,
    (match, open, token, close, nextOpen, ifWord) =>
      `${open}<span class="tsrx-hover" tabindex="0" role="img" aria-label="@else if: Chained conditional. Tests another condition when the previous branch failed." data-doc-title="@else if · Chained conditional" data-doc="Tests another condition when the previous branch failed.">${token}</span>${close}${nextOpen}${ifWord}`,
  )
  return html.replace(
    /(>)(@(?:\{|if|else|for|empty|switch|case|default|try|pending|catch))(<\/span>)/g,
    (match, open, token, close) => {
      const doc = TSRX_DOCS[token]
      if (!doc) return match
      return `${open}<span class="tsrx-hover" tabindex="0" role="img" aria-label="${escapeHtml(
        `${token}: ${doc[0]}. ${doc[1]}`,
      )}" data-doc-title="${escapeHtml(`${token} · ${doc[0]}`)}" data-doc="${escapeHtml(doc[1])}">${token}</span>${close}`
    },
  )
}

// Site-wide hover glossary: first prose occurrence of each technical term on
// a page gets an editor-style tooltip, so jargon is explained where it sits.
const GLOSSARY = {
  p95: ['p95', '95 of 100 runs were at least this fast. A worst-realistic-case number, not an average.'],
  throughput: ['throughput', 'How much source code is processed per second. Higher is better.'],
  'MiB/s': ['MiB/s', 'Mebibytes of source code processed per second.'],
  projection: ['projection', 'The temporary in-memory TSX copy of your TSRX file that OXC actually reads.'],
  lift: ['lift', 'Converting the formatted TSX copy back into your TSRX syntax.'],
  'fail-closed': ['fail-closed', 'Unsupported input produces a clear error instead of a silently wrong result.'],
}

function addGlossary(article) {
  const seen = new Set()
  const wrapText = (text) => {
    let out = text
    for (const [term, [title, doc]] of Object.entries(GLOSSARY)) {
      if (seen.has(term)) continue
      const pattern = new RegExp(`(^|[\\s(])(${term.replace('/', '\\/').replace('-', '\\-')})(?=[\\s.,;:)]|$)`)
      if (!pattern.test(out)) continue
      seen.add(term)
      out = out.replace(
        pattern,
        (m, pre, word) =>
          `${pre}<span class="tsrx-hover" tabindex="0" role="img" aria-label="${escapeHtml(`${title}: ${doc}`)}" data-doc-title="${escapeHtml(title)}" data-doc="${escapeHtml(doc)}">${word}</span>`,
      )
    }
    return out
  }
  return article.replace(/(<(?:p|li)>)([\s\S]*?)(<\/(?:p|li)>)/g, (match, open, body, close) => {
    const parts = body.split(/(<[^>]+>)/)
    for (let i = 0; i < parts.length; i += 2) parts[i] = wrapText(parts[i])
    return open + parts.join('') + close
  })
}

function createMarked(slugger, headings) {
  const marked = new Marked()
  marked.use({
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens)
        const id = slugger(html)
        headings.push({ depth, id, text: html.replace(/<[^>]*>/g, '') })
        const anchor =
          depth > 1
            ? `<a class="header-anchor" href="#${id}" aria-label="Permalink to “${escapeHtml(
                html.replace(/<[^>]*>/g, ''),
              )}”">#</a>`
            : ''
        return `<h${depth} id="${id}">${html}${anchor}</h${depth}>\n`
      },
      code({ text, lang }) {
        const language = (lang || 'text').split(/\s/)[0]
        const tryButton =
          language === 'tsrx'
            ? `<button type="button" class="try-button" data-code="${escapeHtml(text)}">Try in playground</button>`
            : ''
        let body = highlightHtml(text, language)
        if (language === 'tsrx') body = addTsrxHovers(body)
        return `<div class="code-block" data-lang="${escapeHtml(language)}">${body}${tryButton}</div>\n`
      },
      link({ href, tokens }) {
        const text = this.parser.parseInline(tokens)
        if (/^https?:\/\//.test(href)) {
          return `<a href="${href}" target="_blank" rel="noreferrer">${text}<span class="visually-hidden"> (opens in new tab)</span></a>`
        }
        return `<a href="${withBase(href)}">${text}</a>`
      },
    },
  })
  return marked
}

// Collect page text per heading section for the client-side search index.
function extractSections(marked, body, page) {
  const slugger = makeSlugger()
  const sections = []
  let current = { title: page.title, anchor: '', parts: [] }
  const flush = () => {
    const text = current.parts.join(' ').replace(/\s+/g, ' ').trim()
    if (text || current.anchor) sections.push({ ...current, text })
  }
  const plain = (raw) =>
    raw
      .replace(/```[^\n]*\n?/g, ' ')
      .replace(/[`*_#>|]/g, ' ')
      .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/<[^>]*>/g, ' ')
  for (const token of marked.lexer(body)) {
    if (token.type === 'heading' && token.depth <= 3) {
      const id = slugger(token.text)
      if (token.depth === 1) {
        current.title = token.text
        continue
      }
      flush()
      current = { title: token.text, anchor: id, parts: [] }
    } else if (token.raw) {
      current.parts.push(plain(token.raw))
    }
  }
  flush()
  return sections.map((section, index) => ({
    id: `${page.link}#${index}`,
    page: page.title,
    group: page.group,
    title: section.title,
    href: withBase(page.link) + (section.anchor ? `#${section.anchor}` : ''),
    text: section.text.slice(0, 1200),
  }))
}

const navHtml = config.nav
  .map((item) => `<li><a href="${withBase(item.link)}">${item.text}</a></li>`)
  .join('')

function sidebarHtml(activeLink) {
  return config.sidebar
    .map(
      (group) => `
      <section class="sidebar-group">
        <h2 class="sidebar-group-title">${group.text}</h2>
        <ul>
          ${group.items
            .map(
              (item) =>
                `<li><a href="${withBase(item.link)}"${
                  item.link === activeLink ? ' aria-current="page"' : ''
                }>${item.text}</a></li>`,
            )
            .join('\n')}
        </ul>
      </section>`,
    )
    .join('\n')
}

function outlineHtml(headings) {
  const items = headings.filter((h) => h.depth === 2 || h.depth === 3)
  if (items.length === 0) return ''
  return `
    <nav class="outline" aria-labelledby="outline-title">
      <p class="outline-title" id="outline-title">On this page</p>
      <ul>
        ${items
          .map(
            (h) =>
              `<li class="outline-depth-${h.depth}"><a href="#${h.id}">${escapeHtml(h.text)}</a></li>`,
          )
          .join('\n')}
      </ul>
    </nav>`
}

function prevNextHtml(pageIndex, flat) {
  const prev = flat[pageIndex - 1]
  const next = flat[pageIndex + 1]
  if (!prev && !next) return ''
  const cell = (item, kind, label) =>
    item
      ? `<div class="pager-link ${kind}"><a href="${withBase(item.link)}"><span class="pager-label">${label}</span><span class="pager-title">${item.text}</span></a></div>`
      : '<div></div>'
  return `<nav class="pager" aria-label="Previous and next page">
    ${cell(prev, 'prev', 'Previous page')}
    ${cell(next, 'next', 'Next page')}
  </nav>`
}

const themeInit = `(() => {
  try {
    const stored = localStorage.getItem('oxc-tsrx-theme')
    const dark = stored ? stored === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches
    document.documentElement.classList.toggle('dark', dark)
  } catch {}
})()`

const favicon = withBase('/assets/logo.svg')
const socialImage = `${config.origin}${withBase('/assets/social-card.png')}`

function canonicalUrl(pathname) {
  if (pathname === '/') return `${config.origin}${base}`
  return `${config.origin}${withBase(pathname)}`
}

const searchDialog = `
<dialog id="search-dialog" class="search-dialog" aria-label="Search documentation">
  <div class="search-panel">
    <form class="search-form" role="search" onsubmit="return false">
      <label class="visually-hidden" for="search-input">Search documentation</label>
      <input id="search-input" type="search" role="combobox" aria-expanded="false"
        aria-controls="search-results" aria-autocomplete="list" autocomplete="off"
        placeholder="Search docs" />
      <button type="button" class="search-close" id="search-close">Esc</button>
    </form>
    <ul id="search-results" class="search-results" role="listbox" aria-label="Search results"></ul>
    <p id="search-status" class="search-status" role="status"></p>
  </div>
</dialog>`

function headerHtml() {
  return `
<header class="navbar">
  <div class="navbar-inner">
    <button id="menu-toggle" class="menu-toggle" aria-label="Navigation menu" aria-expanded="false" aria-controls="sidebar">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 6h18M3 12h18M3 18h18"/></svg>
    </button>
    <a class="site-title" href="${withBase('/index.html')}"><img class="site-logo" src="${withBase('/assets/logo.svg')}" alt="" width="26" height="26" />${config.title}</a>
    <div class="navbar-spacer"></div>
    <button id="search-button" class="search-button" aria-label="Search documentation">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/></svg>
      <span class="search-button-text">Search</span>
      <kbd class="search-key" aria-hidden="true">⌘K</kbd>
    </button>
    <nav class="top-nav" aria-label="Main navigation"><ul>${navHtml}</ul></nav>
    <button id="theme-toggle" class="theme-toggle" aria-label="Toggle dark theme" aria-pressed="false">
      <svg class="icon-sun" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.9 4.9l1.4 1.4m11.4 11.4 1.4 1.4M2 12h2m16 0h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>
      <svg class="icon-moon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8z"/></svg>
    </button>
  </div>
</header>`
}

function pageShell({ title, description, pathname, bodyClass, header, main }) {
  const fullTitle = title === config.title ? title : `${title} | ${config.title}`
  const summary = description || config.description
  const canonical = canonicalUrl(pathname)
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${escapeHtml(fullTitle)}</title>
<meta name="description" content="${escapeHtml(summary)}" />
<link rel="canonical" href="${canonical}" />
<meta property="og:type" content="website" />
<meta property="og:site_name" content="${escapeHtml(config.title)}" />
<meta property="og:url" content="${canonical}" />
<meta property="og:title" content="${escapeHtml(fullTitle)}" />
<meta property="og:description" content="${escapeHtml(summary)}" />
<meta property="og:image" content="${socialImage}" />
<meta property="og:image:width" content="1200" />
<meta property="og:image:height" content="630" />
<meta property="og:image:alt" content="OXC for TSRX" />
<meta name="twitter:card" content="summary_large_image" />
<meta name="twitter:title" content="${escapeHtml(fullTitle)}" />
<meta name="twitter:description" content="${escapeHtml(summary)}" />
<meta name="twitter:image" content="${socialImage}" />
<meta name="twitter:image:alt" content="OXC for TSRX" />
<meta name="color-scheme" content="light dark" />
<link rel="icon" href="${favicon}" />
<link rel="preload" href="${withBase('/assets/fonts/space-grotesk-latin.woff2')}" as="font" type="font/woff2" crossorigin />
<link rel="preload" href="${withBase('/assets/fonts/inter-latin.woff2')}" as="font" type="font/woff2" crossorigin />
<script>${themeInit}</script>
<link rel="stylesheet" href="${withBase('/assets/style.css')}" />
</head>
<body class="${bodyClass}">
<a class="skip-link" href="#main-content">Skip to content</a>
${header}
${main}
${searchDialog}
<div id="route-announcer" class="visually-hidden" aria-live="polite"></div>
<script type="module" src="${withBase('/assets/app.js')}"></script>
</body>
</html>
`
}

// Static projection explorer: authored TSRX, the projected TSX OXC actually
// sees, and the diagnostics mapped back. Data is precomputed by
// docs/generate-projection.mjs from the real tsrx_syntax crate and lint CLI.
async function projectionExplorerHtml() {
  let example
  try {
    example = JSON.parse(await readFile(path.join(docsDir, 'projection-example.json'), 'utf8'))
  } catch {
    return '<p><em>Projection example data is not generated yet. Run <code>node docs/generate-projection.mjs</code>.</em></p>'
  }
  const diagnosticsList = example.diagnostics
    .map(
      (diagnostic) =>
        `<li><code>${escapeHtml(diagnostic.code)}</code> (${escapeHtml(diagnostic.severity)}): ${escapeHtml(
          diagnostic.message,
        )} <span class="explorer-span">at authored bytes ${diagnostic.labels[0].span.offset}–${
          diagnostic.labels[0].span.offset + diagnostic.labels[0].span.length
        }</span></li>`,
    )
    .join('\n')
  const tabs = [
    { id: 'authored', label: '1 · Your TSRX', body: addTsrxHovers(highlightHtml(example.tsrx, 'tsrx')) },
    { id: 'projected', label: '2 · Projected TSX (what OXC sees)', body: highlightHtml(example.projected, 'tsx') },
    {
      id: 'mapped',
      label: '3 · Diagnostics mapped back',
      body: `<div class="explorer-diagnostics"><p>Real <code>oxc-tsrx</code> output for this file. Every position points at the authored TSRX on tab 1, never at the scaffolding on tab 2:</p><ul>${diagnosticsList}</ul></div>`,
    },
  ]
  return `<div class="explorer" data-explorer>
  <div class="explorer-tabs" role="tablist" aria-label="Projection stages">
    ${tabs
      .map(
        (tab, index) =>
          `<button type="button" role="tab" id="explorer-tab-${tab.id}" aria-controls="explorer-panel-${tab.id}" aria-selected="${index === 0}" ${index === 0 ? '' : 'tabindex="-1"'}>${tab.label}</button>`,
      )
      .join('\n')}
  </div>
  ${tabs
    .map(
      (tab, index) =>
        `<div class="explorer-panel" role="tabpanel" id="explorer-panel-${tab.id}" aria-labelledby="explorer-tab-${tab.id}" ${index === 0 ? '' : 'hidden'}>${tab.body}</div>`,
    )
    .join('\n')}
</div>`
}

// "Copy page ▾" split menu: copy/view as Markdown, open in AI assistants.
function pageMenuHtml(link) {
  const mdHref = withBase(link.replace(/\.html$/, '.md'))
  const absoluteMd = `${config.origin}${mdHref}`
  const prompt = encodeURIComponent(
    `Read ${absoluteMd} so I can ask questions about this OXC for TSRX documentation page.`,
  )
  return `<div class="page-menu" data-page-menu>
      <button type="button" class="copy-md-button page-menu-main" data-md-href="${mdHref}">Copy page</button>
      <button type="button" class="page-menu-toggle" aria-haspopup="menu" aria-expanded="false" aria-label="More ways to use this page">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>
      </button>
      <ul class="page-menu-list" role="menu" hidden>
        <li role="none"><button type="button" role="menuitem" class="copy-md-button" data-md-href="${mdHref}">Copy page as Markdown</button></li>
        <li role="none"><a role="menuitem" href="${mdHref}" target="_blank" rel="noreferrer">View as plain Markdown</a></li>
        <li role="none"><a role="menuitem" href="https://chatgpt.com/?hints=search&q=${prompt}" target="_blank" rel="noreferrer">Open in ChatGPT</a></li>
        <li role="none"><a role="menuitem" href="https://claude.ai/new?q=${prompt}" target="_blank" rel="noreferrer">Open in Claude</a></li>
      </ul>
    </div>`
}


// Interactive pipeline: the ASCII diagrams replaced by a step-through where
// every stage shows the real artifact from docs/projection-example.json.
async function loadProjectionExample() {
  try {
    return JSON.parse(await readFile(path.join(docsDir, 'projection-example.json'), 'utf8'))
  } catch {
    return null
  }
}

async function pipelineHtml(kind) {
  const example = await loadProjectionExample()
  if (!example) return '<p><em>Run <code>node docs/generate-projection.mjs</code> to generate the pipeline example.</em></p>'
  const tokensTable = `<div class="table-wrap"><table class="pipeline-tokens"><thead><tr><th>Token</th><th>Bytes</th></tr></thead><tbody>${(
    example.tokens ?? []
  )
    .map(
      (token) =>
        `<tr><td><code>${escapeHtml(token.kind)}</code></td><td>${token.start}–${token.end}</td></tr>`,
    )
    .join('')}</tbody></table></div>`
  const diagnosticsList = `<ul>${example.diagnostics
    .map(
      (diagnostic) =>
        `<li><code>${escapeHtml(diagnostic.code)}</code>: ${escapeHtml(diagnostic.message)} <span class="explorer-span">at your bytes ${diagnostic.labels[0].span.offset}–${diagnostic.labels[0].span.offset + diagnostic.labels[0].span.length}</span></li>`,
    )
    .join('')}</ul>`
  const stages = [
    {
      id: 'source',
      label: 'Your TSRX',
      text: 'The file you wrote, byte for byte. Nothing is changed on disk at any point.',
      body: addTsrxHovers(highlightHtml(example.tsrx, 'tsrx')),
    },
    {
      id: 'scan',
      label: 'Scan',
      text: 'One byte-oriented pass finds every TSRX control token and records its exact position. This is the real overlay for the file above:',
      body: tokensTable,
    },
    {
      id: 'project',
      label: 'Projection',
      text: `The TSRX syntax becomes ${kind === 'format' ? 'formatting-safe markers' : 'valid TSX placeholders'} in an in-memory copy; your code is copied verbatim. This is the actual projection:`,
      body: highlightHtml(example.projected, 'tsx'),
    },
    kind === 'format'
      ? {
          id: 'engine',
          label: 'Oxfmt formats',
          text: 'Canonical Oxfmt parses and lays out that copy exactly once. The markers are designed to survive formatting so nothing about your control flow is lost.',
          body: '',
        }
      : {
          id: 'engine',
          label: 'OXC lints',
          text: 'The real OXC parser and linter run on that copy, exactly once. These are the genuine diagnostics for this file:',
          body: diagnosticsList,
        },
    kind === 'format'
      ? {
          id: 'back',
          label: 'Lift back',
          text: 'A checked single pass converts the formatted copy back into TSRX: markers become @-controls again, raw <code>&lt;style&gt;</code> bytes are restored from your original, and the result must re-scan to the same structure before anything is written.',
          body: '',
        }
      : {
          id: 'back',
          label: 'Mapped back',
          text: 'Every diagnostic is translated to your original bytes. Anything that would point at placeholder code is dropped instead of shown, so errors always land on code you wrote.',
          body: '',
        },
  ]
  const prefix = `pl-${kind}`
  return `<div class="explorer pipeline" data-explorer>
  <div class="explorer-tabs pipeline-tabs" role="tablist" aria-label="${kind === 'format' ? 'Format' : 'Lint'} pipeline stages">
    ${stages
      .map(
        (stage, index) =>
          `<button type="button" role="tab" id="${prefix}-tab-${stage.id}" aria-controls="${prefix}-panel-${stage.id}" aria-selected="${index === 0}" ${index === 0 ? '' : 'tabindex="-1"'}><span class="pipeline-step" aria-hidden="true">${index + 1}</span>${stage.label}</button>`,
      )
      .join('\n')}
  </div>
  ${stages
    .map(
      (stage, index) =>
        `<div class="explorer-panel" role="tabpanel" id="${prefix}-panel-${stage.id}" aria-labelledby="${prefix}-tab-${stage.id}" ${index === 0 ? '' : 'hidden'}><p class="pipeline-text">${stage.text}</p>${stage.body}</div>`,
    )
    .join('\n')}
</div>`
}

function renderDocPage({ page, article, headings, pageIndex, flat }) {
  const main = `
<div class="layout">
  <div id="sidebar-backdrop" class="sidebar-backdrop" hidden></div>
  <aside id="sidebar" class="sidebar" aria-label="Sidebar">
    <nav aria-label="Docs navigation">
      ${sidebarHtml(page.link)}
    </nav>
  </aside>
  <main id="main-content" class="content">
    <div class="doc-toolbar">${pageMenuHtml(page.link)}</div>
    <article class="doc">
      ${article}
    </article>
    ${prevNextHtml(pageIndex, flat)}
  </main>
  <aside class="aside" aria-label="Page outline">${outlineHtml(headings)}</aside>
</div>`
  return pageShell({
    title: page.title,
    description: page.description,
    pathname: page.link,
    bodyClass: 'doc-page',
    header: headerHtml(),
    main,
  })
}

// Real TSRX hero snippet, highlighted with the actual TSRX grammar. This is
// oxc-tsrx-fmt's converged output, so the default demo state is format-clean.
const heroCode = `export function TaskList({ tasks }: Props) @{
  const pending = tasks.filter((task) => !task.done);

  <section class="tasks">
    @if (pending.length > 0) {
      @for (const task of pending; key task.id) {
        <TaskRow task={task} />;
      } @empty {
        <AllDone />;
      }
    } @else {
      <SignIn />;
    }
    <style>
      .tasks { display: grid; gap: 0.5rem; }
    </style>
  </section>;
}`

async function renderHomePage({ description }) {
  const hero = config.hero
  const main = `
<main id="main-content" class="home">
  <section class="hero">
    <img class="hero-logo" src="${withBase('/assets/logo.svg')}" alt="" width="64" height="64" />
    <h1 class="hero-name">${hero.name}</h1>
    <p class="hero-text">${hero.text}</p>
    <p class="hero-tagline">${hero.tagline}</p>
    <div class="hero-actions">
      ${hero.actions
        .map(
          (action) =>
            `<a class="action action-${action.theme}" href="${withBase(action.link)}">${action.text}</a>`,
        )
        .join('\n')}
    </div>
  </section>
  <section class="band" aria-label="TSRX example">
    <div class="code-panel" id="hero-demo">
      <div class="code-panel-bar">
        <span class="code-panel-dots" aria-hidden="true"><i></i><i></i><i></i></span>
        <span class="code-panel-file">src/TaskList.tsrx</span>
        <span class="code-panel-hint" id="demo-hint"></span>
        <span class="code-panel-actions" id="demo-actions" hidden>
          <button type="button" class="demo-button" id="pg-scenario-clean">Clean</button>
          <button type="button" class="demo-button" id="pg-scenario-lint">Lint findings</button>
          <button type="button" class="demo-button" id="pg-scenario-messy">Messy → Format</button>
          <button type="button" class="demo-button" id="pg-scenario-types">Type error</button>
        </span>
      </div>
      <div class="code-panel-editor" id="demo-editor">
        ${highlightHtml(heroCode, 'tsrx')}
      </div>
      <div class="code-panel-status">
        <span id="demo-status" aria-live="polite">pre-generated example · static preview</span>
        <span id="demo-meta">native lint and format run only on the local development server</span>
      </div>
    </div>
  </section>
  <section class="home-bench" aria-label="Headline performance">
    <h2>Fast, and gated on it</h2>
    <p>Real numbers from the aggregate-selected committed benchmark reports, rebuilt into this page at build time. The first chart shows matched absolute CLI timings and names its frozen ratio gates in each tooltip. The second chart shows selected release gates: there, each bar shows how close the result sits to its frozen budget (the dashed line), and the release fails if a result crosses it. Hover or focus a row for its exact denominator and measurement boundary. These project-specific results are tied to the recorded host, corpus, and output boundary.</p>
    <h3 class="home-bench-sub">Matched 1,000-file TSX CLI comparison</h3>
    ${await comparativeChartHtml()}
    <h3 class="home-bench-sub">Selected frozen release gates</h3>
    ${await homeBenchmarksHtml()}
    <p class="home-bench-link"><a href="${withBase('/reference/benchmarks.html')}">See every gate and report →</a></p>
  </section>
  <section class="features" aria-label="Feature highlights">
    <ul class="features-grid">
      ${config.features
        .map(
          (feature) => `
      <li class="feature">
        <span class="feature-icon">${feature.icon}</span>
        <h2 class="feature-title">${feature.title}</h2>
        <p class="feature-details">${feature.details}</p>
      </li>`,
        )
        .join('\n')}
    </ul>
  </section>
  <footer class="home-footer">
    <p class="footer-disclaimer">${config.footer.disclaimer}</p>
    <p><a href="${config.repository}" target="_blank" rel="noreferrer">Source, issues, and releases on GitHub<span class="visually-hidden"> (opens in new tab)</span></a></p>
    <p><a href="https://www.npmjs.com/package/oxlint-tsrx" target="_blank" rel="noreferrer"><code>oxlint-tsrx</code> on npm<span class="visually-hidden"> (opens in new tab)</span></a> · <a href="https://www.npmjs.com/package/oxfmt-tsrx" target="_blank" rel="noreferrer"><code>oxfmt-tsrx</code> on npm<span class="visually-hidden"> (opens in new tab)</span></a> · <a href="https://www.npmjs.com/package/@oxc-tsrx/runtime" target="_blank" rel="noreferrer"><code>@oxc-tsrx/runtime</code> on npm<span class="visually-hidden"> (opens in new tab)</span></a></p>
    ${footerBadge}
    <p>${config.footer.copyright}</p>
  </footer>
</main>`
  return pageShell({
    title: config.title,
    description,
    pathname: '/',
    bodyClass: 'home-page',
    header: headerHtml(),
    main,
  })
}

// Self-contained playground default: declares its own types and components
// so the opt-in type-check lane starts clean instead of full of TS errors.
const playgroundCode = `type Task = { id: string; label: string; done: boolean };

function TaskRow({ task }: { task: Task }) @{
  <li>{task.label}</li>;
}

export function TaskList({ tasks }: { tasks: Task[] }) @{
  const pending = tasks.filter((task) => !task.done);

  <section class="tasks">
    @if (pending.length > 0) {
      <ul>
        @for (const task of pending; key task.id) {
          <TaskRow task={task} />;
        }
      </ul>;
    } @else {
      <p>All done!</p>;
    }
  </section>;
}`

function renderPlaygroundPage() {
  const main = `
<main id="main-content" class="home playground-page">
  <section class="pg" aria-label="Playground">
    <header class="pg-topbar">
      <h1 class="pg-title">TSRX Playground</h1>
      <p class="pg-tagline">Real <code>oxc-tsrx</code> · <code>oxc-tsrx-fmt</code>. <span id="pg-mode-note">On the published static preview, output is pre-generated; run the local development server for live editing.</span></p>
    </header>
    <div class="pg-toolbar pg-examples-bar" id="pg-side" hidden>
      <div class="pg-examples" role="group" aria-label="Clickable examples">
        <span class="pg-examples-label">Examples</span>
        <button type="button" class="demo-button" id="pg-scenario-clean">Clean</button>
        <button type="button" class="demo-button" id="pg-scenario-lint">Lint findings</button>
        <button type="button" class="demo-button" id="pg-scenario-messy">Messy → Format</button>
        <button type="button" class="demo-button" id="pg-scenario-types">Type error</button>
        <button type="button" class="demo-button" id="pg-scenario-silence">Silence a rule</button>
        <button type="button" class="demo-button" id="pg-scenario-config">Custom config</button>
      </div>
      <p class="pg-note" id="pg-scenario-note">Each example edits the file and runs the real engines; the note here explains which flags were used.</p>
    </div>
    <div class="pg-panes">
      <div class="code-panel pg-panel" id="hero-demo">
        <div class="code-panel-bar">
          <span class="code-panel-dots" aria-hidden="true"><i></i><i></i><i></i></span>
          <span class="code-panel-file">playground.tsrx</span>
          <span class="code-panel-hint" id="demo-hint"></span>
          <span class="code-panel-actions" id="demo-actions" hidden>
            <button type="button" class="demo-button" id="demo-share">Share</button>
            <button type="button" class="demo-button" id="demo-format">Format</button>
            <button type="button" class="demo-button" id="demo-reset">Reset</button>
          </span>
        </div>
        <div class="code-panel-editor" id="demo-editor">
          ${highlightHtml(playgroundCode, 'tsrx')}
        </div>
        <div class="code-panel-status">
          <span id="demo-status" aria-live="polite">pre-generated example · static preview</span>
          <span id="demo-meta">native lint and format run only on the local development server</span>
        </div>
      </div>
      <div class="code-panel pg-output" id="pg-output" data-explorer hidden>
        <div class="code-panel-bar pg-output-tabs" role="tablist" aria-label="Engine output">
          <span class="pg-pane-label" aria-hidden="true">Engine output</span>
          <button type="button" role="tab" id="pg-tab-projected" aria-controls="pg-panel-projected" aria-selected="true">Projected TSX</button>
          <button type="button" role="tab" id="pg-tab-structure" aria-controls="pg-panel-structure" aria-selected="false" tabindex="-1">Structure</button>
          <button type="button" role="tab" id="pg-tab-diagnostics" aria-controls="pg-panel-diagnostics" aria-selected="false" tabindex="-1">Diagnostics</button>
          <button type="button" role="tab" id="pg-tab-formatted" aria-controls="pg-panel-formatted" aria-selected="false" tabindex="-1">Formatted</button>
        </div>
        <div class="pg-output-body">
          <div role="tabpanel" id="pg-panel-projected" aria-labelledby="pg-tab-projected"><p class="pg-note pg-output-note">The legal TSX the real projection engine hands to OXC: your bytes copied verbatim, TSRX controls replaced by scaffold markers.</p><div class="pg-output-code" id="pg-projected"></div></div>
          <div role="tabpanel" id="pg-panel-structure" aria-labelledby="pg-tab-structure" hidden><p class="pg-note pg-output-note">The structural overlay from the byte-oriented scan: every TSRX control token and its byte span.</p><div class="pg-output-code" id="pg-structure"></div></div>
          <div role="tabpanel" id="pg-panel-diagnostics" aria-labelledby="pg-tab-diagnostics" hidden><p class="pg-note pg-output-note">Raw <code>oxc-tsrx --format=json</code> diagnostics, mapped to your original bytes.</p><div class="pg-output-code" id="pg-diagnostics"></div></div>
          <div role="tabpanel" id="pg-panel-formatted" aria-labelledby="pg-tab-formatted" hidden><p class="pg-note pg-output-note">What <code>oxc-tsrx-fmt</code> produces for the current source.</p><div class="pg-output-code" id="pg-formatted"></div></div>
        </div>
        <div class="code-panel-status"><span id="pg-output-status">output follows the editor as you type</span></div>
      </div>
    </div>
  </section>
</main>`
  return pageShell({
    title: 'Playground',
    description:
      'A static TSRX preview that becomes an interactive native lint and format playground on the localhost development server.',
    pathname: '/playground.html',
    bodyClass: 'home-page',
    header: headerHtml(),
    main,
  })
}

async function build() {
  await validateOutputDirectory()
  await rm(outDir, { recursive: true, force: true })
  await mkdir(outDir, { recursive: true })

  const flat = config.sidebar.flatMap((group) =>
    group.items.map((item) => ({ ...item, group: group.text })),
  )
  const searchDocs = []

  const markdownPages = []
  for (const [pageIndex, item] of flat.entries()) {
    const sourcePath = path.join(docsDir, item.link.replace(/^\//, '').replace(/\.html$/, '.md'))
    const source = await readFile(sourcePath, 'utf8')
    const { data, body } = parseFrontmatter(source)
    const page = {
      link: item.link,
      group: item.group,
      title: data.title || item.text,
      description: data.description || '',
    }
    const headings = []
    const marked = createMarked(makeSlugger(), headings)
    let article = marked.parse(body)
    article = article
      .replaceAll('<table>', '<div class="table-wrap"><table>')
      .replaceAll('</table>', '</table></div>')
    if (article.includes('<!-- benchmarks:auto -->')) {
      article = article.replace('<!-- benchmarks:auto -->', await benchmarksSectionsHtml())
      const anchor = headings.findIndex((heading) => heading.text === 'Measurement hygiene')
      headings.splice(anchor === -1 ? headings.length : anchor, 0, ...benchmarkHeadings)
    }
    if (article.includes('<!-- projection-explorer -->')) {
      article = article.replace('<!-- projection-explorer -->', await projectionExplorerHtml())
    }
    if (article.includes('<!-- pipeline:lint -->')) {
      article = article.replace('<!-- pipeline:lint -->', await pipelineHtml('lint'))
    }
    if (article.includes('<!-- pipeline:format -->')) {
      article = article.replace('<!-- pipeline:format -->', await pipelineHtml('format'))
    }
    article = addGlossary(article)
    searchDocs.push(...extractSections(new Marked(), body, page))
    const html = renderDocPage({ page, article, headings, pageIndex, flat })
    const outPath = path.join(outDir, item.link.replace(/^\//, ''))
    await mkdir(path.dirname(outPath), { recursive: true })
    await writeFile(outPath, html)
    // Raw markdown twin for the copy-as-Markdown button and llms-full.txt.
    await writeFile(outPath.replace(/\.html$/, '.md'), body)
    markdownPages.push({ ...page, body })
  }

  // llms.txt index and llms-full.txt corpus (https://llmstxt.org).
  const llmsIndex = [
    `# ${config.title}`,
    '',
    `> ${config.description}`,
    '',
    ...config.sidebar.map((group) =>
      [
        `## ${group.text}`,
        '',
        ...group.items.map((item) => {
          const page = markdownPages.find((candidate) => candidate.link === item.link)
          return `- [${item.text}](${withBase(item.link.replace(/\.html$/, '.md'))})${page?.description ? `: ${page.description}` : ''}`
        }),
        '',
      ].join('\n'),
    ),
  ].join('\n')
  await writeFile(path.join(outDir, 'llms.txt'), llmsIndex)
  await writeFile(
    path.join(outDir, 'llms-full.txt'),
    markdownPages
      .map((page) => `<!-- ${page.group} / ${page.title} (${page.link}) -->\n\n${page.body}`)
      .join('\n\n---\n\n'),
  )

  const home = parseFrontmatter(await readFile(path.join(docsDir, 'index.md'), 'utf8'))
  await writeFile(
    path.join(outDir, 'index.html'),
    await renderHomePage({ description: home.data.description }),
  )
  await writeFile(path.join(outDir, 'playground.html'), renderPlaygroundPage())

  await cp(path.join(docsDir, 'assets'), path.join(outDir, 'assets'), { recursive: true })
  await rm(path.join(outDir, 'assets', 'logos'), { recursive: true, force: true })
  await cp(
    path.join(docsDir, '..', 'node_modules', 'minisearch', 'dist', 'es'),
    path.join(outDir, 'assets', 'minisearch'),
    { recursive: true },
  )
  await writeFile(path.join(outDir, 'search-index.json'), JSON.stringify(searchDocs))
  await writeFile(
    path.join(outDir, 'demo-capabilities.json'),
    `${JSON.stringify({ ok: true, mode: 'static', native: false, typeAware: false, projection: false })}\n`,
  )

  const publicPaths = ['/', ...flat.map(({ link }) => link), '/playground.html']
  await writeFile(
    path.join(outDir, 'robots.txt'),
    `User-agent: *\nAllow: ${base}\nSitemap: ${canonicalUrl('/sitemap.xml')}\n`,
  )
  await writeFile(
    path.join(outDir, 'sitemap.xml'),
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${publicPaths
      .map((pathname) => `  <url><loc>${canonicalUrl(pathname)}</loc></url>`)
      .join('\n')}\n</urlset>\n`,
  )

  console.log(`built ${publicPaths.length} pages, ${searchDocs.length} search sections -> ${outDir}`)
}

await build()
