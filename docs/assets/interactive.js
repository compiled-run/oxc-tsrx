// Interactive doc-page components (matrix filter, review route, editor
// replay). Loaded on demand by app.js only when a page contains one, so
// the home page never pays for it. Init is idempotent via data-ready.

function initMatrixFilters() {
  for (const filter of document.querySelectorAll('[data-matrix-filter]:not([data-ready])')) {
    filter.dataset.ready = '1'
    const chips = [...filter.querySelectorAll('[data-matrix-chip]')]
    const rows = [...filter.querySelectorAll('tr[data-classification]')]
    const status = filter.querySelector('[data-matrix-status]')
    const select = (slug) => {
      let shown = 0
      for (const chip of chips) chip.setAttribute('aria-pressed', String(chip.dataset.matrixChip === slug))
      for (const row of rows) {
        row.hidden = slug !== 'all' && row.dataset.classification !== slug
        if (!row.hidden) shown += 1
      }
      status.textContent =
        shown === rows.length
          ? `Showing all ${shown} responsibilities.`
          : `Showing ${shown} of ${rows.length} responsibilities.`
    }
    for (const chip of chips) chip.addEventListener('click', () => select(chip.dataset.matrixChip))
    select('all')
  }
}

function initReviewRoute() {
  for (const route of document.querySelectorAll('[data-review-route]:not([data-ready])')) {
    route.dataset.ready = '1'
    const checks = [...route.querySelectorAll('[data-review-check]')]
    const status = route.querySelector('[data-review-status]')
    const update = () => {
      let left = 0
      let done = 0
      for (const check of checks) {
        check.closest('.review-step').classList.toggle('review-step-done', check.checked)
        if (check.checked) done += 1
        else left += Number(check.dataset.minutes)
      }
      status.textContent =
        done === 0
          ? `About ${route.dataset.totalMinutes} minutes of reading for a full first pass.`
          : done === checks.length
            ? 'You have read the whole map.'
            : `${done} of ${checks.length} steps done. About ${left} minutes of reading left.`
    }
    for (const check of checks) check.addEventListener('change', update)
    update()
  }
}

function initEditorReplay(cleanupCallbacks) {
  for (const replay of document.querySelectorAll('[data-editor-replay]:not([data-ready])')) {
    replay.dataset.ready = '1'
    const button = replay.querySelector('[data-er-play]')
    const tabs = [...replay.querySelectorAll('[role="tab"]')]
    let timerId = null
    const stopReplay = () => {
      if (timerId !== null) clearTimeout(timerId)
      timerId = null
    }
    cleanupCallbacks.push(stopReplay)
    button.addEventListener('click', () => {
      stopReplay()
      if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
        const current = tabs.findIndex((tab) => tab.getAttribute('aria-selected') === 'true')
        tabs[(current + 1) % tabs.length].click()
        return
      }
      let index = 0
      const advance = () => {
        if (!replay.isConnected) return stopReplay()
        tabs[index++].click()
        if (index < tabs.length) timerId = setTimeout(advance, 2400)
        else {
          timerId = null
          button.textContent = 'Replay'
        }
      }
      advance()
    })
  }
}

export function init(cleanupCallbacks) {
  initMatrixFilters()
  initReviewRoute()
  initEditorReplay(cleanupCallbacks)
}
