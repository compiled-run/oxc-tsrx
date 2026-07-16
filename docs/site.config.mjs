// Site configuration for the static docs generator (docs/build.mjs).
export default {
  title: 'OXC for TSRX',
  description:
    'Rust-native linting and formatting for .tsrx source through canonical OXC. No fork, no patches, one parse.',
  origin: 'https://thejackshelton.github.io',
  // Root-absolute base path the site is served under, with trailing slash.
  base: '/oxc-tsrx/',
  repository: 'https://github.com/thejackshelton/oxc-tsrx',
  nav: [
    { text: 'Guide', link: '/guide/introduction.html' },
    { text: 'Playground', link: '/playground.html' },
    { text: 'Integrations', link: '/integrations/configuration.html' },
    { text: 'Architecture', link: '/architecture/rust-oxc-core.html' },
    { text: 'Reference', link: '/reference/cli.html' },
    { text: 'GitHub', link: 'https://github.com/thejackshelton/oxc-tsrx' },
  ],
  sidebar: [
    {
      text: 'Guide',
      items: [
        { text: 'Introduction', link: '/guide/introduction.html' },
        { text: 'Getting Started', link: '/guide/getting-started.html' },
        { text: 'TSRX Syntax Support', link: '/guide/tsrx-syntax.html' },
        { text: 'Linting', link: '/guide/linting.html' },
        { text: 'Formatting', link: '/guide/formatting.html' },
      ],
    },
    {
      text: 'Integrations',
      items: [
        { text: 'Configuration', link: '/integrations/configuration.html' },
        { text: 'Vite and Vite+', link: '/integrations/vite-plus.html' },
        { text: 'Editor', link: '/integrations/editor.html' },
      ],
    },
    {
      text: 'Architecture',
      items: [
        { text: 'Rust/OXC Core', link: '/architecture/rust-oxc-core.html' },
      ],
    },
    {
      text: 'Reference',
      items: [
        { text: 'CLI', link: '/reference/cli.html' },
        { text: 'Benchmarks', link: '/reference/benchmarks.html' },
        { text: 'Limitations', link: '/reference/limitations.html' },
      ],
    },
  ],
  hero: {
    name: 'OXC for TSRX',
    text: 'Native OXC lint and format across the Vite toolchain',
    tagline:
      'Real Oxlint rules and real Oxfmt formatting for your .tsrx files. No fork, no patches, one parse.',
    actions: [
      { theme: 'brand', text: 'Get Started', link: '/guide/getting-started.html' },
      { theme: 'alt', text: 'What is OXC for TSRX?', link: '/guide/introduction.html' },
    ],
  },
  features: [
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 8.5v7a2 2 0 0 1-1 1.73l-6 3.5a2 2 0 0 1-2 0l-6-3.5A2 2 0 0 1 5 15.5v-7a2 2 0 0 1 1-1.73l6-3.5a2 2 0 0 1 2 0l6 3.5a2 2 0 0 1 1 1.73Z"/><path d="m5.3 7.3 7.7 4.5 7.7-4.5M13 21.5V11.8"/></svg>',
      title: 'Non-fork by construction',
      details:
        'One adapter crate pins canonical OXC at an exact commit. No source snapshot, no Cargo patch, no downstream patch queue.',
    },
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M10 9.5 12.5 8v8"/></svg>',
      title: 'Exactly one parse',
      details:
        'A byte-oriented scan, one legal-TSX projection allocation, and a single canonical OXC parse. Even dynamic tags validate against that same AST.',
    },
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r="0.5" fill="currentColor"/></svg>',
      title: 'Diagnostics on your bytes',
      details:
        'Real OXC lint diagnostics map through affine identity segments to original UTF-8 TSRX spans. Labels that would touch scaffolding are suppressed, never faked.',
    },
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 20V8m0 0L3.5 11.5M7 8l3.5 3.5M17 4v12m0 0 3.5-3.5M17 16l-3.5-3.5"/></svg>',
      title: 'Checked formatting lift',
      details:
        'Canonical Oxfmt layout is lifted back to TSRX in one indexed forward pass, then rescanned to the same structural fingerprint before any write.',
    },
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13 2 4.5 13.5H11L9.5 22 19 10h-6.5L13 2Z"/></svg>',
      title: 'Fast by contract',
      details:
        'Every headline number above is rebuilt from a committed, identity-checked benchmark report. The applicable frozen ratio and absolute-budget gates fail the release when performance regresses.',
    },
    {
      icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2.5 20 6v5.5c0 4.7-3.2 8.6-8 10-4.8-1.4-8-5.3-8-10V6l8-3.5Z"/><path d="M12 8v4.5m0 3.5v.01"/></svg>',
      title: 'Fail-closed honesty',
      details:
        'Unimplemented grammar and unsupported configuration fail loudly before any parse or write. Nothing is silently dropped to look fast.',
    },
  ],
  footer: {
    disclaimer:
      'OXC for TSRX is an independent community project: not affiliated with, endorsed by, or a product of VoidZero or the OXC team. It consumes canonical OXC without forking it.',
    copyright: 'MIT Licensed',
  },
}
