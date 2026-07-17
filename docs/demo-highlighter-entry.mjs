import { createHighlighterCoreSync } from 'shiki/core'
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript'
import githubLight from 'shiki/dist/themes/github-light.mjs'
import githubDark from 'shiki/dist/themes/github-dark.mjs'
import jsx from 'shiki/dist/langs/jsx.mjs'
import tsx from 'shiki/dist/langs/tsx.mjs'
import css from 'shiki/dist/langs/css.mjs'
import tsrxGrammar from './tsrx.tmLanguage.json'

let demoHighlighter = null

export function createDemoHighlighter() {
  if (demoHighlighter) return demoHighlighter

  const highlighter = createHighlighterCoreSync({
    themes: [githubLight, githubDark],
    langs: [
      jsx,
      tsx,
      css,
      { ...tsrxGrammar, embeddedLangs: ['jsx', 'tsx', 'css'], name: 'tsrx' },
    ],
    engine: createJavaScriptRegexEngine({ forgiving: true }),
  })

  demoHighlighter = {
    highlight(code, lang) {
      const language = highlighter.getLoadedLanguages().includes(lang) ? lang : 'text'
      let html = highlighter.codeToHtml(code, {
        lang: language,
        themes: { light: 'github-light', dark: 'github-dark' },
        defaultColor: false,
      })
      if (!html.includes('tabindex=')) html = html.replace('<pre ', '<pre tabindex="0" ')
      return html.replaceAll('--shiki-dark:#6A737D', '--shiki-dark:#8B949E')
    },
  }
  return demoHighlighter
}
