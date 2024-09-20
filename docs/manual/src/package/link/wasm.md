# wasm Backend Link Options

#### Configurable Options

- The `exports` option is used to specify the function names exported by the `wasm` backend.

  For example, in the following configuration, the `hello` function from the current package is exported as the `hello` function in the `wasm` module, and the `foo` function is exported as the `bar` function in the `wasm` module. In the `wasm` host, the `hello` and `bar` functions can be called to invoke the `hello` and `foo` functions from the current package.

  ```json
  {
    "link": {
      "wasm": {
        "exports": [
          "hello",
          "foo:bar"
        ]
      }
    }
  }
  ```

- The `heap-start-address` option is used to specify the starting address of the linear memory that can be used when compiling to the `wasm` backend.

  For example, the following configuration sets the starting address of the linear memory to 1024.

  ```json
  {
    "link": {
      "wasm": {
        "heap-start-address": 1024
      }
    }
  }
  ```

- The `import-memory` option is used to specify the linear memory imported by the `wasm` module.

  For example, the following configuration specifies that the linear memory imported by the `wasm` module is the `memory` variable from the `env` module.

  ```json
  {
    "link": {
      "wasm": {
        "import-memory": {
          "module": "env",
          "name": "memory"
        }
      }
    }
  }
  ```

- The `export-memory-name` option is used to specify the name of the linear memory exported by the `wasm` module.

  ```json
  {
    "link": {
      "wasm": {
        "export-memory-name": "memory"
      }
    }
  }
  ```