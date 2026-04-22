name = "example/dsl_only"

version = "0.1.0"

import {
  "moonbitlang/cli@0.1.2",
  "moonbitlang/core@0.4.8",
  "moonbitlang/x@0.4.6",
}

options(
  source: "src",
  readme: "README.mbt.md",
  repository: "https://example.com/repo",
  license: "Apache-2.0",
  keywords: ["dsl", "fixture"],
  description: "Fixture for module DSL parsing",
  "bin-deps": {
    "tool/cli": { "path": "../bin", "bin-pkg": ["cli"] },
  },
  "compile-flags": ["-DDEBUG", "-Wall"],
  "link-flags": ["-lm"],
  warnings: "-unused",
  "warn-list": "-deprecated",
  "preferred-target": "wasm-gc",
  "include": ["src/**", "README.mbt.md"],
  "exclude": ["target/**", "**/*.tmp"],
  "scripts": {
    "prebuild": "node ./prebuild.js",
    "postbuild": "echo done",
  },
)
