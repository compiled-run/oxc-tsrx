# `@oxc-tsrx/parser`

The canonical OXC-shaped parser for JavaScript, TypeScript, JSX, and TSRX. Ordinary
files take a direct public-OXC path; only `.tsrx` files enter TSRX scanning and
authored-tree reconstruction. Results expose independent lazy `program`, `module`,
`comments`, and `errors` getters, with synchronous and asynchronous entry points.

```js
import { parseSync } from "@oxc-tsrx/parser";

const result = parseSync("View.tsrx", "function View() { return @if (ok) <p>yes</p>; }");
console.log(result.program, result.errors);
```

Native code is supplied by the matching `@oxc-tsrx/native-*` optional package.
The loader validates its package role, target, version, API/ABI, exact OXC revision,
object header, byte length, and SHA-256 digest before loading it. No install script,
runtime download, OXC fork, JavaScript parser fallback, or child-process parse path
is used.
