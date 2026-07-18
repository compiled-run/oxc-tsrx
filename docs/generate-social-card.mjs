// Deterministic OG social card generation for the docs site.
// Run: node docs/generate-social-card.mjs
// Poster-style card built for thumbnail legibility: the logo and wordmark
// large in the center, TSRX @-syntax tokens as edge decoration. Rendered at
// 2x with system Chrome, then downscaled to 1200x630 so text stays crisp in
// link previews.
import { readFile, unlink } from 'node:fs/promises'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright-core'

const run = promisify(execFile)
const docsDir = path.dirname(fileURLToPath(import.meta.url))
const assetsDir = path.join(docsDir, 'assets')

const [interFont, groteskFont, logoSvg] = await Promise.all([
  readFile(path.join(assetsDir, 'fonts', 'inter-latin.woff2')),
  readFile(path.join(assetsDir, 'fonts', 'space-grotesk-latin.woff2')),
  readFile(path.join(assetsDir, 'logo.svg'), 'utf8'),
])

const fontFace = (family, data) => `
  @font-face {
    font-family: '${family}';
    src: url(data:font/woff2;base64,${data.toString('base64')}) format('woff2');
    font-weight: 100 900;
    font-display: block;
  }`

// Edge decorations: real TSRX syntax, kept away from the center so the
// wordmark owns the card. [text, x, y, size, opacity, rotation].
const tokens = [
  ['@if', 96, 92, 34, 0.5, -8],
  ['@for', 1010, 110, 40, 0.55, 6],
  ['@empty', 130, 508, 30, 0.42, 5],
  ['@switch', 962, 452, 32, 0.48, -5],
  ['@else', 60, 286, 26, 0.36, 0],
  ['@try', 1068, 288, 28, 0.42, 0],
  ['@case', 292, 52, 24, 0.32, 4],
  ['@pending', 836, 560, 26, 0.36, -4],
  ['@catch', 320, 566, 24, 0.32, 6],
  ['<{tag}>', 872, 44, 24, 0.32, -6],
  ['@{', 40, 420, 44, 0.30, 0],
  ['}', 1130, 180, 44, 0.28, 0],
  ['.tsrx', 1076, 552, 30, 0.5, 0],
]

const html = `<!doctype html>
<meta charset="utf-8">
<style>
  ${fontFace('Inter', interFont)}
  ${fontFace('Space Grotesk', groteskFont)}
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { width: 1200px; height: 630px; overflow: hidden; }
  .card {
    position: relative;
    width: 1200px;
    height: 630px;
    background: #0d0b14;
    font-family: 'Inter', sans-serif;
    overflow: hidden;
  }
  .glow {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(760px 520px at 50% 42%, rgba(124, 58, 237, 0.28), transparent 70%),
      radial-gradient(560px 400px at 6% 4%, rgba(91, 33, 182, 0.20), transparent 70%),
      radial-gradient(560px 400px at 96% 96%, rgba(91, 33, 182, 0.20), transparent 70%);
  }
  .grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(rgba(196, 181, 253, 0.05) 1px, transparent 1px),
      linear-gradient(90deg, rgba(196, 181, 253, 0.05) 1px, transparent 1px);
    background-size: 52px 52px;
    mask-image: radial-gradient(760px 560px at 50% 50%, transparent 34%, #000 78%);
    -webkit-mask-image: radial-gradient(760px 560px at 50% 50%, transparent 34%, #000 78%);
  }
  .tokens span {
    position: absolute;
    font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
    color: #c4b5fd;
  }
  .center {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 30px;
    z-index: 2;
  }
  .center svg {
    width: 128px;
    height: 128px;
    filter: drop-shadow(0 16px 48px rgba(124, 58, 237, 0.55));
  }
  .name {
    font-family: 'Space Grotesk', 'Inter', sans-serif;
    font-weight: 700;
    font-size: 104px;
    line-height: 1;
    letter-spacing: -0.03em;
    background: linear-gradient(115deg, #c4b5fd 15%, #e9d5ff 50%, #a78bfa 90%);
    background-clip: text;
    -webkit-background-clip: text;
    color: transparent;
  }
  .tagline {
    font-size: 30px;
    font-weight: 500;
    color: #b5b5ad;
  }
  .tagline b { color: #e9d5ff; font-weight: 600; }
</style>
<div class="card">
  <div class="glow"></div>
  <div class="grid"></div>
  <div class="tokens">${tokens
    .map(
      ([t, x, y, size, op, rot]) =>
        `<span style="left:${x}px;top:${y}px;font-size:${size}px;opacity:${op};transform:rotate(${rot}deg)">${String(
          t,
        )
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;')}</span>`,
    )
    .join('')}</div>
  <div class="center">
    ${logoSvg}
    <div class="name">OXC for TSRX</div>
    <div class="tagline">The full <b>OXC</b> toolchain in <b>.tsrx</b></div>
  </div>
</div>`

const browser = await chromium.launch({ channel: 'chrome', headless: true })
const page = await browser.newPage({
  viewport: { width: 1200, height: 630 },
  deviceScaleFactor: 2,
})
await page.setContent(html, { waitUntil: 'networkidle' })
await page.evaluate(() => document.fonts.ready)
const raw = path.join(assetsDir, 'social-card.raw.png')
await page.screenshot({ path: raw })
await browser.close()

// Downscale 2400x1260 -> 1200x630 so the file matches the og:image:width and
// og:image:height the site declares. Mild unsharp keeps text crisp.
const out = path.join(assetsDir, 'social-card.png')
await run('magick', [raw, '-resize', '1200x630', '-unsharp', '0x0.6+0.6+0.01', '-strip', out])
await unlink(raw)
console.log(`wrote ${out}`)
