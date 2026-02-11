![check](https://github.com/moonbitlang/moonrun/actions/workflows/ci.yml/badge.svg)

# moonrun

Moonrun is the WebAssembly runtime for MoonBit, utilizing V8 at its core to offer an efficient and flexible environment for executing WASM.

# Building and Running

## Building

To build the project, ensure that Rust and Cargo are installed. Then execute:
```
cargo build
```

## Running

To run a WebAssembly file:
```
./target/debug/moonrun path/to/your/file.wasm
```

To run the same JS runtime glue directly on Node.js:
```
node ./src/template/moonrun.js path/to/your/file.wasm
```

## JS runtime layout

- `src/template/js_glue_core.js`: runtime-agnostic glue logic.
- `src/template/js_host_node.js`: host adapter + CLI parsing for Node.js.
- `src/template/moonrun.js`: Node.js entrypoint (`host_node` + `glue_core`).
- Distribution note: for a single-file Node runtime, concatenate `js_host_node.js` then `js_glue_core.js`.

# Contribution

To contribute, please read the contribution guidelines at [docs/dev](./docs/dev/README.md).
