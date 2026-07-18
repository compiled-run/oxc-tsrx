# T999 terminal audit attempt 1

## Verdict

`not_complete` for committed design `3b090fb7fdca4a91ebc982e561e42c7ee3582bf4`.
The commit, branch, scope, structure, links, and design-only boundary passed. Three semantic
contradictions remain and must be corrected together before T999 is repeated.

## Required corrections

1. **Exact compatibility declarations (UF-05, UF-06, UF-11; T005-R01).** The design incorrectly
   imports and re-exports `CommentWithLocation` from the `./types` surface, adds
   `Readonly<ParseOptions>`, and names but does not freeze the inherited Volar mapping and plugin
   action fields. Match exact `@tsrx/core@0.1.32`: root exports only the three consumed functions;
   `CommentWithLocation` is reachable through the augmented ESTree namespace; `ParseOptions` is
   mutable; and the declarations explicitly include `DefinitionLocation`,
   `PluginActionOverrides`, the six Volar `CodeInformation` feature fields, and all five mapping
   record fields including required `generatedLengths`.
2. **Independent native package lanes (UF-08, UF-10, UF-13; T005-R04/R08).** The design promises
   that compatibility CSS failure withholds only the facade, but also requires both addons in each
   canonical native tarball. Keep the existing eight `@oxc-tsrx/native-<target>` packages for the
   three executables plus `parser.node`; add eight independently gated
   `@oxc-tsrx/tsrx-core-compat-native-<target>` packages containing only
   `tsrx_core_compat.node`. The parser loader never installs or resolves the CSS/recovery family.
   Use role-discriminated canonical transport ABI versus compatibility facade ABI records and
   separate install-size totals.
3. **Literal recovery oracle (UF-09, UF-11; T005-R02).** The matrix said fixture `R` contained a
   comment while showing a comment-free fragment. Freeze exact filename/source, caller sentinels,
   positions, error fields, comment fields, array identity, and recovered topology. The detailed
   Judge selected `View.tsrx` and
   `function View(){ return <div>{/*marker*/}<span>x</div>; }`, with strict `pos: 48`,
   `raisedAt: 55`; collected usage-error range 48..49; and the inner span ending at 47 with
   `closingElement: null` and own `unclosed: true`.

## Evidence

- `.fable-codex/runs/parser-upstream-t999-v2/units/T999/result.json`
- `.fable-codex/runs/parser-upstream-t999-detail/units/T999-detail/result.json`
- exact installed `@tsrx/core@0.1.32` declarations and parser/plugin/error sources
- exact installed `@volar/language-core@2.4.28` and `@volar/source-map@2.4.28` declarations
- `compliance/css-boundary.json`, `packages/runtime/dist/targets.js`, and
  `scripts/package-native.mjs`
- read-only direct-reference probes of strict/collect/loose recovery behavior

The sole corrective product file is `docs/architecture/tsrx-parser-api.md`. No implementation,
Markless edit, OXC contact, publication, or external write is authorized.
