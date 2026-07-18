import { mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { D2 } from '@terrastruct/d2'

const docsDir = path.dirname(fileURLToPath(import.meta.url))
const sourceDir = path.join(docsDir, 'diagrams')
const outputDir = path.join(docsDir, 'assets', 'diagrams')
const colorTokens = new Map([
  ['#FFFFFF', '--dg-surface'],
  ['#F7F8FE', '--dg-accent-soft'],
  ['#0A0F25', '--dg-text'],
  ['#0D32B2', '--dg-edge'],
])

function optimizeInlineSvg(svg, name, seenColors) {
  // D2 emits direct presentation attributes on this simple shape set, plus two
  // repeated style blocks. One embeds a WOFF font and the other repeats theme
  // classes. The site CSS supplies the small shared shape and font rules instead.
  const stylePattern = /<style type="text\/css"><!\[CDATA\[[\s\S]*?<\/style>/g
  if ((svg.match(stylePattern) ?? []).length !== 2) {
    throw new Error(`Expected two removable D2 style blocks in ${name}`)
  }
  return svg
    .replace(stylePattern, '')
    .replace(/\b(fill|stroke)="(#[0-9A-Fa-f]{3,8})"/g, (attribute, property, color) => {
      const normalized = color.toUpperCase()
      const token = colorTokens.get(normalized)
      if (!token) {
        throw new Error(`Unexpected D2 ${property} color ${color} in ${name}`)
      }
      seenColors.add(normalized)
      return `${property}="var(${token})"`
    })
    .replace(/\s+/g, ' ')
    .replace(/> </g, '><')
    .trim()
}

async function renderDiagrams() {
  const entries = (await readdir(sourceDir)).filter((entry) => entry.endsWith('.d2')).sort()
  const d2 = new D2()
  const rendered = new Map()
  const seenColors = new Set()

  for (const entry of entries) {
    const name = path.basename(entry, '.d2')
    const source = await readFile(path.join(sourceDir, entry), 'utf8')
    const { diagram } = await d2.compile(source, { layout: 'elk' })
    const svg = await d2.render(diagram, {
      themeID: 0,
      sketch: false,
      pad: 24,
      salt: `${name}-light`,
      noXMLTag: true,
    })
    rendered.set(name, `${optimizeInlineSvg(svg, name, seenColors)}\n`)
  }

  const missingColors = [...colorTokens.keys()].filter((color) => !seenColors.has(color))
  if (missingColors.length > 0) {
    throw new Error(`Expected D2 colors were not emitted: ${missingColors.join(', ')}`)
  }

  await rm(outputDir, { recursive: true, force: true })
  await mkdir(outputDir, { recursive: true })
  for (const [name, svg] of rendered) {
    await writeFile(path.join(outputDir, `${name}.svg`), svg)
  }
}

renderDiagrams().then(
  () => process.exit(0),
  (error) => {
    console.error(error)
    process.exit(1)
  },
)
