# Step definitions: ../../tests/behavior/conditional_compilation.rs
# Preview assertions: ../../tests/behavior/preview.rs
# Command assertions: ../../tests/behavior/command.rs
@conditional_tests
Feature: Applying source conditions to checks and test drivers
  CLI used here: moon check/test --target js --dry-run --sort-input.
  Ordinary sources, blackbox tests and doctests must obey the same conditions.
  js_only_test.mbt belongs only to JS blackbox tests; ordinary sources are
  passed as doctest-only inputs when checking or building blackbox targets.
  Tables enumerate every command of the named kind. Sources and doctests are
  filenames relative to the directory column. Generated drivers are identified
  by their producer's package:kind, and must be consumed by the right compiler command.

  Scenario: Checking includes selected blackbox files and selected doctests
    Given a project with backend and profile conditions on library source files
    When I preview moon "check --target js"
    Then the command succeeds
    And stderr is empty
    And the "check" commands use exactly these source inputs:
      | package                           | driver kind | directory | sources                                                                        | doctests                                                                       | generated drivers |
      | username/hello/lib                |             | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                                                                                |                   |
      | username/hello/main               |             | src/main  | main.mbt                                                                       |                                                                                |                   |
      | username/hello/lib_blackbox_test  |             | src/lib   | hello_test.mbt js_only_test.mbt                                                | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                   |
      | username/hello/main_blackbox_test |             | src/main  |                                                                                | main.mbt                                                                       |                   |

  Scenario: Test drivers and compiler commands agree on selected inputs
    Given a project with backend and profile conditions on library source files
    When I preview moon "test --target js"
    Then the command succeeds
    And stderr is empty
    And the "generate-test-driver" commands use exactly these source inputs:
      | package             | driver kind | directory | sources                                                                        | doctests                                                                       | generated drivers |
      | username/hello/lib  | internal    | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                                                                                |                   |
      | username/hello/main | internal    | src/main  | main.mbt                                                                       |                                                                                |                   |
      | username/hello/lib  | blackbox    | src/lib   | hello_test.mbt js_only_test.mbt                                                | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                   |
      | username/hello/main | blackbox    | src/main  |                                                                                | main.mbt                                                                       |                   |
    And the "build-package" commands use exactly these source inputs:
      | package                           | driver kind | directory | sources                                                                        | doctests                                                                       | generated drivers            |
      | username/hello/lib                |             | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                                                                                |                              |
      | username/hello/main               |             | src/main  | main.mbt                                                                       |                                                                                |                              |
      | username/hello/lib                |             | src/lib   | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt |                                                                                | username/hello/lib:internal  |
      | username/hello/main               |             | src/main  | main.mbt                                                                       |                                                                                | username/hello/main:internal |
      | username/hello/lib_blackbox_test  |             | src/lib   | hello_test.mbt js_only_test.mbt                                                | all.mbt js_or_wasm.mbt only_debug.mbt only_js.mbt wasm_release_or_js_debug.mbt | username/hello/lib:blackbox  |
      | username/hello/main_blackbox_test |             | src/main  |                                                                                | main.mbt                                                                       | username/hello/main:blackbox |
