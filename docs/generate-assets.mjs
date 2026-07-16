// Deterministic SVG asset generation for the docs site. Run: node docs/generate-assets.mjs
import { writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const assetsDir = path.join(path.dirname(fileURLToPath(import.meta.url)), 'assets')

// ---------- hero band: violet field with radiating light streaks ----------
// Deterministic pseudo-random from a fixed seed so regeneration is stable.
function mulberry32(seed) {
  let a = seed
  return () => {
    a |= 0
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

const rand = mulberry32(20260716)
const width = 1600
const height = 720
const cx = width / 2
const cy = height * 0.62

let streaks = ''
for (let i = 0; i < 88; i++) {
  const angle = rand() * Math.PI * 2
  const inner = 140 + rand() * 190
  const length = 260 + rand() * 620
  const spread = 0.55 + rand() * 2.1
  const x1 = cx + Math.cos(angle) * inner
  const y1 = cy + Math.sin(angle) * inner * 0.62
  const x2 = cx + Math.cos(angle) * (inner + length)
  const y2 = cy + Math.sin(angle) * (inner + length) * 0.62
  const cool = rand() < 0.22
  const color = cool ? '#8be9fd' : '#e9d5ff'
  const opacity = (cool ? 0.10 : 0.07) + rand() * 0.10
  streaks += `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="${color}" stroke-width="${spread.toFixed(2)}" stroke-linecap="round" opacity="${opacity.toFixed(3)}"/>`
}

const rays = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
  <defs>
    <linearGradient id="base" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#4c1d95"/>
      <stop offset="0.5" stop-color="#5b21b6"/>
      <stop offset="1" stop-color="#3b0764"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.62" r="0.75">
      <stop offset="0" stop-color="#a78bfa" stop-opacity="0.55"/>
      <stop offset="0.45" stop-color="#7c3aed" stop-opacity="0.22"/>
      <stop offset="1" stop-color="#7c3aed" stop-opacity="0"/>
    </radialGradient>
    <filter id="soften" x="-10%" y="-10%" width="120%" height="120%">
      <feGaussianBlur stdDeviation="1.4"/>
    </filter>
  </defs>
  <rect width="${width}" height="${height}" fill="url(#base)"/>
  <g filter="url(#soften)">${streaks}</g>
  <rect width="${width}" height="${height}" fill="url(#glow)"/>
</svg>
`

// ---------- logo candidates for /logos ----------
const GRAD = `<linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
  <stop offset="0" stop-color="#a855f7"/>
  <stop offset="0.55" stop-color="#7c3aed"/>
  <stop offset="1" stop-color="#5b21b6"/>
</linearGradient>`

const wrap = (slug, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="OXC for TSRX logo candidate ${slug}">
  <defs>${GRAD}</defs>
  ${body}
</svg>
`

// Path-drawn @ centered on (32,32): inner ring, tail, open outer arc.
const AT_PATH = `<g fill="none" stroke="#fff" stroke-width="4.5" stroke-linecap="round">
    <circle cx="32" cy="32" r="7"/>
    <path d="M39 32 v4 a4.5 4.5 0 0 0 9 0 v-4 a16 16 0 1 0 -6 12.5"/>
  </g>`

// Path-drawn braces, ~30px tall, drawn around a given x.
const braceLeft = (x, s = 1) =>
  `<path fill="none" stroke="#fff" stroke-width="${4.2 * s}" stroke-linecap="round" stroke-linejoin="round"
     d="M${x + 6 * s} ${32 - 15 * s} c${-4.5 * s} 0 ${-6 * s} ${2.5 * s} ${-6 * s} ${6 * s} v${5 * s} c0 ${2.8 * s} ${-1.8 * s} ${4 * s} ${-4 * s} ${4 * s} c${2.2 * s} 0 ${4 * s} ${1.2 * s} ${4 * s} ${4 * s} v${5 * s} c0 ${3.5 * s} ${1.5 * s} ${6 * s} ${6 * s} ${6 * s}"/>`
const braceRight = (x, s = 1) =>
  `<path fill="none" stroke="#fff" stroke-width="${4.2 * s}" stroke-linecap="round" stroke-linejoin="round"
     d="M${x - 6 * s} ${32 - 15 * s} c${4.5 * s} 0 ${6 * s} ${2.5 * s} ${6 * s} ${6 * s} v${5 * s} c0 ${2.8 * s} ${1.8 * s} ${4 * s} ${4 * s} ${4 * s} c${-2.2 * s} 0 ${-4 * s} ${1.2 * s} ${-4 * s} ${4 * s} v${5 * s} c0 ${3.5 * s} ${-1.5 * s} ${6 * s} ${-6 * s} ${6 * s}"/>`

const SQUARE = '<rect width="64" height="64" rx="15" fill="url(#g)"/>'

const logoCandidates = [
  {
    slug: 'classic-text',
    name: 'Classic (current) — monospace @{',
    svg: wrap(
      'classic-text',
      `${SQUARE}
  <text x="31" y="43.5" font-family="Menlo, Consolas, ui-monospace, monospace" font-size="26" font-weight="700" fill="#ffffff" text-anchor="middle">@{</text>`,
    ),
  },
  {
    slug: 'at-mark',
    name: 'At mark — path-drawn @ on gradient',
    svg: wrap('at-mark', `${SQUARE}\n  ${AT_PATH}`),
  },
  {
    slug: 'at-brace',
    name: 'At + brace — @ with a closing brace',
    svg: wrap(
      'at-brace',
      `${SQUARE}
  <g transform="translate(-6 0)">${AT_PATH.replaceAll('32', '30').replace('stroke-width="4.5"', 'stroke-width="4.2"')}</g>
  ${braceRight(52, 0.82)}`,
    ),
  },
  {
    slug: 'braces-bolt',
    name: 'Braces + bolt — control flow between braces',
    svg: wrap(
      'braces-bolt',
      `<rect width="64" height="64" rx="15" fill="#241245"/>
  ${braceLeft(14, 0.9)}
  ${braceRight(50, 0.9)}
  <path fill="url(#g)" stroke="#c4b5fd" stroke-width="1" stroke-linejoin="round" d="M35.5 14 25 34.5 h6.5 L28.5 50 39 29.5 h-6.5 L35.5 14 Z"/>`,
    ),
  },
  {
    slug: 'hex-at',
    name: 'Hex crate — Rust hexagon with @',
    svg: wrap(
      'hex-at',
      `<path d="M32 4 55 17.2 v26.6 L32 57 9 43.8 V17.2 Z" fill="url(#g)" stroke="url(#g)" stroke-width="6" stroke-linejoin="round"/>
  <g transform="translate(32 32) scale(0.82) translate(-32 -32)">${AT_PATH}</g>`,
    ),
  },
  {
    slug: 'diamond-brace',
    name: 'Diamond — rotated square with brace',
    svg: wrap(
      'diamond-brace',
      `<rect x="12.5" y="12.5" width="39" height="39" rx="10" fill="url(#g)" transform="rotate(45 32 32)"/>
  ${braceLeft(29, 0.85)}`,
    ),
  },
  {
    slug: 'one-parse-ring',
    name: 'One-parse ring — open ring, single pass',
    svg: wrap(
      'one-parse-ring',
      `<path fill="none" stroke="url(#g)" stroke-width="9" stroke-linecap="round" d="M51.5 40.5 A21.5 21.5 0 1 1 49.5 19"/>
  <circle cx="32" cy="32" r="6.5" fill="url(#g)"/>
  <circle cx="53.5" cy="24.5" r="4.5" fill="#a855f7"/>`,
    ),
  },
  {
    slug: 'projection-stack',
    name: 'Projection — lifted layer over outline',
    svg: wrap(
      'projection-stack',
      `<rect x="17" y="7" width="40" height="40" rx="11" fill="none" stroke="url(#g)" stroke-width="3.5"/>
  <rect x="7" y="17" width="40" height="40" rx="11" fill="url(#g)"/>
  <g transform="translate(-5 5)">${braceLeft(24, 0.8)}${braceRight(40, 0.8)}</g>`,
    ),
  },
  {
    slug: 'dyn-tag',
    name: 'Dynamic tag — angle brackets around braces',
    svg: wrap(
      'dyn-tag',
      `${SQUARE}
  <g fill="none" stroke="#fff" stroke-width="4.2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M19 21 8.5 32 19 43"/>
    <path d="M45 21 55.5 32 45 43"/>
  </g>
  <g opacity="0.95">${braceLeft(24, 0.62)}${braceRight(40, 0.62)}</g>`,
    ),
  },
  {
    slug: 'prompt',
    name: 'Prompt — @ with a cursor',
    svg: wrap(
      'prompt',
      `${SQUARE}
  <g transform="translate(24 30) scale(0.62) translate(-32 -32)">${AT_PATH.replace('stroke-width="4.5"', 'stroke-width="6"')}</g>
  <rect x="37" y="40" width="12" height="5" rx="2.5" fill="#fff"/>`,
    ),
  },
]

import { mkdir } from 'node:fs/promises'
const logosDir = path.join(assetsDir, 'logos')
await mkdir(logosDir, { recursive: true })
for (const candidate of logoCandidates) {
  await writeFile(path.join(logosDir, `${candidate.slug}.svg`), candidate.svg)
}
await writeFile(
  path.join(logosDir, 'index.json'),
  JSON.stringify(
    logoCandidates.map(({ slug, name }, index) => ({ id: index + 1, slug, name })),
    null,
    2,
  ),
)

// The chosen site logo (picked from /logos): at-mark.
const logo = logoCandidates
  .find((candidate) => candidate.slug === 'at-mark')
  .svg.replace('logo candidate at-mark', 'OXC for TSRX logo')
await writeFile(path.join(assetsDir, 'logo.svg'), logo)
await writeFile(path.join(assetsDir, 'hero-rays.svg'), rays)
console.log(`wrote assets/logo.svg, assets/hero-rays.svg, and ${logoCandidates.length} candidates in assets/logos/`)
