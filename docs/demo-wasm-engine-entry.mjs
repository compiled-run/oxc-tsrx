// Bundle entry for the in-browser demo engine (dist/assets/demo-wasm/engine.js).
//
// Hand-written variant of the NAPI-RS generated wasi-browser loader: the
// generated file compiles the module synchronously, and Chrome refuses sync
// WebAssembly compiles above 8 MB on the main thread. This loader passes the
// URL through to the async instantiation path (instantiateStreaming) instead.
// The import layout mirrors docs/tools/demo-wasm/dist/demo-wasm.wasi-browser.js.
import {
  getDefaultContext as getEmnapiContext,
  instantiateNapiModule,
  WASI,
} from '@napi-rs/wasm-runtime'

const wasi = new WASI({ version: 'preview1' })
const sharedMemory = new WebAssembly.Memory({
  initial: 4000,
  maximum: 65536,
  shared: true,
})

const { napiModule } = await instantiateNapiModule(
  new URL('./demo-wasm.wasm32-wasi.wasm', import.meta.url).href,
  {
    context: getEmnapiContext(),
    asyncWorkPoolSize: 4,
    wasi,
    onCreateWorker() {
      return new Worker(new URL('./wasi-worker-browser.mjs', import.meta.url), {
        type: 'module',
      })
    },
    overwriteImports(importObject) {
      importObject.env = {
        ...importObject.env,
        ...importObject.napi,
        ...importObject.emnapi,
        memory: sharedMemory,
      }
      return importObject
    },
    beforeInit({ instance }) {
      for (const name of Object.keys(instance.exports)) {
        if (name.startsWith('__napi_register__')) {
          instance.exports[name]()
        }
      }
    },
  },
)

export const lint = napiModule.exports.lint
export const format = napiModule.exports.format
export const project = napiModule.exports.project
