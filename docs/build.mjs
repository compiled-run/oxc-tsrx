// Static docs site generator: markdown in docs/ -> HTML in docs/dist/.
// Plain JavaScript, no framework. Run with: node docs/build.mjs
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { cp, lstat, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Marked } from 'marked'
import { build as rolldownBuild } from 'rolldown'
import {
  benchmarkHeadings,
  benchmarksSectionsHtml,
  benchmarksSectionsMarkdown,
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

const diagramCache = new Map()

function decorateDiagramSvg(svg, metadata, figureId) {
  const [, width, height] = svg.match(/viewBox="0 0 ([0-9]+) ([0-9]+)"/) ?? []
  if (!width || !height) throw new Error(`Diagram ${figureId} has no numeric viewBox`)
  let decorated = svg.replace(
    '<svg ',
    `<svg class="diagram-svg" width="${width}" height="${height}" role="group" aria-label="${escapeHtml(
      metadata.title,
    )}" `,
  )
  for (const [nodeId, caption] of Object.entries(metadata.nodes)) {
    const d2Class = Buffer.from(nodeId).toString('base64')
    const marker = `<g class="${d2Class}">`
    const replacement = `<g class="${d2Class} diagram-node" data-diagram-node="${escapeHtml(
      nodeId,
    )}" data-caption="${escapeHtml(caption)}" tabindex="0" role="button" aria-label="${escapeHtml(
      caption,
    )}" aria-pressed="false">`
    if (!decorated.includes(marker)) {
      throw new Error(`Diagram ${figureId} has no rendered node named ${nodeId}`)
    }
    decorated = decorated.replace(marker, replacement)
  }
  return decorated
}

async function diagramHtml(name) {
  if (diagramCache.has(name)) return diagramCache.get(name)
  const sourceDir = path.join(docsDir, 'diagrams')
  const assetDir = path.join(docsDir, 'assets', 'diagrams')
  const metadata = JSON.parse(await readFile(path.join(sourceDir, `${name}.json`), 'utf8'))
  const figureId = `diagram-${name}`
  const svg = decorateDiagramSvg(
    await readFile(path.join(assetDir, `${name}.svg`), 'utf8'),
    metadata,
    figureId,
  )
  const steps = metadata.steps
    ? `<div class="diagram-steps pipeline-tabs" role="group" aria-label="${escapeHtml(
        `${metadata.title} steps`,
      )}">${metadata.steps
        .map(
          (step, index) =>
            `<button type="button" data-diagram-step data-nodes="${escapeHtml(
              JSON.stringify(step.nodes),
            )}"${step.caption ? ` data-caption="${escapeHtml(step.caption)}"` : ''} aria-pressed="false"><span class="pipeline-step">${index + 1}</span>${escapeHtml(
              step.label,
            )}</button>`,
        )
        .join('')}</div>`
    : ''
  const html = `<figure class="diagram" id="${figureId}" aria-labelledby="${figureId}-caption">
${steps}<div class="diagram-caption-strip" aria-live="polite">Select a diagram node to read its explanation.</div>
<div class="diagram-scroll">${svg}</div>
<figcaption id="${figureId}-caption"><strong>${escapeHtml(metadata.title)}.</strong> ${escapeHtml(
    metadata.caption,
  )}</figcaption>
</figure>`
  diagramCache.set(name, html)
  return html
}

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

// Content hash of the shared chrome assets, appended as ?v= to their URLs so
// deployed pages never pair fresh HTML with a stale cached stylesheet.
const assetVersion = createHash('sha256')
  .update(await readFile(path.join(docsDir, 'assets', 'style.css')))
  .update(await readFile(path.join(docsDir, 'assets', 'app.js')))
  .digest('hex')
  .slice(0, 10)

// Read the pinned OXC revision from the adapter crate so the footer badge can
// never disagree with the code.
const adapterSource = await readFile(
  path.join(docsDir, '..', 'crates', 'oxc_adapter', 'src', 'lib.rs'),
  'utf8',
)
const oxcRevision = /OXC_REVISION: &str = "([0-9a-f]{40})"/.exec(adapterSource)?.[1] ?? 'unknown'
const reportDate = (await latestReportDates()).toISOString().slice(0, 10)
const footerBadge = `<p class="footer-badge">Pinned OXC <code>${oxcRevision.slice(0, 12)}</code> · benchmark report ${reportDate} · ${config.footer.copyright}</p>`

// Editor-style hover docs for TSRX constructs in code examples, mirroring the
// quick-info experience of the Markless VS Code extension.
const TSRX_DOCS = {
  '@{': [
    'Statement container',
    'A statement container that allows you to have statements and markup colocated.',
  ],
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
    /(>)([ \t]*)(@else)(<\/span>)(<span[^>]*>)(\s*if\b)/g,
    (match, open, whitespace, token, close, nextOpen, ifWord) =>
      `${open}${whitespace}<span class="tsrx-hover" tabindex="0" role="img" aria-label="@else if: Chained conditional. Tests another condition when the previous branch failed." data-doc-title="@else if · Chained conditional" data-doc="Tests another condition when the previous branch failed.">${token}</span>${close}${nextOpen}${ifWord}`,
  )
  return html.replace(
    /(<span(?! class="tsrx-hover")[^>]*>)([ \t]*)(@(?:\{|if|else|for|empty|switch|case|default|try|pending|catch))(<\/span>)/g,
    (match, open, whitespace, token, close) => {
      const doc = TSRX_DOCS[token]
      if (!doc) return match
      return `${open}${whitespace}<span class="tsrx-hover" tabindex="0" role="img" aria-label="${escapeHtml(
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

const githubIcon =
  '<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.55 0-.28-.01-1.02-.02-2-3.2.7-3.87-1.54-3.87-1.54-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.03 1.75 2.69 1.25 3.34.95.1-.74.4-1.25.73-1.54-2.55-.29-5.23-1.28-5.23-5.68 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.04 0 0 .96-.31 3.16 1.18a11 11 0 0 1 2.88-.39c.98 0 1.96.13 2.88.39 2.19-1.49 3.16-1.18 3.16-1.18.62 1.58.23 2.75.11 3.04.74.81 1.18 1.83 1.18 3.09 0 4.42-2.69 5.39-5.25 5.67.41.36.78 1.06.78 2.14 0 1.54-.02 2.79-.02 3.17 0 .31.21.67.8.55A11.51 11.51 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z"/></svg>'
const navHtml = config.nav
  .map((item) =>
    item.link.startsWith('https://github.com')
      ? `<li><a class="nav-github" href="${item.link}" aria-label="${item.text} repository" title="${item.text}">${githubIcon}</a></li>`
      : `<li><a href="${withBase(item.link)}">${item.text}</a></li>`,
  )
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
  if (pageIndex < 0) return ''
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
    <a class="site-title" href="${withBase('/')}"><img class="site-logo" src="${withBase('/assets/logo.svg')}" alt="" width="26" height="26" />${config.title}</a>
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
<link rel="stylesheet" href="${withBase('/assets/style.css')}?v=${assetVersion}" />
</head>
<body class="${bodyClass}">
<a class="skip-link" href="#main-content">Skip to content</a>
${header}
${main}
${searchDialog}
<div id="route-announcer" class="visually-hidden" aria-live="polite"></div>
<script type="module" src="${withBase('/assets/app.js')}?v=${assetVersion}"></script>
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

function transcriptOutputHtml(output) {
  const lines = output.split('\n')
  return lines
    .map((line, index) => {
      if (index === lines.length - 1 && line === '') return ''
      const severity = /:\d+:\d+: warning\b/.test(line)
        ? ' gs-terminal-line-warning'
        : /:\d+:\d+: error\b/.test(line)
          ? ' gs-terminal-line-error'
          : ''
      return `<span class="gs-terminal-line gs-terminal-output${severity}">${escapeHtml(line)}</span>${index < lines.length - 1 ? '\n' : ''}`
    })
    .join('')
}

const terminalDemoDefaultCaption =
  'This output was captured from the real native binaries at build time, so it matches what they actually returned.'

function terminalDemoMarkdown(example, generator = 'docs/generate-projection.mjs') {
  if (!example?.transcript?.length) {
    return `_Run \`node ${generator}\` to generate the terminal walkthrough._`
  }
  const transcript = example.transcript
    .flatMap((entry) => [
      ...(entry.comment ? [`# ${entry.comment}`] : []),
      `$ ${entry.command}`,
      entry.output.trimEnd(),
      '',
    ])
    .join('\n')
    .trimEnd()
  return [
    example.caption ?? terminalDemoDefaultCaption,
    '',
    '```text',
    transcript,
    '```',
  ].join('\n')
}

function terminalDemoHtml(example, generator = 'docs/generate-projection.mjs') {
  if (!example?.transcript?.length) {
    return `<p><em>Run <code>node ${generator}</code> to generate the terminal walkthrough.</em></p>`
  }
  // One block per command, separated by a blank line so the sequence of
  // steps stays visually distinct.
  const transcript = example.transcript
    .map((entry) => {
      const parts = []
      if (entry.comment) {
        parts.push(
          `<span class="gs-terminal-line gs-terminal-comment"># ${escapeHtml(entry.comment)}</span>`,
        )
      }
      parts.push(
        `<span class="gs-terminal-line gs-terminal-command">${escapeHtml(entry.command)}</span>`,
      )
      const output = transcriptOutputHtml(entry.output)
      if (output) parts.push(output)
      return parts.join('\n')
    })
    .join('\n\n')
  // Unique per page so multiple walkthrough regions satisfy landmark-unique.
  const regionLabel = `Recorded output of ${example.transcript[0].command.split('\n')[0]}`
  return `<figure class="gs-terminal" data-terminal-demo>
  <div class="gs-terminal-titlebar">
    <span class="gs-terminal-title">See it run</span>
    <button type="button" data-terminal-play aria-label="Play terminal walkthrough">Play</button>
  </div>
  <pre class="gs-terminal-transcript" role="region" aria-label="${escapeHtml(regionLabel)}" tabindex="0">${transcript}</pre>
  <figcaption>${escapeHtml(example.caption ?? terminalDemoDefaultCaption)}</figcaption>
</figure>`
}

// ---------- transplant matrix filter (Upstreaming to OXC) ----------
// <!-- matrix-filter --> before a table adds classification badges to each
// row and a chip bar that app.js turns into live row filtering. Without JS
// the badges still render and every row stays visible.
const MATRIX_CLASSIFICATIONS = [
  { slug: 'reuse', chip: 'Direct reuse', match: 'Direct reuse' },
  { slug: 'adapt', chip: 'Adapt or replace', match: 'Adapt or replace' },
  { slug: 'glue', chip: 'Product glue', match: 'Standalone product glue' },
  { slug: 'redesign', chip: 'Upstream-only redesign', match: 'Upstream-only redesign' },
]

function matrixFilterHtml(article) {
  const marker = '<!-- matrix-filter -->'
  const markerIndex = article.indexOf(marker)
  const start = article.indexOf('<div class="table-wrap">', markerIndex)
  const end = article.indexOf('</table></div>', start)
  if (markerIndex === -1 || start === -1 || end === -1) {
    throw new Error('matrix-filter marker found without a following table')
  }
  const counts = new Map(MATRIX_CLASSIFICATIONS.map((entry) => [entry.slug, 0]))
  const table = article
    .slice(start, end)
    .replace(/<tr>([\s\S]*?)<\/tr>/g, (row, cells) => {
      const entry = MATRIX_CLASSIFICATIONS.find((candidate) =>
        cells.includes(`<strong>${candidate.match}</strong>`),
      )
      if (!entry) return row
      counts.set(entry.slug, counts.get(entry.slug) + 1)
      return `<tr data-classification="${entry.slug}">${cells.replace(
        `<strong>${entry.match}</strong>`,
        `<span class="matrix-badge matrix-badge-${entry.slug}">${entry.match}</span>`,
      )}</tr>`
    })
  const total = [...counts.values()].reduce((sum, count) => sum + count, 0)
  const chips = [
    `<button type="button" data-matrix-chip="all" aria-pressed="true">All <span class="matrix-count">${total}</span></button>`,
    ...MATRIX_CLASSIFICATIONS.map(
      (entry) =>
        `<button type="button" data-matrix-chip="${entry.slug}" aria-pressed="false"><span class="matrix-badge matrix-badge-${entry.slug}" aria-hidden="true"></span>${entry.chip} <span class="matrix-count">${counts.get(entry.slug)}</span></button>`,
    ),
  ].join('\n    ')
  const replacement = `<div class="matrix-filter" data-matrix-filter>
  <div class="matrix-chips" role="group" aria-label="Filter the transplant matrix by classification">
    ${chips}
  </div>
  <p class="matrix-status" aria-live="polite" data-matrix-status></p>
  ${table}</table></div>
</div>`
  return article.slice(0, markerIndex) + article.slice(markerIndex + marker.length, start) + replacement + article.slice(end + '</table></div>'.length)
}

// ---------- review route checklist (Upstreaming to OXC) ----------
// <!-- review-route --> before an ordered list turns each step into a
// checklist item with its stated reading time, and app.js keeps a running
// "minutes left" total. Without JS the checkboxes still work, the total is
// simply static.
const REVIEW_MINUTE_WORDS = { five: 5, ten: 10, fifteen: 15, twenty: 20 }

function reviewRouteHtml(article) {
  const marker = '<!-- review-route -->'
  const markerIndex = article.indexOf(marker)
  const start = article.indexOf('<ol>', markerIndex)
  const end = article.indexOf('</ol>', start)
  if (markerIndex === -1 || start === -1 || end === -1) {
    throw new Error('review-route marker found without a following ordered list')
  }
  let totalMinutes = 0
  let stepIndex = 0
  const items = article
    .slice(start + '<ol>'.length, end)
    .replace(/<li>([\s\S]*?)<\/li>/g, (item, body) => {
      stepIndex += 1
      const minuteMatch = body.match(/Roughly\s+(\w+)\s+minutes/)
      const minutes = minuteMatch ? (REVIEW_MINUTE_WORDS[minuteMatch[1]] ?? 0) : 0
      totalMinutes += minutes
      const badge = minutes
        ? `<span class="matrix-badge review-step-minutes">${minutes} min</span>`
        : '<span class="matrix-badge review-step-minutes review-step-optional">optional</span>'
      return `<li class="review-step"><input type="checkbox" data-review-check data-minutes="${minutes}" aria-label="Mark review step ${stepIndex} as read" /><span class="review-step-body">${body.trim()}</span>${badge}</li>`
    })
  const replacement = `<div class="review-route" data-review-route data-total-minutes="${totalMinutes}">
  <p class="review-status" aria-live="polite" data-review-status>A full first pass is about ${totalMinutes} minutes of reading. Check steps off as you go.</p>
  <ol class="review-route-list">${items}</ol>
</div>`
  return article.slice(0, markerIndex) + article.slice(markerIndex + marker.length, start) + replacement + article.slice(end + '</ol>'.length)
}

// ---------- editor replay (Editor integration) ----------
// A VS Code styled window that steps through a real editing session: open
// with live diagnostics, apply the validated quick fix, then format on save.
// The buffer states are highlighted at build time and the diagnostics and
// latency figures are the ones recorded by the editor benchmark and the
// Extension Host walkthrough.

// Wraps the first occurrence of a plain-text target inside highlighted HTML
// with a squiggle span that reuses the site's hover-doc tooltip. The match
// may cross token boundaries; each covered slice is wrapped separately.
function addSquiggle(html, target, kind, title, doc) {
  const segments = html.split(/(<[^>]+>)/)
  let text = ''
  const spans = []
  for (const [index, segment] of segments.entries()) {
    if (index % 2 === 0 && segment) {
      spans.push({ index, start: text.length, end: text.length + segment.length })
      text += segment
    }
  }
  const start = text.indexOf(target)
  if (start === -1) throw new Error(`Editor replay target not found: ${target}`)
  const end = start + target.length
  const hover = `class="er-squiggle er-squiggle-${kind} tsrx-hover" tabindex="0" role="img" aria-label="${escapeHtml(
    `${title}: ${doc}`,
  )}" data-doc-title="${escapeHtml(title)}" data-doc="${escapeHtml(doc)}"`
  for (const span of spans) {
    if (span.end <= start || span.start >= end) continue
    const segment = segments[span.index]
    const from = Math.max(0, start - span.start)
    const to = Math.min(segment.length, end - span.start)
    segments[span.index] =
      `${segment.slice(0, from)}<span ${hover}>${segment.slice(from, to)}</span>${segment.slice(to)}`
  }
  return segments.join('')
}

const EDITOR_REPLAY_DIAGNOSTICS = {
  console: {
    kind: 'warning',
    title: 'eslint(no-console) · warning',
    doc: 'Unexpected console statement. Mapped to the exact authored bytes you typed, never to projection scaffolding.',
  },
  debugger: {
    kind: 'error',
    title: 'eslint(no-debugger) · error',
    doc: '`debugger` statement is not allowed. The quick fix for this line is validated against your authored bytes before VS Code applies it.',
  },
}

function editorReplayWindow({ code, targets, problems, status }) {
  let body = highlightHtml(code, 'tsrx')
  body = addTsrxHovers(body)
  for (const target of targets) {
    const diagnostic = EDITOR_REPLAY_DIAGNOSTICS[target]
    body = addSquiggle(
      body,
      target === 'console' ? 'console.log' : 'debugger',
      diagnostic.kind,
      diagnostic.title,
      diagnostic.doc,
    )
  }
  const problemLines = problems.length
    ? problems
        .map(
          (problem) =>
            `<li class="er-problem er-problem-${problem.kind}">${escapeHtml(problem.text)}</li>`,
        )
        .join('')
    : '<li class="er-problem er-problem-clear">No problems detected in Counter.tsrx</li>'
  const problemCount = problems.filter((problem) => problem.kind === 'error').length
  const warningCount = problems.filter((problem) => problem.kind === 'warning').length
  return `<div class="er-window" role="group" aria-label="Simulated VS Code window">
  <div class="er-titlebar"><span class="er-dot"></span><span class="er-dot"></span><span class="er-dot"></span><span class="er-filetab">Counter.tsrx</span></div>
  <div class="er-code">${body}</div>
  <div class="er-problems" aria-label="Problems panel"><p class="er-problems-title">Problems</p><ul>${problemLines}</ul></div>
  <div class="er-statusbar"><span class="er-status-counts" aria-label="${problemCount} errors, ${warningCount} warnings">✕ ${problemCount} ⚠ ${warningCount}</span><span class="er-status-latency">${escapeHtml(status)}</span></div>
</div>`
}

function editorReplayStages() {
  const openCode = `export function Counter({start}:{start:number}) @{
  var count = start;
  console.log("mounted");
  debugger;

  <div   class="counter">
    <span>{count}</span>
  </div>
}`
  const fixedCode = `export function Counter({start}:{start:number}) @{
  var count = start;
  console.log("mounted");

  <div   class="counter">
    <span>{count}</span>
  </div>
}`
  const formattedCode = `export function Counter({ start }: { start: number }) @{
  var count = start;
  console.log("mounted");

  <div class="counter">
    <span>{count}</span>
  </div>
}`
  const consoleProblem = {
    kind: 'warning',
    text: 'eslint(no-console): Unexpected console statement · at your authored bytes',
  }
  return [
    {
      id: 'open',
      label: 'Open',
      text: 'You open an unsaved buffer with two problems in it. The native server lints the in-memory text and the squiggles land on the exact bytes you typed. Hover a squiggle to read the real diagnostic.',
      window: editorReplayWindow({
        code: openCode,
        targets: ['console', 'debugger'],
        problems: [
          consoleProblem,
          {
            kind: 'error',
            text: 'eslint(no-debugger): `debugger` statement is not allowed · at your authored bytes',
          },
        ],
        status: 'open to first diagnostics: 2.40 ms median',
      }),
    },
    {
      id: 'quickfix',
      label: 'Quick fix',
      text: 'You accept the quickfix for no-debugger. The server only offered it because the affected text exists verbatim in your file and the fixed result reparses as valid TSRX. The debugger line is gone and the error disappears with it.',
      window: editorReplayWindow({
        code: fixedCode,
        targets: ['console'],
        problems: [consoleProblem],
        status: 'code action round trip: under 1 ms p95',
      }),
    },
    {
      id: 'format',
      label: 'Format on save',
      text: 'You save. Oxfmt formats a projected TSX copy, the result is lifted back into TSRX syntax, and the lift is verified before the editor applies one edit. The messy spacing is gone and your @-controls are untouched.',
      window: editorReplayWindow({
        code: formattedCode,
        targets: ['console'],
        problems: [consoleProblem],
        status: 'format request: under 1 ms p95 · code actions never touch disk',
      }),
    },
  ]
}

function editorReplayHtml() {
  const stages = editorReplayStages()
  const prefix = 'er'
  return `<figure class="er-replay" data-editor-replay>
  <div class="er-replay-head">
    <span class="er-replay-title">One editing session, replayed</span>
    <button type="button" class="er-play" data-er-play aria-label="Play the editor session">Play</button>
  </div>
  <div class="explorer pipeline er-stages" data-explorer>
    <div class="explorer-tabs pipeline-tabs" role="tablist" aria-label="Editor session stages">
      ${stages
        .map(
          (stage, index) =>
            `<button type="button" role="tab" id="${prefix}-tab-${stage.id}" aria-controls="${prefix}-panel-${stage.id}" aria-selected="${index === 0}" ${index === 0 ? '' : 'tabindex="-1"'}><span class="pipeline-step" aria-hidden="true">${index + 1}</span>${stage.label}</button>`,
        )
        .join('\n      ')}
    </div>
    ${stages
      .map(
        (stage, index) =>
          `<div class="explorer-panel" role="tabpanel" id="${prefix}-panel-${stage.id}" aria-labelledby="${prefix}-tab-${stage.id}" ${index === 0 ? '' : 'hidden'}><p class="pipeline-text">${stage.text}</p>${stage.window}</div>`,
      )
      .join('\n    ')}
  </div>
  <figcaption>The diagnostics, quick-fix rules, and latency figures are the real recorded ones: 2.40 ms median open-to-diagnostics and sub-millisecond edit, format, and code-action p95 on the recorded Apple M5 Pro, with zero disk writes from code actions.</figcaption>
</figure>`
}

function editorReplayMarkdown() {
  return [
    'One editing session, replayed in three stages:',
    '',
    '1. **Open.** An unsaved buffer with a `console.log` warning and a `debugger` error gets live diagnostics on the exact authored bytes (2.40 ms median open to first diagnostics).',
    '2. **Quick fix.** The validated `no-debugger` quickfix removes the statement; it was only offered because the text exists verbatim and the result reparses (under 1 ms p95).',
    '3. **Format on save.** Oxfmt formats a projected copy, the lift back to TSRX is verified, and one edit fixes the messy spacing (under 1 ms p95, zero disk writes).',
  ].join('\n')
}

// ---------- annotated config examples (Configuration) ----------
// <!-- annotate-config --> before a fenced jsonc block gives the known keys
// the same hover-doc treatment as TSRX tokens, so hovering a field explains
// what the native boundary does with it.
const CONFIG_DOCS = {
  plugins: ['plugins', 'Enables built-in plugin rule sets like react or typescript. JavaScript plugins are not supported and fail loudly.'],
  env: ['env', 'Declares an environment such as browser, which defines its globals through the canonical ConfigStoreBuilder.'],
  globals: ['globals', 'Declares project-specific globals and whether code may write to them.'],
  rules: ['rules', 'Sets severity and options per rule with canonical OXC precedence. CLI -A, -W, and -D flags win over these.'],
  overrides: ['overrides', 'Per-pattern changes, matched against your authored .tsrx paths before any projection exists, so **/*.tsrx keeps working.'],
  files: ['files', 'The glob patterns this override applies to.'],
  ignorePatterns: ['ignorePatterns', 'Paths to skip, rooted at the directory the config file lives in.'],
  options: ['options', 'Exit policy (denyWarnings, maxWarnings) and the opt-in type lanes.'],
  typeAware: ['typeAware', 'Opts into tsgolint type-aware rules. The direct native command still requires the explicit --type-aware flag so a TypeScript-Go process never starts unexpectedly.'],
  typeCheck: ['typeCheck', 'Also publishes TypeScript syntactic and semantic diagnostics, and implies the type-aware lane.'],
  singleQuote: ['singleQuote', 'Prefer single quotes in JS and TS output.'],
  semi: ['semi', 'Whether statements end with semicolons.'],
  printWidth: ['printWidth', 'The line width the formatter tries to fit.'],
  singleAttributePerLine: ['singleAttributePerLine', 'Puts each JSX attribute on its own line when an element wraps.'],
}

function annotateConfigBlocks(article) {
  const marker = '<!-- annotate-config -->'
  while (article.includes(marker)) {
    const markerIndex = article.indexOf(marker)
    const start = article.indexOf('<div class="code-block" data-lang="jsonc">', markerIndex)
    if (start === -1) throw new Error('annotate-config marker found without a following jsonc block')
    const end = article.indexOf('</div>', start)
    let block = article.slice(start, end)
    let annotated = 0
    block = block.replace(
      /(<span[^>]*>)([ \t]*)(&quot;|")([A-Za-z]+)\3(<\/span>)/g,
      (match, open, whitespace, quote, key, close) => {
        const doc = CONFIG_DOCS[key]
        if (!doc) return match
        annotated += 1
        return `${open}${whitespace}<span class="tsrx-hover" tabindex="0" role="img" aria-label="${escapeHtml(
          `${doc[0]}: ${doc[1]}`,
        )}" data-doc-title="${escapeHtml(doc[0])}" data-doc="${escapeHtml(doc[1])}">${quote}${key}${quote}</span>${close}`
      },
    )
    if (annotated === 0) throw new Error('annotate-config found no known keys to annotate')
    article = article.slice(0, markerIndex) + article.slice(markerIndex + marker.length, start) + block + article.slice(end)
  }
  return article
}

function alignProjectionLines(sourceLines, projectedLines) {
  const lengths = Array.from({ length: sourceLines.length + 1 }, () =>
    Array(projectedLines.length + 1).fill(0),
  )
  for (let sourceIndex = sourceLines.length - 1; sourceIndex >= 0; sourceIndex--) {
    for (let projectedIndex = projectedLines.length - 1; projectedIndex >= 0; projectedIndex--) {
      lengths[sourceIndex][projectedIndex] =
        sourceLines[sourceIndex] === projectedLines[projectedIndex]
          ? lengths[sourceIndex + 1][projectedIndex + 1] + 1
          : Math.max(
              lengths[sourceIndex + 1][projectedIndex],
              lengths[sourceIndex][projectedIndex + 1],
            )
    }
  }

  const pairs = []
  let sourceIndex = 0
  let projectedIndex = 0
  while (sourceIndex < sourceLines.length && projectedIndex < projectedLines.length) {
    if (sourceLines[sourceIndex] === projectedLines[projectedIndex]) {
      pairs.push({ sourceIndex, projectedIndex })
      sourceIndex++
      projectedIndex++
    } else if (
      lengths[sourceIndex + 1][projectedIndex] >=
      lengths[sourceIndex][projectedIndex + 1]
    ) {
      sourceIndex++
    } else {
      projectedIndex++
    }
  }
  return pairs
}

function decorateProjectionLines(html, mapIds, { unpairedAttr, diagLines } = {}) {
  let lineIndex = 0
  return html
    .replace(/<span class="line">/g, () => {
      const index = lineIndex++
      const mapId = mapIds.get(index)
      const attrs = [
        mapId !== undefined ? ` data-map-id="${mapId}"` : unpairedAttr ? ` ${unpairedAttr}` : '',
        diagLines?.has(index) ? ' data-diag-line' : '',
      ].join('')
      return `<span class="line"${attrs}>`
    })
    .replace(/\r?\n(?=<span class="line"(?:\s|>))/g, '')
}

// "How it works" walkthrough: the four pipeline steps as buttons that light up
// the matching lines of the linked projection map, one explanation at a time.
async function howItWorksHtml() {
  const example = await loadProjectionExample()
  if (!example) {
    return '<p><em>Run <code>node docs/generate-projection.mjs</code> to generate the walkthrough.</em></p>'
  }

  const sourceLines = example.tsrx.split('\n')
  const projectedLines = example.projected.split('\n')
  const pairs = alignProjectionLines(sourceLines, projectedLines)
  const sourceMapIds = new Map()
  const projectedMapIds = new Map()
  pairs.forEach(({ sourceIndex, projectedIndex }, mapId) => {
    sourceMapIds.set(sourceIndex, mapId)
    projectedMapIds.set(projectedIndex, mapId)
  })

  // The lines the real diagnostics point at, in both panes.
  const lineOfOffset = (text, offset) => text.slice(0, offset).split('\n').length - 1
  const sourceDiagLines = new Set(
    example.diagnostics.flatMap((diagnostic) =>
      diagnostic.labels.map((label) => lineOfOffset(example.tsrx, label.span.offset)),
    ),
  )
  const projectedDiagLines = new Set(
    pairs
      .filter((pair) => sourceDiagLines.has(pair.sourceIndex))
      .map((pair) => pair.projectedIndex),
  )

  const source = addTsrxHovers(
    decorateProjectionLines(highlightHtml(example.tsrx, 'tsrx'), sourceMapIds, {
      unpairedAttr: 'data-tsrx-only',
      diagLines: sourceDiagLines,
    }),
  )
  const projected = decorateProjectionLines(
    highlightHtml(example.projected, 'tsx'),
    projectedMapIds,
    { unpairedAttr: 'data-scaffold', diagLines: projectedDiagLines },
  )

  const diagCodes = [...new Set(example.diagnostics.map((diagnostic) => diagnostic.code))]
    .map((code) => `<code>${escapeHtml(code)}</code>`)
    .join(' and ')
  const steps = [
    {
      id: 'scan',
      label: 'Scan',
      text: 'One pass finds the TSRX-only lines, highlighted on the left. Stock OXC cannot parse them.',
    },
    {
      id: 'project',
      label: 'Project',
      text: 'Each construct becomes a valid TSX placeholder, highlighted on the right. Every other byte is your code, untouched.',
    },
    {
      id: 'lint',
      label: 'Run the real OXC',
      text: `Stock OXC runs on the copy, exactly once, and flags the highlighted lines: ${diagCodes}.`,
    },
    {
      id: 'map',
      label: 'Map back',
      text: 'Each warning lands back on the bytes you wrote, highlighted on the left. Formatting is lifted back the same way.',
    },
  ]
  return `<figure class="projection-map how-it-works" data-projection-map data-how-it-works>
  <div class="hiw-steps" role="group" aria-label="The four steps of the pipeline">
    ${steps
      .map(
        (step, index) =>
          `<button type="button" data-hiw-step="${step.id}" aria-pressed="false"><span class="pipeline-step" aria-hidden="true">${index + 1}</span>${step.label}</button>`,
      )
      .join('\n    ')}
    <button type="button" class="hiw-dim-toggle" aria-pressed="false" data-scaffolding-toggle>Dim the scaffolding</button>
  </div>
  <div class="hiw-strip" aria-live="polite">
    ${steps
      .map(
        (step) => `<p class="hiw-text" data-hiw-text="${step.id}">${step.text}</p>`,
      )
      .join('\n    ')}
  </div>
  <div class="projection-map-panes">
    <section class="projection-map-pane" aria-label="Your TSRX source code" data-map-id-count="${pairs.length}">
      <h3>Your TSRX</h3>
      <div class="projection-map-code">${source}</div>
    </section>
    <section class="projection-map-pane" aria-label="The projected TSX code OXC sees" data-map-id-count="${pairs.length}">
      <h3>What OXC actually sees</h3>
      <div class="projection-map-code">${projected}</div>
    </section>
  </div>
  <figcaption>Hover any shared line to see its twin in the other pane.</figcaption>
</figure>`
}

// Plain-markdown twin of the walkthrough for the copy-as-Markdown page,
// llms-full.txt, and the search index.
const howItWorksMarkdown = `1. **Scans** the file once and records where the TSRX-only syntax is.
2. **Projects** it: builds an in-memory copy where each TSRX construct is
   swapped for equivalent, valid TSX placeholders. Your real code between
   those constructs is copied byte-for-byte, and the tool remembers exactly
   which byte ranges are "your code" and which are placeholder.
3. **Runs the real OXC** (parser, then linter or formatter) on that copy.
   Exactly once. Even dynamic tags are validated against this same parse.
4. **Maps the results back** to your original file. Lint errors point at your
   actual \`.tsrx\` lines and columns. For formatting, a final step (the
   *lift*) converts the formatted TSX copy back into TSRX and double-checks
   that nothing structural changed.`

// "Copy page ▾" split menu: copy/view as Markdown, open in AI assistants.
// Package-manager install tabs. Authors write only the npm command after a
// <!-- pm-install --> marker; the pnpm/yarn/bun equivalents are derived here
// so the variants can never drift apart.
const PM_INSTALL_PREFIXES = {
  npm: 'npm install --save-dev',
  pnpm: 'pnpm add -D',
  yarn: 'yarn add -D',
  bun: 'bun add -d',
}
const PM_INSTALL_PATTERN = /<!-- pm-install -->\r?\n```sh\r?\n([\s\S]*?)\r?\n```/g

function pmInstallTabsHtml(npmCommand, groupId) {
  if (!npmCommand.startsWith(PM_INSTALL_PREFIXES.npm)) {
    throw new Error(
      `pm-install block must start with "${PM_INSTALL_PREFIXES.npm}", got: ${npmCommand.split('\n')[0]}`,
    )
  }
  const managers = Object.keys(PM_INSTALL_PREFIXES)
  const buttons = managers
    .map(
      (pm, index) =>
        `<button type="button" role="tab" id="pm-tab-${groupId}-${pm}" aria-controls="pm-panel-${groupId}-${pm}" aria-selected="${index === 0}" tabindex="${index === 0 ? 0 : -1}" data-pm="${pm}">${pm}</button>`,
    )
    .join('')
  const panels = managers
    .map((pm, index) => {
      const command =
        pm === 'npm' ? npmCommand : npmCommand.replace(PM_INSTALL_PREFIXES.npm, PM_INSTALL_PREFIXES[pm])
      return `<div role="tabpanel" id="pm-panel-${groupId}-${pm}" aria-labelledby="pm-tab-${groupId}-${pm}" data-pm="${pm}"${index === 0 ? '' : ' hidden'}><div class="code-block" data-lang="sh">${highlightHtml(command, 'sh')}</div></div>`
    })
    .join('')
  return `<div class="pm-tabs" data-pm-tabs><div class="pm-tabs-bar" role="tablist" aria-label="Package manager">${buttons}</div>${panels}</div>\n`
}

function pageMenuHtml(link) {
  const mdHref = withBase(`${link}.md`)
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

// Named terminal walkthroughs (docs/generate-transcripts.mjs), embedded on
// pages via <!-- terminal-demo:NAME -->.
async function loadTerminalTranscripts() {
  try {
    return JSON.parse(await readFile(path.join(docsDir, 'terminal-transcripts.json'), 'utf8'))
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
    <p>A release gate is an automated pass or fail check that runs before every release. Each benchmark below has a frozen budget, meaning the worst result we allow ourselves to ship. If a new build ever lands on the wrong side of a budget, the release fails and does not go out. Every number below is read from the committed benchmark reports when this site is built. Measured ${reportDate} on one machine; your hardware will differ.</p>
    <h3 class="home-bench-sub">Lint the same 1,000 files, three tools</h3>
    ${await comparativeChartHtml()}
    <h3 class="home-bench-sub">Release gates we ship against</h3>
    ${await homeBenchmarksHtml()}
    <p class="home-bench-link"><a href="${withBase('/reference/benchmarks')}">See every gate and report →</a></p>
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
  <section class="home-upstream" aria-label="Upstreaming to OXC">
    <h2>We want to upstream this to OXC</h2>
    <p>OXC for TSRX is an independent community project, and its end goal is for TSRX support to live in OXC itself. The reusable language core is kept small, isolated, and benchmark-gated so that OXC maintainers can review it with as little effort as possible. No OXC maintainer interest or endorsement is claimed.</p>
    <p class="home-upstream-link"><a href="${withBase('/architecture/upstreaming-to-oxc')}">Read the upstreaming review map →</a></p>
  </section>
  <footer class="home-footer">
    <p class="footer-links"><a href="${config.repository}" target="_blank" rel="noreferrer">GitHub<span class="visually-hidden"> (opens in new tab)</span></a> · <a href="https://www.npmjs.com/package/oxlint-tsrx" target="_blank" rel="noreferrer">oxlint-tsrx<span class="visually-hidden"> (opens in new tab)</span></a> · <a href="https://www.npmjs.com/package/oxfmt-tsrx" target="_blank" rel="noreferrer">oxfmt-tsrx<span class="visually-hidden"> (opens in new tab)</span></a> · <a href="https://www.npmjs.com/package/@oxc-tsrx/runtime" target="_blank" rel="noreferrer">@oxc-tsrx/runtime<span class="visually-hidden"> (opens in new tab)</span></a></p>
    ${footerBadge}
    <p class="footer-disclaimer">${config.footer.disclaimer}</p>
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
    pathname: '/playground',
    bodyClass: 'home-page',
    header: headerHtml(),
    main,
  })
}

async function build() {
  execFileSync(process.execPath, [path.join(docsDir, 'render-diagrams.mjs')], { stdio: 'inherit' })
  await validateOutputDirectory()
  await rm(outDir, { recursive: true, force: true })
  await mkdir(outDir, { recursive: true })

  const flat = config.sidebar.flatMap((group) =>
    group.items.map((item) => ({ ...item, group: group.text })),
  )
  const supplementalPages = [
    {
      text: 'Embedded CSS boundary',
      link: '/architecture/embedded-css-boundary',
      group: 'Architecture',
    },
  ]
  const pages = [...flat, ...supplementalPages]
  const searchDocs = []

  const markdownPages = []
  for (const [pageIndex, item] of pages.entries()) {
    const sourcePath = path.join(docsDir, `${item.link.replace(/^\//, '')}.md`)
    const source = await readFile(sourcePath, 'utf8')
    const { data, body: sourceBody } = parseFrontmatter(source)
    // Swap pm-install blocks for placeholders before markdown rendering; the
    // markdown twin keeps only the plain npm fence with the marker stripped.
    const pmInstallBlocks = []
    const body = sourceBody.replace(PM_INSTALL_PATTERN, (match, command) => {
      pmInstallBlocks.push(command)
      return `<!-- pm-tabs:${pmInstallBlocks.length - 1} -->`
    })
    let exportedBody = sourceBody.replace(/<!-- pm-install -->\r?\n/g, '')
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
      const benchmarkMarkdown = await benchmarksSectionsMarkdown()
      article = article.replace('<!-- benchmarks:auto -->', await benchmarksSectionsHtml())
      exportedBody = exportedBody.replace('<!-- benchmarks:auto -->', benchmarkMarkdown)
      const anchor = headings.findIndex((heading) => heading.text === 'Measurement hygiene')
      headings.splice(anchor === -1 ? headings.length : anchor, 0, ...benchmarkHeadings)
    }
    if (article.includes('<!-- projection-explorer -->')) {
      article = article.replace('<!-- projection-explorer -->', await projectionExplorerHtml())
    }
    if (article.includes('<!-- how-it-works -->')) {
      article = article.replace('<!-- how-it-works -->', await howItWorksHtml())
      exportedBody = exportedBody.replace('<!-- how-it-works -->', howItWorksMarkdown)
    }
    if (article.includes('<!-- terminal-demo -->')) {
      const example = await loadProjectionExample()
      article = article.replace('<!-- terminal-demo -->', terminalDemoHtml(example))
      exportedBody = exportedBody.replace('<!-- terminal-demo -->', terminalDemoMarkdown(example))
    }
    for (const match of article.matchAll(/<!-- terminal-demo:([a-z0-9-]+) -->/g)) {
      const demo = (await loadTerminalTranscripts())?.demos?.[match[1]]
      const generator = 'docs/generate-transcripts.mjs'
      article = article.replace(match[0], terminalDemoHtml(demo, generator))
      exportedBody = exportedBody.replace(match[0], terminalDemoMarkdown(demo, generator))
    }
    if (article.includes('<!-- matrix-filter -->')) {
      article = matrixFilterHtml(article)
    }
    if (article.includes('<!-- review-route -->')) {
      article = reviewRouteHtml(article)
    }
    if (article.includes('<!-- editor-replay -->')) {
      article = article.replace('<!-- editor-replay -->', editorReplayHtml())
      exportedBody = exportedBody.replace('<!-- editor-replay -->', editorReplayMarkdown())
    }
    if (article.includes('<!-- annotate-config -->')) {
      article = annotateConfigBlocks(article)
    }
    if (article.includes('<!-- pipeline:lint -->')) {
      article = article.replace('<!-- pipeline:lint -->', await pipelineHtml('lint'))
    }
    if (article.includes('<!-- pipeline:format -->')) {
      article = article.replace('<!-- pipeline:format -->', await pipelineHtml('format'))
    }
    for (const match of article.matchAll(/<!-- diagram:([a-z0-9-]+) -->/g)) {
      article = article.replace(match[0], await diagramHtml(match[1]))
    }
    for (const [index, command] of pmInstallBlocks.entries()) {
      article = article.replace(`<!-- pm-tabs:${index} -->`, pmInstallTabsHtml(command, index))
    }
    article = addGlossary(article)
    searchDocs.push(...extractSections(new Marked(), exportedBody, page))
    const html = renderDocPage({
      page,
      article,
      headings,
      pageIndex: pageIndex < flat.length ? pageIndex : -1,
      flat,
    })
    const outPath = path.join(outDir, `${item.link.replace(/^\//, '')}.html`)
    await mkdir(path.dirname(outPath), { recursive: true })
    await writeFile(outPath, html)
    // Raw markdown twin for the copy-as-Markdown button and llms-full.txt.
    await writeFile(outPath.replace(/\.html$/, '.md'), exportedBody)
    markdownPages.push({ ...page, body: exportedBody })
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
          return `- [${item.text}](${withBase(`${item.link}.md`)})${page?.description ? `: ${page.description}` : ''}`
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
  // Ship the stylesheet without its comments (the source keeps them). The
  // stylesheet loads on every page, and the home page has a hard transfer
  // budget in docs/verify.mjs; comments are the one safe-to-drop chunk.
  const shippedStyle = path.join(outDir, 'assets', 'style.css')
  await writeFile(
    shippedStyle,
    (await readFile(shippedStyle, 'utf8'))
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/\n{3,}/g, '\n\n'),
  )
  await rolldownBuild({
    input: path.join(docsDir, 'demo-highlighter-entry.mjs'),
    platform: 'browser',
    output: {
      format: 'esm',
      file: path.join(outDir, 'assets', 'demo-highlighter.js'),
      minify: true,
    },
  })
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

  const publicPaths = ['/', ...pages.map(({ link }) => link), '/playground']
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
