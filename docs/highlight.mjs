// Shared shiki highlighter for the docs build and the dev-server demo API.
// TSRX highlighting follows the octane/ripple docs sites: shiki with the TSRX
// TextMate grammar, embedded jsx/tsx/css islands enabled.
import { readFile } from 'node:fs/promises'
import { createHighlighter } from 'shiki'

const FENCE_ALIASES = { sh: 'shellscript', bash: 'shellscript', js: 'javascript', ts: 'typescript' }

let highlighterPromise = null

export function getDocsHighlighter() {
  highlighterPromise ??= (async () => {
    const tsrxGrammar = JSON.parse(
      await readFile(new URL('./tsrx.tmLanguage.json', import.meta.url), 'utf8'),
    )
    const highlighter = await createHighlighter({
      themes: ['github-light', 'github-dark'],
      langs: [
        'javascript',
        'typescript',
        'jsx',
        'tsx',
        'css',
        'shellscript',
        'json',
        'jsonc',
        { ...tsrxGrammar, embeddedLangs: ['jsx', 'tsx', 'css'], name: 'tsrx' },
      ],
    })
    return highlighter
  })()
  return highlighterPromise
}

export function highlightWith(highlighter, code, lang) {
  const requested = FENCE_ALIASES[lang] ?? lang
  const language = highlighter.getLoadedLanguages().includes(requested) ? requested : 'text'
  let html = highlighter.codeToHtml(code, {
    lang: language,
    themes: { light: 'github-light', dark: 'github-dark' },
    defaultColor: false,
  })
  if (!html.includes('tabindex=')) html = html.replace('<pre ', '<pre tabindex="0" ')
  // github-dark's comment gray fails WCAG contrast on our darker code
  // background; lift it to GitHub's brighter comment gray.
  return html.replaceAll('--shiki-dark:#6A737D', '--shiki-dark:#8B949E')
}

export async function highlightHtml(code, lang) {
  return highlightWith(await getDocsHighlighter(), code, lang)
}
