# Step definitions: ../../tests/behavior/conditional_compilation.rs
# Preview assertions: ../../tests/behavior/preview.rs
# Command assertions: ../../tests/behavior/command.rs
@conditional_compilation
Feature: Selecting library sources by backend and optimization profile
  CLI used here: moon build --target BACKEND [--debug] --dry-run --sort-input,
  and moon bundle --target js --dry-run --sort-input.
  The fixture's targets rules combine backend atoms, debug/release, not, and/or.
  In particular wasm_release_or_js_debug.mbt needs either Wasm Release or JS Debug.
  Each table gives the entire source input set for every build-package command.
  Filenames are relative to the row's directory; empty cells mean no inputs.
  This tests selection, not SDK core link order or compiled .core contents.

  Scenario: Switching backends reselects sources and omitted build profile means debug
    All previews share one project and build directory. Switching back to wasm-gc
    must restore its selection rather than reuse the previous backend's files.

    Given a project with backend and profile conditions on library source files
    When I preview moon "build --target wasm-gc"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                         | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt all_wasm.mbt not_js.mbt only_debug.mbt only_wasm_gc.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                        |          |                   |
    When I preview moon "build --target wasm"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                                                         | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt all_wasm.mbt js_or_wasm.mbt native_and_wasm.mbt not_js.mbt only_debug.mbt only_wasm.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                                                        |          |                   |
    When I preview moon "build --target wasm-gc"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                         | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt all_wasm.mbt not_js.mbt only_debug.mbt only_wasm_gc.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                        |          |                   |
    When I preview moon "build --target js"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                                        | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                                       |          |                   |
    When I preview moon "build --target wasm --debug"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                                                         | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt all_wasm.mbt js_or_wasm.mbt native_and_wasm.mbt not_js.mbt only_debug.mbt only_wasm.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                                                        |          |                   |
    When I preview moon "build --target wasm-gc --debug"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                         | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt all_wasm.mbt not_js.mbt only_debug.mbt only_wasm_gc.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                        |          |                   |
    When I preview moon "build --target js --debug"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                                        | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                                                       |          |                   |

  Scenario: Bundling defaults to release and selects release-only sources
    Given a project with backend and profile conditions on library source files
    When I preview moon "bundle --target js"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                               | doctests | generated drivers |
      | username/hello/lib  |             | src/lib   | all.mbt js_and_release.mbt js_or_wasm.mbt only_js.mbt |          |                   |
      | username/hello/main |             | src/main  | main.mbt                                              |          |                   |
    And the preview has 0 "link-core" commands
