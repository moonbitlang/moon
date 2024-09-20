# JS Backend Link Options

#### Configurable Options

- The `exports` option is used to specify the function names to export in the JavaScript module.

  For example, in the following configuration, the `hello` function from the current package is exported as the `hello` function in the JavaScript module. In the JavaScript host, the `hello` function can be called to invoke the `hello` function from the current package.

  ```json
  {
    "link": {
      "js": {
        "exports": [
          "hello"
        ]
      }
    }
  }
  ```

- The `format` option is used to specify the output format of the JavaScript module.

  The currently supported formats are:
  - `esm`
  - `cjs`
  - `iife`

  For example, the following configuration sets the output format of the current package to ES Module.

  ```json
  {
    "link": {
      "js": {
        "format": "esm"
      }
    }
  }
  ```
