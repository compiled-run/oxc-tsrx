// TSRX playground engine. Powers the home-page demo panel and the /playground
// page: editable overlay editor, live lint via the real oxc-tsrx binary,
// oxfmt formatting shown as a reviewable diff, opt-in type-aware runs, rule
// severity filters, Oxlint config, and shareable URL snippets.

const escapeHtml = (text) =>
  String(text)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
const validRuleName = (rule) => typeof rule === 'string' && /^[\w@/-]+$/u.test(rule)

// The API lives under the site's base path (e.g. /oxc-tsrx/api/...); this
// module lives at <base>/assets/, so resolve relative to the module URL.
const apiUrl = (endpoint) => new URL(`../api/${endpoint}`, import.meta.url)
const capabilitiesUrl = new URL('../demo-capabilities.json', import.meta.url)

async function api(endpoint, body) {
  const response = await fetch(apiUrl(endpoint), { method: 'POST', body })
  if (!response.ok) throw new Error(`API ${endpoint} failed`)
  return response.json()
}

function byteToCharIndex(text, byteOffset) {
  let bytes = 0
  let index = 0
  for (const ch of text) {
    if (bytes >= byteOffset) return index
    const cp = ch.codePointAt(0)
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4
    index += ch.length
  }
  return index
}

// ---- URL-hash sharing ----
const b64uEncode = (text) => {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '')
}
const b64uDecode = (encoded) => {
  const binary = atob(encoded.replaceAll('-', '+').replaceAll('_', '/'))
  const bytes = Uint8Array.from(binary, (ch) => ch.charCodeAt(0))
  return new TextDecoder().decode(bytes)
}

function readShareHash() {
  const params = new URLSearchParams(location.hash.slice(1))
  const state = {}
  try {
    if (params.get('code')) state.code = b64uDecode(params.get('code'))
    if (params.get('config')) state.config = b64uDecode(params.get('config'))
  } catch {}
  if (params.get('filters')) {
    state.filters = params
      .get('filters')
      .split(',')
      .map((entry) => {
        const [rule, severity] = entry.split(':')
        return { rule, severity }
      })
      .filter((filter) => validRuleName(filter.rule) && ['allow', 'warn', 'deny'].includes(filter.severity))
  }
  state.typeAware = params.get('ta') === '1'
  state.typeCheck = params.get('tc') === '1'
  return state
}

export async function initDemo(panel) {
  const hint = panel.querySelector('#demo-hint')
  const editor = panel.querySelector('#demo-editor')
  const pre = editor.querySelector('pre')
  const codeEl = pre.querySelector('code')
  const statusEl = panel.querySelector('#demo-status')
  const metaEl = panel.querySelector('#demo-meta')
  const shared = readShareHash()
  let health = null
  try {
    health = await (await fetch(capabilitiesUrl)).json()
  } catch {}
  if (!health?.ok || !health.native) {
    statusEl.textContent = 'pre-generated example · static preview'
    metaEl.textContent = 'native lint and format run only on the local development server'
    if (shared.code) {
      codeEl.innerHTML = shared.code
        .split('\n')
        .map((line) => `<span class="line">${escapeHtml(line)}</span>`)
        .join('\n')
      pre.setAttribute('aria-label', 'Static TSRX source preview')
      statusEl.textContent = 'shared source loaded · static preview'
    }
    if (hint) hint.textContent = 'static preview'
    return
  }

  const clientHighlighterPromise = import(new URL('./demo-highlighter.js', import.meta.url))
    .then((module) => module.createDemoHighlighter())
    .catch(() => null)

  const actions = panel.querySelector('#demo-actions')
  const formatButton = panel.querySelector('#demo-format')
  const resetButton = panel.querySelector('#demo-reset')
  const shareButton = panel.querySelector('#demo-share')
  const sidePanel = document.getElementById('pg-side')
  const original = pre.textContent

  // Engine options live in plain state; the clickable examples set them and
  // the footer explains which flags ran. No raw controls to decipher.
  const engineState = {
    typeAware: Boolean(shared.typeAware && health.typeAware),
    typeCheck: Boolean(shared.typeCheck && health.typeAware),
    config: shared.config,
    filters: shared.filters ?? [],
  }

  if (sidePanel) sidePanel.hidden = false
  const modeNote = document.getElementById('pg-mode-note')
  if (modeNote) modeNote.textContent = 'Connected to the loopback native development server.'

  const timesEl = document.createElement('span')
  timesEl.id = 'demo-times'
  timesEl.className = 'demo-times'
  timesEl.hidden = true
  panel.querySelector('.code-panel-status').append(timesEl)
  let highlightMs = null
  let compileMs = null
  const updateTimes = (hideHighlight = false) => {
    const parts = []
    if (highlightMs !== null && !hideHighlight) {
      parts.push(
        `highlighted in ${highlightMs < 10 ? highlightMs.toFixed(1) : Math.round(highlightMs)} ms`,
      )
    }
    if (compileMs !== null) parts.push(`compiled in ${Math.round(compileMs)} ms`)
    timesEl.textContent = parts.join(' · ')
    timesEl.hidden = parts.length === 0
  }

  // ---- measurements ----
  const preStyle = getComputedStyle(pre)
  const lineHeight = Number.parseFloat(preStyle.lineHeight)
  const padTop = Number.parseFloat(preStyle.paddingTop)
  const firstToken = codeEl.querySelector('.line > span')
  const gutterX = firstToken
    ? firstToken.getBoundingClientRect().left - pre.getBoundingClientRect().left
    : Number.parseFloat(preStyle.paddingLeft)
  const probe = document.createElement('span')
  probe.textContent = 'M'.repeat(20)
  probe.style.position = 'absolute'
  probe.style.visibility = 'hidden'
  codeEl.appendChild(probe)
  const charWidth = probe.getBoundingClientRect().width / 20
  probe.remove()

  // ---- overlay construction ----
  editor.classList.add('demo-active')
  pre.removeAttribute('tabindex')
  // The highlighted mirror is visual-only once the editable control exists;
  // exposing both would make assistive technology read the source twice.
  pre.setAttribute('aria-hidden', 'true')
  const diagLayer = document.createElement('div')
  diagLayer.className = 'demo-diags'
  diagLayer.setAttribute('aria-hidden', 'true')
  const textarea = document.createElement('textarea')
  textarea.className = 'demo-input'
  textarea.id = 'demo-input'
  textarea.value = original
  textarea.wrap = 'off'
  textarea.spellcheck = false
  textarea.autocapitalize = 'off'
  textarea.autocomplete = 'off'
  textarea.setAttribute('aria-label', 'Editable TSRX demo source')
  textarea.setAttribute('aria-autocomplete', 'list')
  let sourceGeneration = 0
  const escapeNote = document.createElement('span')
  escapeNote.className = 'visually-hidden'
  escapeNote.id = 'demo-escape-note'
  escapeNote.textContent =
    'Tab indents code inside this editor. Press Escape to move focus out.'
  textarea.setAttribute('aria-describedby', escapeNote.id)
  for (const [property, value] of [
    ['fontFamily', preStyle.fontFamily],
    ['fontSize', preStyle.fontSize],
    ['lineHeight', preStyle.lineHeight],
    ['paddingTop', `${padTop}px`],
    ['paddingLeft', `${gutterX}px`],
    ['paddingRight', preStyle.paddingRight],
    ['paddingBottom', preStyle.paddingBottom],
  ]) {
    textarea.style[property] = value
  }
  const tooltip = document.createElement('div')
  tooltip.className = 'demo-tooltip'
  tooltip.setAttribute('role', 'tooltip')
  tooltip.hidden = true
  const srDiagnostics = document.createElement('ul')
  srDiagnostics.className = 'visually-hidden'
  srDiagnostics.setAttribute('aria-label', 'Current lint diagnostics')
  editor.append(diagLayer, textarea)
  panel.append(tooltip, srDiagnostics, escapeNote)
  actions.hidden = false
  hint.textContent = health.completions ? 'edit me · real oxlint & oxfmt · TS completions on' : 'edit me · real oxlint & oxfmt'

  // In the playground workbench the editor fills its pane and scrolls both
  // axes internally; on the home page it grows with its content instead.
  const fillMode = Boolean(editor.closest('.pg-panes'))
  const syncSize = () => {
    if (fillMode) {
      textarea.style.height = '100%'
      textarea.style.overflowY = 'auto'
      return
    }
    textarea.style.height = `${pre.offsetHeight}px`
    editor.style.height = `${pre.offsetHeight}px`
  }
  syncSize()
  textarea.addEventListener('scroll', () => {
    const shift = `translate(${-textarea.scrollLeft}px, ${-textarea.scrollTop}px)`
    pre.style.transform = shift
    diagLayer.style.transform = shift
    hideTooltip()
  })

  // ---- rendering: synchronous client Shiki with the server path as fallback ----
  let mirrorLineHtml = [...codeEl.querySelectorAll(':scope > .line')].map((el) => el.outerHTML)
  let mirrorLines = original.split('\n')

  const adoptMirror = (text) => {
    mirrorLines = text.split('\n')
    mirrorLineHtml = [...codeEl.querySelectorAll(':scope > .line')].map((el) => el.outerHTML)
  }

  // Instant approximate tokens for lines being edited: same span shape and
  // palette as shiki, so there is no flash while the real re-highlight
  // (debounced, server-side) is in flight. The server stays source of truth.
  const QUICK_COLORS = {
    c: ['#6A737D', '#8B949E'],
    s: ['#032F62', '#9ECBFF'],
    d: ['#D73A49', '#F97583'],
    k: ['#D73A49', '#F97583'],
    n: ['#005CC5', '#79B8FF'],
    t: ['#22863A', '#85E89D'],
    f: ['#6F42C1', '#B392F0'],
    y: ['#005CC5', '#79B8FF'],
  }
  const QUICK_KINDS = ['c', 's', 'd', 'k', 'n', 't', 'f', 'y']
  const QUICK_RE =
    /(\/\/.*$)|("(?:[^"\\]|\\.)*"?|'(?:[^'\\]|\\.)*'?|`(?:[^`\\]|\\.)*`?)|(@\{|@[a-z]+)|(\b(?:export|import|from|function|return|const|let|var|type|interface|new|await|async|if|else|for|of|in|switch|case|default|try|catch)\b)|(\b\d+(?:\.\d+)?\b)|(<\/?[A-Za-z][\w.-]*|\/?>)|([a-z_$][\w$]*(?=\s*\())|([A-Z][\w$]*)/g
  const quickTokens = (line) => {
    let out = ''
    let last = 0
    for (const match of line.matchAll(QUICK_RE)) {
      out += escapeHtml(line.slice(last, match.index))
      const groupIndex = match.slice(1).findIndex((group) => group !== undefined)
      const [light, dark] = QUICK_COLORS[QUICK_KINDS[groupIndex]]
      out += `<span style="--shiki-light:${light};--shiki-dark:${dark}">${escapeHtml(match[0])}</span>`
      last = match.index + match[0].length
    }
    return out + escapeHtml(line.slice(last))
  }

  const syncMirror = (text) => {
    const newLines = text.split('\n')
    let start = 0
    while (
      start < newLines.length &&
      start < mirrorLines.length &&
      newLines[start] === mirrorLines[start]
    ) {
      start++
    }
    let endNew = newLines.length - 1
    let endOld = mirrorLines.length - 1
    while (endNew >= start && endOld >= start && newLines[endNew] === mirrorLines[endOld]) {
      endNew--
      endOld--
    }
    const html = newLines.map((line, index) => {
      if (index < start) return mirrorLineHtml[index]
      if (index > endNew) return mirrorLineHtml[endOld + (index - endNew)]
      return `<span class="line">${quickTokens(line)}</span>`
    })
    codeEl.innerHTML = html.join('\n')
    mirrorLines = newLines
    mirrorLineHtml = html
  }

  let highlightTimer = null
  let highlightGeneration = 0
  const rehighlight = (text) => {
    clearTimeout(highlightTimer)
    const generation = ++highlightGeneration
    highlightTimer = setTimeout(async () => {
      try {
        const { html } = await api('highlight', text)
        if (html && generation === highlightGeneration && textarea.value === text) {
          const doc = new DOMParser().parseFromString(html, 'text/html')
          const fresh = doc.querySelector('code')
          if (fresh) {
            codeEl.innerHTML = fresh.innerHTML
            adoptMirror(text)
          }
        }
      } catch {}
      syncSize()
    }, 200)
  }

  const CLIENT_HIGHLIGHT_CUTOFF = 32768
  let clientHighlighter = null
  let highlightFrame = null
  let latestHighlightText = original
  let composing = false

  const renderLegacy = (text) => {
    syncMirror(text)
    syncSize()
    rehighlight(text)
    updateTimes(text.length > CLIENT_HIGHLIGHT_CUTOFF)
  }

  const renderEditor = (text) => {
    latestHighlightText = text
    if (!clientHighlighter) {
      renderLegacy(text)
      return
    }
    if (composing) return
    if (text.length > CLIENT_HIGHLIGHT_CUTOFF) {
      if (highlightFrame !== null) cancelAnimationFrame(highlightFrame)
      highlightFrame = null
      renderLegacy(text)
      return
    }
    clearTimeout(highlightTimer)
    highlightGeneration += 1
    if (highlightFrame !== null) return
    highlightFrame = requestAnimationFrame(() => {
      highlightFrame = null
      const currentText = latestHighlightText
      if (!clientHighlighter || composing) return
      if (currentText.length > CLIENT_HIGHLIGHT_CUTOFF) {
        renderLegacy(currentText)
        return
      }
      try {
        const startedAt = performance.now()
        const html = clientHighlighter.highlight(currentText, 'tsrx')
        const doc = new DOMParser().parseFromString(html, 'text/html')
        const fresh = doc.querySelector('code')
        if (!fresh) throw new Error('client highlighter returned no code element')
        codeEl.innerHTML = fresh.innerHTML
        adoptMirror(currentText)
        syncSize()
        highlightMs = performance.now() - startedAt
        updateTimes()
      } catch {
        clientHighlighter = null
        editor.dataset.highlighter = 'server'
        highlightMs = null
        renderLegacy(currentText)
      }
    })
  }

  void clientHighlighterPromise.then((highlighter) => {
    if (!highlighter) return
    clearTimeout(highlightTimer)
    highlightGeneration += 1
    clientHighlighter = highlighter
    editor.dataset.highlighter = 'client'
    renderEditor(textarea.value)
  })

  // ---- diagnostics ----
  let segments = []
  const clearDiagnostics = () => {
    diagLayer.innerHTML = ''
    srDiagnostics.innerHTML = ''
    segments = []
  }
  const renderDiagnostics = (text, diagnostics) => {
    clearDiagnostics()
    for (const diagnostic of diagnostics) {
      for (const label of diagnostic.labels ?? []) {
        const start = byteToCharIndex(text, label.span.offset)
        const end = byteToCharIndex(text, label.span.offset + label.span.length)
        const before = text.slice(0, start)
        const line = (before.match(/\n/g) ?? []).length
        const col = start - (before.lastIndexOf('\n') + 1)
        const lineEnd = text.indexOf('\n', start)
        const clampedEnd = lineEnd === -1 ? end : Math.min(end, lineEnd)
        const segment = {
          x: gutterX + col * charWidth,
          y: padTop + line * lineHeight,
          w: Math.max((clampedEnd - start) * charWidth, charWidth * 0.8),
          h: lineHeight,
          severity: diagnostic.severity,
          code: diagnostic.code,
          message: diagnostic.message,
          line: line + 1,
        }
        segments.push(segment)
        const marker = document.createElement('div')
        marker.className = `demo-diag ${diagnostic.severity === 'error' ? 'error' : 'warning'}`
        marker.style.left = `${segment.x}px`
        marker.style.top = `${segment.y}px`
        marker.style.width = `${segment.w}px`
        marker.style.height = `${segment.h}px`
        diagLayer.appendChild(marker)
        const item = document.createElement('li')
        item.textContent = `${diagnostic.severity} ${diagnostic.code} on line ${segment.line}: ${diagnostic.message}`
        srDiagnostics.appendChild(item)
      }
    }
  }

  let formatStatusAt = 0
  const setStatus = (html, tone = 'ok') => {
    statusEl.innerHTML = html
    statusEl.dataset.tone = tone
    if (html.includes('oxfmt')) formatStatusAt = Date.now()
  }

  const lintOptions = () => ({
    typeAware: engineState.typeAware,
    typeCheck: engineState.typeCheck,
    config: engineState.config?.trim() || undefined,
    filters: engineState.filters.length ? engineState.filters : undefined,
  })

  // ---- live output panes (playground page): projection, structure,
  // diagnostics, formatted — all straight from the real engine.
  const outputPanel = document.getElementById('pg-output')
  const outputStatus = document.getElementById('pg-output-status')
  if (outputPanel) outputPanel.hidden = false

  const renderCodeInto = async (elementId, source, lang, generation) => {
    const target = document.getElementById(elementId)
    if (!target) return
    try {
      const { html } = await api('highlight', JSON.stringify({ source, lang }))
      if (generation === outputGeneration && html) {
        target.innerHTML = html
        return
      }
    } catch {}
    if (generation !== outputGeneration) return
    target.innerHTML = `<pre class="pg-plain"><code>${escapeHtml(source)}</code></pre>`
  }

  const lineOf = (text, byteOffset) => {
    const charIndex = byteToCharIndex(text, byteOffset)
    return (text.slice(0, charIndex).match(/\n/g) ?? []).length + 1
  }

  let outputTimer = null
  let outputGeneration = 0
  const refreshOutputs = (text, lintResult) => {
    if (!outputPanel) return
    clearTimeout(outputTimer)
    const generation = ++outputGeneration
    outputTimer = setTimeout(async () => {
      if (generation !== outputGeneration || textarea.value !== text) return
      const [projection, format] = await Promise.all([
        api('project', text).catch(() => ({ error: 'projection request failed' })),
        api('format', text).catch(() => ({ error: 'format request failed' })),
      ])
      if (generation !== outputGeneration || textarea.value !== text) return
      if (projection.error) {
        document.getElementById('pg-projected').innerHTML =
          `<p class="pg-note pg-output-error">✗ ${escapeHtml(projection.error)}</p>`
        document.getElementById('pg-structure').innerHTML = ''
      } else {
        void renderCodeInto('pg-projected', projection.projected, 'tsx', generation)
        const rows = projection.tokens
          .map(
            (token) =>
              `<tr><td><code>${escapeHtml(token.kind)}</code></td><td class="num">line ${lineOf(text, token.start)}</td><td class="num">bytes ${token.start}–${token.end}</td></tr>`,
          )
          .join('')
        document.getElementById('pg-structure').innerHTML =
          `<table class="pg-structure-table"><thead><tr><th>Token</th><th class="num">Line</th><th class="num">Span</th></tr></thead><tbody>${rows}</tbody></table>` +
          `<p class="pg-note">${projection.counts.controls} control${projection.counts.controls === 1 ? '' : 's'} · ${projection.counts.dynamicTags} dynamic tag${projection.counts.dynamicTags === 1 ? '' : 's'} · ${projection.counts.styleBlocks} raw style block${projection.counts.styleBlocks === 1 ? '' : 's'}</p>`
      }
      if (format.error) {
        document.getElementById('pg-formatted').innerHTML =
          `<p class="pg-note pg-output-error">✗ ${escapeHtml(format.error)}</p>`
      } else {
        void renderCodeInto('pg-formatted', format.formatted, 'tsrx', generation)
      }
      if (lintResult && !lintResult.error) {
        void renderCodeInto(
          'pg-diagnostics',
          JSON.stringify(lintResult.diagnostics, null, 2),
          'json',
          generation,
        )
      }
      if (outputStatus) {
        outputStatus.textContent = `engine output for the current source · ${new Date().toLocaleTimeString()}`
      }
    }, 450)
  }

  let lintTimer = null
  let lintGeneration = 0
  const lint = (text, immediate = false) => {
    clearTimeout(lintTimer)
    const generation = ++lintGeneration
    lintTimer = setTimeout(
      async () => {
        try {
          const options = lintOptions()
          const useJson =
            options.typeAware || options.typeCheck || options.config || options.filters
          const result = await api(
            'lint',
            useJson ? JSON.stringify({ source: text, ...options }) : text,
          )
          if (generation !== lintGeneration || textarea.value !== text) return
          if (result.error) {
            renderDiagnostics(text, [])
            setStatus(`✗ ${escapeHtml(result.error)}`, 'error')
            return
          }
          if (typeof result.elapsedMs === 'number') {
            compileMs = result.elapsedMs
            updateTimes(text.length > CLIENT_HIGHLIGHT_CUTOFF)
          }
          // Don't flag the word the caret is still inside: mid-typing
          // identifiers produce transient findings that read as noise.
          const caret = textarea.selectionStart
          let wordStart = caret
          while (wordStart > 0 && /[\w$]/.test(text[wordStart - 1])) wordStart--
          let wordEnd = caret
          while (wordEnd < text.length && /[\w$]/.test(text[wordEnd])) wordEnd++
          const active = document.activeElement === textarea && wordEnd > wordStart
          const visible = active
            ? result.diagnostics.filter(
                (d) =>
                  !(d.labels ?? []).some((label) => {
                    const start = byteToCharIndex(text, label.span.offset)
                    const end = byteToCharIndex(text, label.span.offset + label.span.length)
                    return start < wordEnd && end > wordStart
                  }),
              )
            : result.diagnostics
          renderDiagnostics(text, visible)
          const errors = result.diagnostics.filter((d) => d.severity === 'error').length
          const warnings = result.diagnostics.length - errors
          if (result.diagnostics.length === 0) {
            // Keep a just-shown format confirmation readable for a moment.
            if (Date.now() - formatStatusAt > 2500) {
              setStatus(
                '<span class="hp-ok" aria-hidden="true">✓</span> lint clean · oxlint found nothing',
                'ok',
              )
            }
          } else {
            const parts = []
            if (errors) parts.push(`${errors} error${errors === 1 ? '' : 's'}`)
            if (warnings) parts.push(`${warnings} warning${warnings === 1 ? '' : 's'}`)
            setStatus(
              `✗ oxlint: ${parts.join(' · ')} · hover the underline`,
              errors ? 'error' : 'warning',
            )
          }
          if (metaEl && result.parseCount !== null) {
            const bits = [`${result.parseCount} canonical parse`]
            if (result.ruleCount) bits.push(`${result.ruleCount} rules`)
            bits.push(result.typeAware ? 'type-aware' : 'diagnostics on original bytes')
            if (engineState.typeCheck) bits.push('--type-check')
            else if (engineState.typeAware) bits.push('--type-aware')
            if (engineState.config) bits.push('--config')
            for (const filter of engineState.filters) {
              bits.push(`-${{ allow: 'A', warn: 'W', deny: 'D' }[filter.severity]} ${filter.rule}`)
            }
            metaEl.textContent = bits.join(' · ')
          }
          refreshOutputs(text, result)
        } catch {}
      },
      immediate ? 0 : 350,
    )
  }
  const relint = () => lint(textarea.value, true)

  // ---- share (playground page) ----
  shareButton?.addEventListener('click', async () => {
    const params = new URLSearchParams()
    params.set('code', b64uEncode(textarea.value))
    if (engineState.config?.trim()) params.set('config', b64uEncode(engineState.config))
    if (engineState.filters.length) {
      params.set('filters', engineState.filters.map((f) => `${f.rule}:${f.severity}`).join(','))
    }
    if (engineState.typeAware) params.set('ta', '1')
    if (engineState.typeCheck) params.set('tc', '1')
    const url = `${location.origin}${location.pathname}#${params.toString()}`
    history.replaceState(null, '', `#${params.toString()}`)
    try {
      await navigator.clipboard.writeText(url)
      setStatus('<span class="hp-ok" aria-hidden="true">✓</span> share link copied to clipboard', 'ok')
    } catch {
      setStatus('✗ could not copy the share link', 'error')
    }
  })

  // ---- editor keys: auto-closing pairs and JSX tags mirror the Markless
  // VS Code extension (its language-configuration and autoClosingTags). ----
  const INDENT = '  '
  const insertText = (text) => document.execCommand('insertText', false, text)
  const PAIRS = { '(': ')', '[': ']', '{': '}', '"': '"', "'": "'", '`': '`' }
  const VOID_TAGS = new Set(
    'br,hr,img,input,meta,link,source,area,base,col,embed,param,track,wbr'.split(','),
  )

  // ---- @-directive completions: the Markless extension's snippet catalog.
  // Caret markers: | is the final caret position after insertion.
  const SNIPPETS = [
    ['@if', '@if (|) {\n  \n}'],
    ['@if-@else', '@if (|) {\n  \n} @else {\n  \n}'],
    ['@for-of', '@for (const item of |) {\n  \n}'],
    ['@for-index', '@for (const item of |; index i) {\n  \n}'],
    ['@for-key', '@for (const item of |; key item.id) {\n  \n}'],
    ['@for-@empty', '@for (const item of |) {\n  \n} @empty {\n  \n}'],
    ['@switch-@case', '@switch (|) {\n  @case match: {\n    \n  }\n}'],
    ['@try-@pending', '@try {\n  |\n} @pending {\n  \n}'],
    ['@try-@pending-@catch', '@try {\n  |\n} @pending {\n  \n} @catch (error) {\n  \n}'],
    ['@else', '@else {\n  |\n}'],
    ['@else if', '@else if (|) {\n  \n}'],
    ['@empty', '@empty {\n  |\n}'],
    ['@case', '@case |: {\n  \n}'],
    ['@default', '@default: {\n  |\n}'],
    ['@pending', '@pending {\n  |\n}'],
    ['@catch', '@catch (error) {\n  |\n}'],
    ['@{}', '@{\n  |\n}'],
  ]
  const completions = document.createElement('ul')
  completions.id = `${textarea.id}-completions`
  completions.className = 'demo-completions'
  completions.setAttribute('role', 'listbox')
  completions.setAttribute('aria-label', 'Code completions')
  completions.hidden = true
  editor.setAttribute('role', 'combobox')
  editor.setAttribute('aria-label', 'TSRX code editor completions')
  editor.setAttribute('aria-haspopup', 'listbox')
  editor.setAttribute('aria-expanded', 'false')
  editor.setAttribute('aria-controls', completions.id)
  textarea.setAttribute('aria-controls', completions.id)
  editor.appendChild(completions)
  let completionAnchor = -1
  let completionIndex = 0
  let completionMode = 'snippet'
  let tsEntries = []
  let tsTimer = null
  let completionGeneration = 0

  const hideCompletions = () => {
    completions.hidden = true
    editor.setAttribute('aria-expanded', 'false')
    textarea.removeAttribute('aria-activedescendant')
  }

  const closeCompletions = () => {
    hideCompletions()
    completionAnchor = -1
    clearTimeout(tsTimer)
    completionGeneration += 1
  }

  const activateCompletion = (index) => {
    const options = [...completions.querySelectorAll('[role="option"]')]
    if (options.length === 0) {
      hideCompletions()
      return
    }
    completionIndex = (index + options.length) % options.length
    options.forEach((option, optionIndex) => {
      option.setAttribute('aria-selected', String(optionIndex === completionIndex))
    })
    const active = options[completionIndex]
    textarea.setAttribute('aria-activedescendant', active.id)
    active.scrollIntoView({ block: 'nearest' })
  }

  const openCompletions = () => {
    if (!completions.querySelector('[role="option"]')) {
      hideCompletions()
      return
    }
    positionMenu()
    completions.hidden = false
    editor.setAttribute('aria-expanded', 'true')
    activateCompletion(0)
  }

  const TS_KIND_PRIORITY = { parameter: 0, 'local var': 0, property: 0, method: 0, const: 0, let: 0, function: 1, var: 2, alias: 3 }

  const acceptCompletion = (item) => {
    // Capture the anchor first: insertText fires an input event that resets it.
    const anchor = completionAnchor
    const { selectionStart: caret, value } = textarea
    if (item.dataset.name !== undefined) {
      textarea.setSelectionRange(anchor, caret)
      insertText(item.dataset.name)
      closeCompletions()
      return
    }
    const body = item.dataset.body
    const lineStart = value.lastIndexOf('\n', anchor - 1) + 1
    const indent = /^[ \t]*/.exec(value.slice(lineStart))[0]
    const caretMark = body.indexOf('|')
    const text = body.replace('|', '').replaceAll('\n', `\n${indent}`)
    textarea.setSelectionRange(anchor, caret)
    insertText(text)
    if (caretMark !== -1) {
      const extra = (body.slice(0, caretMark).match(/\n/g) ?? []).length * indent.length
      const position = anchor + caretMark + extra
      textarea.setSelectionRange(position, position)
    }
    closeCompletions()
  }

  const positionMenu = () => {
    const { value } = textarea
    const before = value.slice(0, completionAnchor)
    const line = (before.match(/\n/g) ?? []).length
    const col = completionAnchor - (before.lastIndexOf('\n') + 1)
    completions.style.left = `${Math.max(8, gutterX + col * charWidth - textarea.scrollLeft)}px`
    completions.style.top = `${padTop + (line + 1) * lineHeight - textarea.scrollTop}px`
  }

  const attachOptionHandlers = () => {
    for (const [index, item] of [...completions.querySelectorAll('[role="option"]')].entries()) {
      item.addEventListener('mouseenter', () => activateCompletion(index))
      item.addEventListener('mousedown', (event) => {
        event.preventDefault()
        activateCompletion(index)
        acceptCompletion(item)
      })
    }
  }

  const renderTsMenu = () => {
    if (completionMode !== 'ts' || completionAnchor === -1) return
    const { selectionStart: caret, value } = textarea
    const prefix = value.slice(completionAnchor, caret)
    if (caret < completionAnchor || !/^[\w$]*$/.test(prefix)) {
      closeCompletions()
      return
    }
    const lowered = prefix.toLowerCase()
    const matches = tsEntries
      .filter((entry) => entry.name.toLowerCase().startsWith(lowered))
      .sort(
        (a, b) =>
          (TS_KIND_PRIORITY[a.kind] ?? 9) - (TS_KIND_PRIORITY[b.kind] ?? 9) ||
          a.name.length - b.name.length,
      )
      .slice(0, 8)
    if (matches.length === 0) {
      // Keep the anchor: a fetch may still be in flight.
      hideCompletions()
      return
    }
    completions.innerHTML = matches
      .map(
        (entry, index) =>
          `<li role="option" id="${completions.id}-option-${index}" aria-selected="false" data-name="${escapeHtml(entry.name)}"><code>${escapeHtml(entry.name)}</code><span class="demo-completion-kind">${escapeHtml(entry.kind)}</span></li>`,
      )
      .join('')
    attachOptionHandlers()
    openCompletions()
  }

  // Real TypeScript completions from the type projection via /api/complete.
  const triggerTsCompletions = (refetch) => {
    const caret = textarea.selectionStart
    const { value } = textarea
    let start = caret
    while (start > 0 && /[\w$]/.test(value[start - 1])) start--
    const afterDot = value[start - 1] === '.'
    if (!afterDot && caret - start < 1) {
      if (completionMode === 'ts') closeCompletions()
      return
    }
    completionAnchor = start
    completionMode = 'ts'
    renderTsMenu()
    if (refetch || tsEntries.length === 0) {
      clearTimeout(tsTimer)
      const generation = ++completionGeneration
      const requestedSource = textarea.value
      const requestedOffset = textarea.selectionStart
      tsTimer = setTimeout(async () => {
        try {
          const result = await api(
            'complete',
            JSON.stringify({ source: requestedSource, offset: requestedOffset }),
          )
          if (
            generation !== completionGeneration ||
            completionMode !== 'ts' ||
            textarea.value !== requestedSource
          ) {
            return
          }
          tsEntries = result.entries ?? []
          if (tsEntries.length === 0) {
            // Grammar may be mid-edit and fail closed: fall back to
            // word suggestions from this file, like editors do.
            const words = [...new Set(textarea.value.match(/[A-Za-z_$][\w$]{2,}/g) ?? [])]
            tsEntries = words.map((name) => ({ name, kind: 'in file' }))
          }
          // Re-derive the anchor from the current caret; renders may have
          // hidden the menu while the request was in flight.
          const caretNow = textarea.selectionStart
          let startNow = caretNow
          while (startNow > 0 && /[\w$]/.test(textarea.value[startNow - 1])) startNow--
          completionAnchor = startNow
          renderTsMenu()
        } catch {}
      }, 150)
    }
  }

  const updateCompletions = () => {
    if (completionAnchor === -1) return
    const { selectionStart: caret, value } = textarea
    const typed = value.slice(completionAnchor, caret)
    if (caret < completionAnchor || !/^@[\w{-]*$/.test(typed)) {
      closeCompletions()
      return
    }
    const matches = SNIPPETS.filter(([name]) => name.startsWith(typed)).slice(0, 8)
    if (matches.length === 0) {
      closeCompletions()
      return
    }
    completions.innerHTML = matches
      .map(
        ([name, body], index) =>
          `<li role="option" id="${completions.id}-option-${index}" aria-selected="false" data-body="${escapeHtml(body)}"><code>${escapeHtml(name)}</code></li>`,
      )
      .join('')
    attachOptionHandlers()
    openCompletions()
  }

  textarea.addEventListener('keydown', (event) => {
    if (event.key === ' ' && event.ctrlKey) {
      event.preventDefault()
      triggerTsCompletions(true)
      return
    }
    if (!completions.hidden) {
      const options = [...completions.querySelectorAll('li')]
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault()
        activateCompletion(completionIndex + (event.key === 'ArrowDown' ? 1 : -1))
        return
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        event.preventDefault()
        acceptCompletion(options[completionIndex])
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        closeCompletions()
        return
      }
    }
    if (event.key === 'Escape') {
      event.preventDefault()
      textarea.blur()
      return
    }
    if (PAIRS[event.key] && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const { selectionStart: start, selectionEnd: end, value } = textarea
      if (start !== end) {
        // Surround the selection, like VS Code's surroundingPairs.
        event.preventDefault()
        const selection = value.slice(start, end)
        insertText(event.key + selection + PAIRS[event.key])
        textarea.setSelectionRange(start + 1, start + 1 + selection.length)
        return
      }
      const next = value[start] ?? ''
      const isQuote = '\'"`'.includes(event.key)
      if (isQuote && next === event.key) {
        event.preventDefault() // skip over the existing closing quote
        textarea.setSelectionRange(start + 1, start + 1)
        return
      }
      if (next === '' || /[\s)\]};,.>]/.test(next)) {
        if (isQuote && /[\w'"`]/.test(value[start - 1] ?? '')) return
        event.preventDefault()
        insertText(event.key + PAIRS[event.key])
        textarea.setSelectionRange(start + 1, start + 1)
        return
      }
      return
    }
    if (')]}'.includes(event.key)) {
      const { selectionStart: start, selectionEnd: end, value } = textarea
      if (start === end && value[start] === event.key) {
        event.preventDefault() // skip over the auto-inserted closer
        textarea.setSelectionRange(start + 1, start + 1)
        return
      }
    }
    if (event.key === 'Backspace') {
      const { selectionStart: start, selectionEnd: end, value } = textarea
      if (start === end && PAIRS[value[start - 1]] === value[start]) {
        event.preventDefault() // delete an empty pair together
        textarea.setSelectionRange(start - 1, start + 1)
        insertText('')
        return
      }
    }
    if (event.key === 'Tab') {
      event.preventDefault()
      const { selectionStart: start, selectionEnd: end, value } = textarea
      const multiline = value.slice(start, end).includes('\n')
      if (!event.shiftKey && !multiline) {
        insertText(INDENT)
        return
      }
      const lineStart = value.lastIndexOf('\n', start - 1) + 1
      const lastLineBreak = value.indexOf('\n', Math.max(end - 1, lineStart))
      const blockEnd = lastLineBreak === -1 ? value.length : lastLineBreak
      const block = value.slice(lineStart, blockEnd)
      const updated = event.shiftKey
        ? block.replace(/^ {1,2}/gm, '')
        : block.replace(/^/gm, INDENT)
      if (updated === block) return
      textarea.setSelectionRange(lineStart, blockEnd)
      insertText(updated)
      textarea.setSelectionRange(lineStart, lineStart + updated.length)
      return
    }
    if (event.key === 'Enter' && !event.isComposing) {
      event.preventDefault()
      const { selectionStart: start, value } = textarea
      const lineStart = value.lastIndexOf('\n', start - 1) + 1
      const indent = /^[ \t]*/.exec(value.slice(lineStart, start))[0]
      const extra = '{(['.includes(value[start - 1]) ? INDENT : ''
      insertText(`\n${indent}${extra}`)
    }
  })

  const applySource = (text) => {
    sourceGeneration += 1
    textarea.value = text
    clearDiagnostics()
    hideTooltip()
    renderEditor(text)
    lint(text, true)
  }

  textarea.addEventListener('input', (event) => {
    sourceGeneration += 1
    // Auto-closing JSX tags, like the Markless extension's autoClosingTags.
    if (event.data === '>' || event.data === '/') {
      const { selectionStart: caret, value } = textarea
      const before = value.slice(0, caret)
      if (event.data === '>') {
        const open = /<([A-Za-z][\w.-]*)(\s[^<>]*)?>$/.exec(before)
        if (
          open &&
          !(open[2] ?? '').trimEnd().endsWith('/') &&
          !VOID_TAGS.has(open[1].toLowerCase()) &&
          !value.slice(caret).startsWith(`</${open[1]}`)
        ) {
          insertText(`</${open[1]}>`)
          textarea.setSelectionRange(caret, caret)
        }
      } else if (before.endsWith('</')) {
        const opens = [...before.matchAll(/<([A-Za-z][\w.-]*)(?:\s[^<>]*?)?(?<!\/)>/g)].map(
          (m) => m[1],
        )
        const closes = [...before.matchAll(/<\/([A-Za-z][\w.-]*)>/g)].map((m) => m[1])
        for (const name of closes) {
          const last = opens.lastIndexOf(name)
          if (last !== -1) opens.splice(last, 1)
        }
        const unclosed = opens.at(-1)
        if (unclosed) insertText(`${unclosed}>`)
      }
    }
    if (event.data === '@') {
      completionMode = 'snippet'
      completionAnchor = textarea.selectionStart - 1
      updateCompletions()
    } else if (completionMode === 'snippet' && completionAnchor !== -1) {
      updateCompletions()
    } else if (health.completions && event.data && /[\w$.]/.test(event.data)) {
      triggerTsCompletions(event.data === '.' || completionAnchor === -1)
    } else if (completionMode === 'ts' && completionAnchor !== -1) {
      renderTsMenu()
    }
    const text = textarea.value
    clearDiagnostics()
    hideTooltip()
    renderEditor(text)
    lint(text)
  })
  textarea.addEventListener('compositionstart', () => {
    composing = true
  })
  textarea.addEventListener('compositionend', () => {
    composing = false
    renderEditor(textarea.value)
  })
  textarea.addEventListener('blur', closeCompletions)

  // ---- tooltip ----
  let quickInfoTimer = null
  let quickInfoLast = -1
  let quickInfoGeneration = 0
  const hideTooltip = () => {
    tooltip.hidden = true
    clearTimeout(quickInfoTimer)
    quickInfoLast = -1
    quickInfoGeneration += 1
  }
  // Hover type info (TypeScript quick info via the type projection).
  const showQuickInfo = (wordStart, offset, clientX, clientY) => {
    clearTimeout(quickInfoTimer)
    const generation = ++quickInfoGeneration
    const requestedSource = textarea.value
    quickInfoTimer = setTimeout(async () => {
      try {
        const { info } = await api(
          'quickinfo',
          JSON.stringify({ source: requestedSource, offset }),
        )
        if (
          !info ||
          generation !== quickInfoGeneration ||
          quickInfoLast !== wordStart ||
          textarea.value !== requestedSource
        ) {
          return
        }
        tooltip.innerHTML =
          `<span class="demo-tooltip-rule"><code>${escapeHtml(info.display)}</code></span>` +
          (info.docs ? `<span class="demo-tooltip-message">${escapeHtml(info.docs)}</span>` : '')
        tooltip.hidden = false
        tooltip.style.left = `${Math.min(Math.max(8, clientX - 20), window.innerWidth - tooltip.offsetWidth - 8)}px`
        tooltip.style.top = `${Math.max(8, clientY - tooltip.offsetHeight - 14)}px`
      } catch {}
    }, 260)
  }
  textarea.addEventListener('mousemove', (event) => {
    const rect = editor.getBoundingClientRect()
    const x = event.clientX - rect.left + textarea.scrollLeft
    const y = event.clientY - rect.top + textarea.scrollTop
    const hit = segments.find(
      (segment) =>
        x >= segment.x && x <= segment.x + segment.w && y >= segment.y && y <= segment.y + segment.h,
    )
    if (!hit) {
      // No diagnostic under the cursor: try TypeScript hover type info.
      if (health.completions) {
        const line = Math.floor((y - padTop) / lineHeight)
        const col = Math.round((x - gutterX) / charWidth)
        const lines = textarea.value.split('\n')
        if (line >= 0 && line < lines.length && col >= 0 && col <= lines[line].length) {
          const offset = lines.slice(0, line).reduce((n, l) => n + l.length + 1, 0) + col
          const ch = textarea.value[offset] ?? ''
          if (/[\w$]/.test(ch)) {
            // Same word: keep the visible tooltip instead of flickering.
            let wordStart = offset
            while (wordStart > 0 && /[\w$]/.test(textarea.value[wordStart - 1])) wordStart--
            if (wordStart !== quickInfoLast) {
              hideTooltip()
              quickInfoLast = wordStart
              showQuickInfo(wordStart, offset, event.clientX, event.clientY)
            }
            return
          }
        }
        hideTooltip()
      }
      hideTooltip()
      return
    }
    tooltip.innerHTML =
      `<span class="demo-tooltip-rule"><code>${escapeHtml(hit.code)}</code> · ${escapeHtml(hit.severity)}</span>` +
      `<span class="demo-tooltip-message">${escapeHtml(hit.message)}</span>`
    tooltip.hidden = false
    // Fixed positioning, anchored directly above the underline segment.
    const segmentLeft = rect.left + hit.x - textarea.scrollLeft
    const segmentTop = rect.top + hit.y - textarea.scrollTop
    tooltip.style.left = `${Math.min(Math.max(8, segmentLeft), window.innerWidth - tooltip.offsetWidth - 8)}px`
    tooltip.style.top = `${Math.max(8, segmentTop - tooltip.offsetHeight - 8)}px`
  })
  textarea.addEventListener('mouseleave', hideTooltip)
  window.addEventListener('scroll', hideTooltip, { passive: true })

  const doFormat = async () => {
    if (formatButton) formatButton.disabled = true
    const requestedSource = textarea.value
    const generation = sourceGeneration
    try {
      const result = await api('format', requestedSource)
      if (sourceGeneration !== generation || textarea.value !== requestedSource) return
      if (result.error) {
        setStatus(`✗ oxfmt: ${escapeHtml(result.error)}`, 'error')
      } else if (result.formatted === requestedSource) {
        setStatus(
          '<span class="hp-ok" aria-hidden="true">✓</span> already converged · oxfmt changed nothing',
          'ok',
        )
      } else {
        applySource(result.formatted)
        setStatus('<span class="hp-ok" aria-hidden="true">✓</span> formatted by oxfmt', 'ok')
      }
    } catch {
      setStatus('✗ format request failed', 'error')
    } finally {
      if (formatButton) formatButton.disabled = false
    }
  }
  formatButton?.addEventListener('click', doFormat)

  resetButton?.addEventListener('click', () => {
    applySource(original)
  })

  // ---- clickable examples: each loads a variant, sets the engine flags
  // internally, and explains what ran. This replaces raw checkboxes/inputs.
  const lintVariant = () =>
    original.replace(
      'const pending = tasks.filter((task) => !task.done);',
      'var total = 0;\n  debugger;\n  const pending = tasks.filter((task) => !task.done);',
    )
  const scenarios = {
    clean: {
      make: () => original,
      note: 'The converged example: lint clean, format converged.',
    },
    lint: {
      make: lintVariant,
      note: 'Added var + debugger: hover the underlines to see the real oxlint findings.',
    },
    messy: {
      make: () =>
        original.replaceAll(' = ', '=').replaceAll('  <', '        <').replaceAll('};', '}   ;'),
      note: 'Loaded with mangled spacing, then formatted by the real oxc-tsrx-fmt a moment later.',
      autoFormat: true,
    },
    types: {
      make: () => original.replace('{task.label}', '{task.titel}'),
      state: { typeAware: true, typeCheck: true },
      note: 'task.titel is a typo. Ran with --type-check: the real TypeScript compiler flags it.',
    },
    silence: {
      make: lintVariant,
      state: {
        filters: [
          { rule: 'no-debugger', severity: 'allow' },
          { rule: 'no-unused-vars', severity: 'allow' },
        ],
      },
      note: 'Same findings as "Lint findings", but ran with -A no-debugger -A no-unused-vars: the CLI severity flags silence them.',
    },
    config: {
      make: () =>
        original.replace('const pending', "console.log('debug');\n  const pending"),
      state: { config: '{ "rules": { "no-console": "error" } }' },
      note: 'console.log becomes an error via --config { "rules": { "no-console": "error" } }.',
    },
  }
  const scenarioNote = document.getElementById('pg-scenario-note')
  let autoFormatTimer = null
  for (const [name, scenario] of Object.entries(scenarios)) {
    document.getElementById(`pg-scenario-${name}`)?.addEventListener('click', () => {
      clearTimeout(autoFormatTimer)
      engineState.typeAware = false
      engineState.typeCheck = false
      engineState.config = undefined
      engineState.filters = []
      let note = scenario.note
      if (scenario.state) {
        if ((scenario.state.typeAware || scenario.state.typeCheck) && !health.typeAware) {
          note = 'Type-aware runs need oxlint-tsgolint on the local server; loaded the variant with syntax lint only.'
        } else {
          Object.assign(engineState, scenario.state)
        }
      }
      applySource(scenario.make())
      if (scenario.autoFormat) {
        autoFormatTimer = setTimeout(doFormat, 900)
      }
      if (scenarioNote) scenarioNote.textContent = note
    })
  }

  // ---- boot: shared code from the URL, then an initial real lint ----
  if (shared.code && shared.code !== original) {
    applySource(shared.code)
  } else {
    lint(original, true)
  }
}
