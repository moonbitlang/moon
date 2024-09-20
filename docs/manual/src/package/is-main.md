# is-main

The `is-main` field is used to specify whether a package needs to be linked into an executable file.

The output of the linking process depends on the backend. When this field is set to `true`:

- For the `wasm` and `wasm-gc` backends, a standalone WebAssembly module will be generated.
- For the `js` backend, a standalone JavaScript file will be generated.
