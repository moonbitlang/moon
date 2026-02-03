name = "example/dsl_only"
version = "0.1.0"
source = "src"
deps = { "moonbitlang/x": "0.4.6" }
warnings = "-unused"

options(
  "preferred-target": "wasm-gc",
  "warn-list": "-deprecated",
)
