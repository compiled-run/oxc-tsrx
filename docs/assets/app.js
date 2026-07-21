// OXC for TSRX docs client — vanilla JS: theme, nav drawer, copy, outline,
// search, and Navigation-API SPA routing (progressive enhancement).

// ---------- theme toggle (persistent chrome) ----------
const themeToggle = document.getElementById('theme-toggle')
const root = document.documentElement

function syncThemeButton() {
  themeToggle.setAttribute('aria-pressed', String(root.classList.contains('dark')))
}

syncThemeButton()

themeToggle.addEventListener('click', () => {
  const dark = root.classList.toggle('dark')
  try {
    localStorage.setItem('oxc-tsrx-theme', dark ? 'dark' : 'light')
  } catch {}
  syncThemeButton()
})

// ---------- mobile sidebar drawer (sidebar/backdrop are per-page, so query lazily) ----------
const menuToggle = document.getElementById('menu-toggle')

function setSidebarOpen(open) {
  const backdrop = document.getElementById('sidebar-backdrop')
  if (!backdrop) return
  document.body.classList.toggle('sidebar-open', open)
  menuToggle.setAttribute('aria-expanded', String(open))
  backdrop.hidden = !open
}

if (menuToggle) {
  menuToggle.addEventListener('click', () =>
    setSidebarOpen(!document.body.classList.contains('sidebar-open')),
  )
  document.addEventListener('click', (event) => {
    if (event.target.id === 'sidebar-backdrop') setSidebarOpen(false)
  })
  window.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && document.body.classList.contains('sidebar-open')) {
      setSidebarOpen(false)
      menuToggle.focus()
    }
  })
}

window.addEventListener('keydown', (event) => {
  if (event.key !== 'Escape') return
  for (const list of document.querySelectorAll('.page-menu-list:not([hidden])')) {
    list.hidden = true
    list.closest('.page-menu')?.querySelector('.page-menu-toggle')?.setAttribute('aria-expanded', 'false')
  }
})

// ---------- per-page: copy buttons on code blocks ----------
function initCopyButtons() {
  for (const block of document.querySelectorAll('.code-block')) {
    if (block.querySelector('.copy-button')) continue
    const button = document.createElement('button')
    button.type = 'button'
    button.className = 'copy-button'
    button.setAttribute('aria-label', 'Copy code to clipboard')
    button.innerHTML =
      '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>'
    button.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(block.querySelector('code').textContent.trimEnd())
        button.classList.add('copied')
        button.setAttribute('aria-label', 'Copied')
        setTimeout(() => {
          button.classList.remove('copied')
          button.setAttribute('aria-label', 'Copy code to clipboard')
        }, 2000)
      } catch {}
    })
    block.appendChild(button)
  }
}

function applyPmChoice(pm) {
  for (const group of document.querySelectorAll('[data-pm-tabs]')) {
    if (!group.querySelector(`[role="tab"][data-pm="${pm}"]`)) continue
    for (const tab of group.querySelectorAll('[role="tab"]')) {
      const active = tab.dataset.pm === pm
      tab.setAttribute('aria-selected', String(active))
      tab.tabIndex = active ? 0 : -1
    }
    for (const panel of group.querySelectorAll('[role="tabpanel"]')) {
      panel.hidden = panel.dataset.pm !== pm
    }
  }
}

function initPmTabs() {
  const selectPm = (pm) => {
    try {
      localStorage.setItem('oxc-tsrx-pm', pm)
    } catch {}
    applyPmChoice(pm)
  }
  for (const group of document.querySelectorAll('[data-pm-tabs]:not([data-ready])')) {
    group.dataset.ready = '1'
    const tabs = [...group.querySelectorAll('[role="tab"]')]
    for (const tab of tabs) tab.addEventListener('click', () => selectPm(tab.dataset.pm))
    group.querySelector('[role="tablist"]').addEventListener('keydown', (event) => {
      const delta = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
      const index = tabs.indexOf(document.activeElement)
      if (!delta || index === -1) return
      event.preventDefault()
      const next = tabs[(index + delta + tabs.length) % tabs.length]
      next.focus()
      selectPm(next.dataset.pm)
    })
  }
  let stored = null
  try {
    stored = localStorage.getItem('oxc-tsrx-pm')
  } catch {}
  if (stored) applyPmChoice(stored)
}

function initDiagrams() {
  for (const figure of document.querySelectorAll('.diagram:not([data-ready])')) {
    figure.dataset.ready = '1'

    const activateNode = (node) => {
      const nodeId = node.dataset.diagramNode
      for (const candidate of figure.querySelectorAll('[data-diagram-node]')) {
        const active = candidate.dataset.diagramNode === nodeId
        candidate.classList.toggle('diagram-node-active', active)
        candidate.setAttribute('aria-pressed', String(active))
      }
      figure.querySelector('.diagram-caption-strip').textContent = node.dataset.caption
    }

    const activateStep = (step) => {
      const selected = new Set(JSON.parse(step.dataset.nodes))
      for (const button of figure.querySelectorAll('[data-diagram-step]')) {
        button.setAttribute('aria-pressed', String(button === step))
      }
      for (const node of figure.querySelectorAll('[data-diagram-node]')) {
        const highlighted = selected.has(node.dataset.diagramNode)
        node.classList.toggle('diagram-step-highlight', highlighted)
        node.classList.toggle('diagram-step-dimmed', !highlighted)
        node.classList.remove('diagram-node-active')
        node.setAttribute('aria-pressed', 'false')
      }
      if (step.dataset.caption) {
        figure.querySelector('.diagram-caption-strip').textContent = step.dataset.caption
      }
    }

    figure.addEventListener('click', (event) => {
      const step = event.target.closest('[data-diagram-step]')
      if (step) {
        activateStep(step)
        return
      }
      const node = event.target.closest('[data-diagram-node]')
      if (node && figure.contains(node)) activateNode(node)
    })

    const firstStep = figure.querySelector('[data-diagram-step]')
    if (firstStep) activateStep(firstStep)

    figure.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter' && event.key !== ' ') return
      const node = event.target.closest('[data-diagram-node]')
      if (!node || !figure.contains(node)) return
      event.preventDefault()
      activateNode(node)
    })
  }
}

function initProjectionMaps() {
  for (const map of document.querySelectorAll('[data-projection-map]:not([data-ready])')) {
    map.dataset.ready = '1'
    let activeMapId = null

    const highlightPair = (mapId) => {
      if (mapId === activeMapId) return
      activeMapId = mapId
      for (const line of map.querySelectorAll('[data-map-id]')) {
        line.classList.toggle('projection-line-active', line.dataset.mapId === mapId)
      }
    }

    map.addEventListener('mouseover', (event) => {
      const line = event.target.closest('[data-map-id]')
      if (line && map.contains(line)) highlightPair(line.dataset.mapId)
    })
    map.addEventListener('mouseleave', () => highlightPair(null))
    map.addEventListener('focusin', (event) => {
      const line = event.target.closest('[data-map-id]')
      if (line && map.contains(line)) highlightPair(line.dataset.mapId)
    })
    map.addEventListener('focusout', (event) => {
      if (!map.contains(event.relatedTarget)) highlightPair(null)
    })

    map.querySelector('[data-scaffolding-toggle]')?.addEventListener('click', (event) => {
      const pressed = event.currentTarget.getAttribute('aria-pressed') !== 'true'
      event.currentTarget.setAttribute('aria-pressed', String(pressed))
      map.classList.toggle('projection-map-dim-scaffolding', pressed)
    })
  }
}

function initHowItWorks() {
  for (const figure of document.querySelectorAll('[data-how-it-works]:not([data-hiw-ready])')) {
    figure.dataset.hiwReady = '1'
    const buttons = [...figure.querySelectorAll('[data-hiw-step]')]

    const selectStep = (step) => {
      figure.dataset.step = step
      for (const button of buttons) {
        button.setAttribute('aria-pressed', String(button.dataset.hiwStep === step))
      }
    }

    for (const button of buttons) {
      button.addEventListener('click', () => selectStep(button.dataset.hiwStep))
    }
    // Without JS the strip shows all four explanations; with JS it becomes a
    // step-through starting at the first step.
    selectStep(buttons[0].dataset.hiwStep)
  }
}

const pageCleanupCallbacks = []

function cleanupPage() {
  for (const cleanup of pageCleanupCallbacks.splice(0)) cleanup()
}

function initTerminalDemos() {
  for (const terminal of document.querySelectorAll('[data-terminal-demo]:not([data-ready])')) {
    terminal.dataset.ready = '1'
    const button = terminal.querySelector('[data-terminal-play]')
    const lines = [...terminal.querySelectorAll('.gs-terminal-line')]
    let timerId = null

    const stopReplay = () => {
      if (timerId !== null) clearTimeout(timerId)
      timerId = null
    }
    pageCleanupCallbacks.push(stopReplay)

    terminal.classList.add('gs-terminal-enhanced')

    button.addEventListener('click', () => {
      stopReplay()
      for (const line of lines) line.classList.add('gs-terminal-line-hidden')

      if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
        for (const line of lines) line.classList.remove('gs-terminal-line-hidden')
        button.textContent = 'Replay'
        button.setAttribute('aria-label', 'Replay terminal walkthrough')
        return
      }

      let index = 0
      const revealNext = () => {
        if (!terminal.isConnected) {
          stopReplay()
          return
        }
        lines[index++].classList.remove('gs-terminal-line-hidden')
        if (index < lines.length) timerId = setTimeout(revealNext, 80)
        else {
          timerId = null
          button.textContent = 'Replay'
          button.setAttribute('aria-label', 'Replay terminal walkthrough')
        }
      }
      revealNext()
    })
  }
}

// ---------- per-page: outline scroll spy (one persistent listener) ----------
let spyEntries = []
let spyActiveItem = null
let spyTicking = false

function collectOutline() {
  spyActiveItem?.classList.remove('active')
  spyActiveItem = null
  spyEntries = [...document.querySelectorAll('.outline a[href^="#"]')]
    .map((link) => ({
      item: link.parentElement,
      heading: document.getElementById(link.getAttribute('href').slice(1)),
    }))
    .filter((entry) => entry.heading)
  updateSpy()
}

function updateSpy() {
  spyTicking = false
  if (spyEntries.length === 0) return
  let current = spyEntries[0]
  for (const entry of spyEntries) {
    if (entry.heading.getBoundingClientRect().top <= 120) current = entry
    else break
  }
  if (current.item !== spyActiveItem) {
    spyActiveItem?.classList.remove('active')
    current.item.classList.add('active')
    spyActiveItem = current.item
  }
}

window.addEventListener(
  'scroll',
  () => {
    if (!spyTicking) {
      spyTicking = true
      requestAnimationFrame(updateSpy)
    }
  },
  { passive: true },
)

function initPage() {
  initCopyButtons()
  initPmTabs()
  initDiagrams()
  initProjectionMaps()
  initHowItWorks()
  initTerminalDemos()
  if (document.querySelector('[data-matrix-filter], [data-review-route], [data-editor-replay]')) {
    import(new URL('./interactive.js', import.meta.url))
      .then((module) => module.init(pageCleanupCallbacks))
      .catch(() => {})
  }
  collectOutline()
  const demo = document.getElementById('hero-demo')
  if (demo && !demo.dataset.ready) {
    demo.dataset.ready = '1'
    import(new URL('./playground.js', import.meta.url))
      .then((module) => module.initDemo(demo))
      .catch(() => {})
  }
}
initPage()

// ---------- delegated one-time handlers (survive SPA content swaps) ----------
const playgroundHref = () =>
  document.querySelector('.top-nav a[href$="/playground"]')?.getAttribute('href') ??
  '/playground'

const toBase64Url = (text) => {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')
}

document.addEventListener('click', async (event) => {
  const tryButton = event.target.closest('.try-button')
  if (tryButton) {
    location.assign(`${playgroundHref()}#code=${toBase64Url(tryButton.dataset.code)}`)
    return
  }
  const copyMd = event.target.closest('.copy-md-button')
  if (copyMd) {
    const label = copyMd.dataset.label ?? (copyMd.dataset.label = copyMd.textContent)
    try {
      const markdown = await (await fetch(copyMd.dataset.mdHref)).text()
      await navigator.clipboard.writeText(markdown)
      copyMd.textContent = 'Copied!'
    } catch {
      copyMd.textContent = 'Copy failed'
    }
    setTimeout(() => {
      copyMd.textContent = label
    }, 2000)
    return
  }
  const menuToggleButton = event.target.closest('.page-menu-toggle')
  if (menuToggleButton) {
    const list = menuToggleButton.closest('.page-menu').querySelector('.page-menu-list')
    const open = list.hidden
    list.hidden = !open
    menuToggleButton.setAttribute('aria-expanded', String(open))
    return
  }
  // Any click outside an open page menu (or on one of its items) closes it.
  for (const list of document.querySelectorAll('.page-menu-list:not([hidden])')) {
    if (!list.contains(event.target) || event.target.closest('[role="menuitem"]')) {
      list.hidden = true
      list.closest('.page-menu')?.querySelector('.page-menu-toggle')?.setAttribute('aria-expanded', 'false')
    }
  }
  const explorerTab = event.target.closest('[data-explorer] [role="tab"]')
  if (explorerTab) {
    const explorer = explorerTab.closest('[data-explorer]')
    for (const tab of explorer.querySelectorAll('[role="tab"]')) {
      const selected = tab === explorerTab
      tab.setAttribute('aria-selected', String(selected))
      if (selected) tab.removeAttribute('tabindex')
      else tab.setAttribute('tabindex', '-1')
    }
    for (const tabPanel of explorer.querySelectorAll('[role="tabpanel"]')) {
      tabPanel.hidden = tabPanel.id !== explorerTab.getAttribute('aria-controls')
    }
  }
})

// ---------- interactive benchmark charts: tooltip on hover/focus ----------
let chartTooltip = null
function showChartTooltip(row) {
  if (!chartTooltip) {
    chartTooltip = document.createElement('div')
    chartTooltip.className = 'chart-tooltip'
    chartTooltip.setAttribute('role', 'tooltip')
    document.body.appendChild(chartTooltip)
  }
  // Dataset values are double-escaped at build time, so they stay inert here.
  const { label, result, budget, pct, pass, note, samples } = row.dataset
  chartTooltip.innerHTML =
    `<strong>${label}</strong>` +
    `<span>Result: ${result}</span>` +
    `<span>Budget: ${budget}</span>` +
    `<span>${pct} · ${pass === 'true' ? '✓ pass' : '✗ fail'}</span>` +
    (samples ? `<span class="chart-tooltip-samples">${samples}</span>` : '') +
    (note ? `<span class="chart-tooltip-note">${note}</span>` : '')
  chartTooltip.hidden = false
  const bar = row.querySelector('.bench-bar')?.getBoundingClientRect() ?? row.getBoundingClientRect()
  const width = chartTooltip.offsetWidth
  chartTooltip.style.left = `${Math.min(Math.max(8, bar.left + bar.width / 2 - width / 2), window.innerWidth - width - 8)}px`
  chartTooltip.style.top = `${Math.max(8, bar.top - chartTooltip.offsetHeight - 8)}px`
}
function hideChartTooltip() {
  if (chartTooltip) chartTooltip.hidden = true
}
function showDocHover(span) {
  if (!chartTooltip) {
    chartTooltip = document.createElement('div')
    chartTooltip.className = 'chart-tooltip'
    chartTooltip.setAttribute('role', 'tooltip')
    document.body.appendChild(chartTooltip)
  }
  chartTooltip.innerHTML = `<strong>${span.dataset.docTitle}</strong><span>${span.dataset.doc}</span>`
  chartTooltip.hidden = false
  const rect = span.getBoundingClientRect()
  const width = chartTooltip.offsetWidth
  chartTooltip.style.left = `${Math.min(Math.max(8, rect.left), window.innerWidth - width - 8)}px`
  chartTooltip.style.top = `${Math.max(8, rect.top - chartTooltip.offsetHeight - 8)}px`
}

const hoverDispatch = (event) => {
  const row = event.target.closest?.('.bench-row')
  if (row) {
    showChartTooltip(row)
    return
  }
  const doc = event.target.closest?.('.tsrx-hover')
  if (doc) showDocHover(doc)
  else hideChartTooltip()
}
document.addEventListener('mouseover', hoverDispatch)
document.addEventListener('focusin', hoverDispatch)
window.addEventListener('scroll', hideChartTooltip, { passive: true })

// Arrow-key navigation inside the explorer tablist.
document.addEventListener('keydown', (event) => {
  if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return
  const tab = event.target.closest?.('[data-explorer] [role="tab"]')
  if (!tab) return
  const tabs = [...tab.closest('[role="tablist"]').querySelectorAll('[role="tab"]')]
  const index = tabs.indexOf(tab)
  const next = tabs[(index + (event.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length]
  next.click()
  next.focus()
  event.preventDefault()
})

// ---------- SPA routing via the Navigation API (Chromium; others fall back to MPA) ----------
const pageCache = new Map()

async function fetchPage(href) {
  const url = new URL(href)
  if (!pageCache.has(url.pathname)) {
    const response = await fetch(url.href)
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    pageCache.set(url.pathname, await response.text())
  }
  return pageCache.get(url.pathname)
}

function swapPage(html, url) {
  cleanupPage()
  const doc = new DOMParser().parseFromString(html, 'text/html')
  const newMain = doc.querySelector('.layout .content')
  const currentMain = document.querySelector('.layout .content')
  if (newMain && currentMain) {
    // doc -> doc: keep header + sidebar alive so focus and scroll state survive.
    currentMain.replaceWith(newMain)
    document.querySelector('.layout .aside').replaceWith(doc.querySelector('.layout .aside'))
    for (const link of document.querySelectorAll('.sidebar a')) {
      if (new URL(link.href).pathname === url.pathname) link.setAttribute('aria-current', 'page')
      else link.removeAttribute('aria-current')
    }
  } else {
    // home <-> doc: structures differ, swap the whole routed region.
    document
      .querySelector('div.layout, main.home')
      .replaceWith(doc.querySelector('div.layout, main.home'))
  }
  document.title = doc.title
  document.body.className = doc.body.className
  setSidebarOpen(false)
  initPage()
  const announcer = document.getElementById('route-announcer')
  if (announcer) announcer.textContent = doc.title
  if (url.hash) {
    document.getElementById(decodeURIComponent(url.hash.slice(1)))?.scrollIntoView()
  } else {
    window.scrollTo(0, 0)
  }
  // Only move focus if the previously focused element was swapped away.
  if (!document.activeElement || document.activeElement === document.body) {
    const main = document.getElementById('main-content')
    main.tabIndex = -1
    main.focus({ preventScroll: true })
  }
}

if ('navigation' in window) {
  navigation.addEventListener('navigate', (event) => {
    if (!event.canIntercept || event.hashChange || event.downloadRequest !== null) return
    const url = new URL(event.destination.url)
    if (url.origin !== location.origin) return
    // Pages are extensionless routes (or "/", or a direct .html file); anything
    // with another file extension is an asset the router must not intercept.
    const lastSegment = url.pathname.split('/').at(-1) ?? ''
    if (lastSegment.includes('.') && !lastSegment.endsWith('.html')) return
    event.intercept({
      scroll: 'manual',
      focusReset: 'manual',
      async handler() {
        try {
          swapPage(await fetchPage(url.href), url)
        } catch {
          location.assign(url.href) // fall back to a full navigation
        }
      },
    })
  })

  // Warm the cache when a nav/sidebar link is hovered or touched.
  document.addEventListener('pointerover', (event) => {
    const link = event.target.closest('.sidebar a, .top-nav a, .pager a, .hero-actions a')
    if (link && new URL(link.href).origin === location.origin) {
      fetchPage(link.href).catch(() => {})
    }
  })
}

// ---------- search (persistent chrome) ----------
const searchButton = document.getElementById('search-button')
const searchDialog = document.getElementById('search-dialog')
const searchInput = document.getElementById('search-input')
const searchResults = document.getElementById('search-results')
const searchStatus = document.getElementById('search-status')
const searchClose = document.getElementById('search-close')

let searchEngine = null
let searchReady = null
let selectedIndex = -1

function loadSearch() {
  searchReady ??= (async () => {
    const [{ default: MiniSearch }, response] = await Promise.all([
      import(new URL('./minisearch/index.js', import.meta.url)),
      fetch(new URL('../search-index.json', import.meta.url)),
    ])
    const documents = await response.json()
    searchEngine = new MiniSearch({
      fields: ['title', 'text', 'page'],
      storeFields: ['title', 'text', 'page', 'group', 'href'],
      searchOptions: {
        boost: { title: 3, page: 2 },
        prefix: true,
        fuzzy: 0.15,
      },
    })
    searchEngine.addAll(documents)
  })()
  return searchReady
}

function openSearch() {
  searchDialog.showModal()
  searchInput.value = ''
  renderResults([])
  searchStatus.textContent = 'Type to search the documentation.'
  searchInput.focus()
  loadSearch().catch(() => {
    searchStatus.textContent = 'Search is unavailable.'
  })
}

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const escapeHtml = (text) =>
  text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')

function highlightTerms(text, terms) {
  let html = escapeHtml(text)
  for (const term of terms) {
    if (term.length < 2) continue
    html = html.replace(new RegExp(`(${escapeRegExp(escapeHtml(term))})`, 'gi'), '<mark>$1</mark>')
  }
  return html
}

function renderResults(results, terms = []) {
  searchResults.innerHTML = ''
  selectedIndex = -1
  searchInput.setAttribute('aria-expanded', String(results.length > 0))
  searchInput.removeAttribute('aria-activedescendant')
  results.forEach((result, index) => {
    const item = document.createElement('li')
    item.setAttribute('role', 'presentation')
    const snippet = result.text ? result.text.slice(0, 160) : ''
    item.innerHTML =
      `<a href="${result.href}" id="search-result-${index}" role="option" aria-selected="false">` +
      `<span class="search-result-page">${escapeHtml(result.group)} › ${escapeHtml(result.page)}</span>` +
      `<span class="search-result-title">${highlightTerms(result.title, terms)}</span>` +
      (snippet ? `<span class="search-result-text">${highlightTerms(snippet, terms)}</span>` : '') +
      '</a>'
    item.addEventListener('mousemove', () => select(index))
    item.querySelector('a').addEventListener('click', () => searchDialog.close())
    searchResults.appendChild(item)
  })
}

const optionElements = () => searchResults.querySelectorAll('[role="option"]')

function select(index) {
  const options = optionElements()
  if (options.length === 0) return
  if (selectedIndex >= 0) options[selectedIndex]?.setAttribute('aria-selected', 'false')
  selectedIndex = (index + options.length) % options.length
  const option = options[selectedIndex]
  option.setAttribute('aria-selected', 'true')
  option.scrollIntoView({ block: 'nearest' })
  searchInput.setAttribute('aria-activedescendant', option.id)
}

searchInput.addEventListener('input', async () => {
  const query = searchInput.value.trim()
  if (!query) {
    renderResults([])
    searchStatus.textContent = 'Type to search the documentation.'
    return
  }
  await loadSearch()
  const results = searchEngine.search(query).slice(0, 12)
  renderResults(results, query.split(/\s+/))
  searchStatus.textContent = results.length
    ? `${results.length} result${results.length === 1 ? '' : 's'}`
    : `No results for “${query}”`
})

searchInput.addEventListener('keydown', (event) => {
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    select(selectedIndex + 1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    select(selectedIndex - 1)
  } else if (event.key === 'Enter') {
    event.preventDefault()
    const link = optionElements()[Math.max(selectedIndex, 0)]
    if (link) link.click()
  }
})

searchButton.addEventListener('click', openSearch)
searchClose.addEventListener('click', () => searchDialog.close())
searchDialog.addEventListener('click', (event) => {
  if (event.target === searchDialog) searchDialog.close()
})
// A non-empty type=search input swallows Escape to clear itself; always close instead.
searchDialog.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault()
    searchDialog.close()
  }
})

window.addEventListener('keydown', (event) => {
  const editing =
    /^(input|textarea|select)$/i.test(document.activeElement?.tagName ?? '') ||
    document.activeElement?.isContentEditable
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    searchDialog.open ? searchDialog.close() : openSearch()
  } else if (event.key === '/' && !editing && !searchDialog.open) {
    event.preventDefault()
    openSearch()
  }
})
