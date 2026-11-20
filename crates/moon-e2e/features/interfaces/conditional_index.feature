# Step definitions: ../../tests/behavior/conditional_compilation.rs
# Command assertions: ../../tests/behavior/command.rs
@conditional_index
Feature: Keeping conditional source information in the package index
  CLI used here: moon check --target wasm-gc --sort-input.
  A real check writes packages.json for IDE consumers. It must retain all source
  files and their conditions, including files inactive for the current backend.
  Its check-command arrays, however, must contain only the selected inputs.
  The table enumerates all ordinary, blackbox and whitebox files; no whitebox
  files are declared. Backend/profile columns describe each dimension separately:
  the combined condition on wasm_release_or_js_debug.mbt is tested by the
  compiler-input scenarios, not inferred by taking their Cartesian product.
  No assertion depends on JSON formatting, SDK link order or artifact contents.
  Diagnostic underline padding may vary; warning codes, messages and locations remain exact.

  Scenario: JS-only files remain indexed but are excluded from WasmGC checks
    Given a project with backend and profile conditions on library source files
    When I check the conditional project for wasm-gc
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Warning: [0002]
         ╭─[ [WORK_DIR]/src/lib/only_debug.mbt:2:7 ]
         │
       2 │   let a = 1
         │       ┬[..]
         │       ╰── Warning (unused_value): Unused variable 'a'
      ───╯
      Warning: [0029]
         ╭─[ [WORK_DIR]/src/main/moon.pkg:2:3 ]
         │
       2 │   "username/hello/lib",
         │   ──────────┬─────────[..]
         │             ╰─────────── Warning (unused_package): Unused package 'username/hello/lib'
      ───╯
      Finished. moon: ran 4 tasks, now up to date (2 warnings, 0 errors)
      """
    When I read the package index at "_build/wasm-gc/debug/check/packages.json"
    Then the index describes backend "wasm-gc" and profile "debug"
    And the index retains exactly these source files and conditions:
      | package | category   | file                         | backends                   | profiles      |
      | lib     | files      | all.mbt                      | Wasm WasmGC Js Native LLVM | Release Debug |
      | lib     | files      | all_wasm.mbt                 | Wasm WasmGC                | Release Debug |
      | lib     | files      | js_and_release.mbt           | Js                         | Release       |
      | lib     | files      | js_or_wasm.mbt               | Wasm Js                    | Release Debug |
      | lib     | files      | native_and_wasm.mbt          | Wasm Native LLVM           | Release Debug |
      | lib     | files      | native_only.mbt              | Native LLVM                | Release Debug |
      | lib     | files      | not_js.mbt                   | Wasm WasmGC Native LLVM    | Release Debug |
      | lib     | files      | only_debug.mbt               | Wasm WasmGC Js Native LLVM | Debug         |
      | lib     | files      | only_js.mbt                  | Js                         | Release Debug |
      | lib     | files      | only_wasm.mbt                | Wasm                       | Release Debug |
      | lib     | files      | only_wasm_gc.mbt             | WasmGC                     | Release Debug |
      | lib     | files      | wasm_release_or_js_debug.mbt | Wasm Js                    | Release Debug |
      | lib     | test-files | hello_test.mbt               | Wasm WasmGC Js Native LLVM | Release Debug |
      | lib     | test-files | js_only_test.mbt             | Js                         | Release Debug |
      | main    | files      | main.mbt                     | Wasm WasmGC Js Native LLVM | Release Debug |
    And the index check commands use exactly these source inputs:
      | package | command            | directory | sources                                                         | doctests                                                        |
      | lib     | check-command      | src/lib   | all.mbt all_wasm.mbt not_js.mbt only_debug.mbt only_wasm_gc.mbt |                                                                 |
      | lib     | test-check-command | src/lib   | hello_test.mbt                                                  | all.mbt all_wasm.mbt not_js.mbt only_debug.mbt only_wasm_gc.mbt |
      | main    | check-command      | src/main  | main.mbt                                                        |                                                                 |
      | main    | test-check-command | src/main  |                                                                 | main.mbt                                                        |
