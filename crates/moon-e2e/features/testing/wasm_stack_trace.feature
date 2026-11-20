# Step definitions: ../../tests/behavior/wasm_stack_trace.rs
# Command assertions: ../../tests/behavior/command.rs
@wasm_stack_trace
Feature: Demangling module-qualified Wasm stack frames
  CLI used here: moon run . --release --no-strip --target wasm-gc --sort-input
  Test cases use moon test --target wasm-gc --sort-input --no-parallelize
  with the flags stated in each scenario.
  Source-map cases build with moon build --quiet --release --no-strip --target wasm-gc,
  alter the generated map, then execute the real artifact with moonrun.
  Release builds name the Wasm module with its package and version.
  --no-strip retains the function names needed for readable stack traces.
  V8 qualifies stack-frame function names with that module name.
  The displayed function should be demangled without repeating the module name,
  while the runtime location should retain the module name and version.

  Scenario: An aborting release executable shows demangled functions and module locations
    Given a versioned Wasm executable that aborts
    When I run the executable in release mode for WasmGC
    Then the command exits with status 1 on Unix and 1 on Windows
    And stdout is empty
    And stderr matches:
      """
      RuntimeError: unreachable
          at __moonbit_main [WORK_DIR]/main.mbt:2 (wasm://wasm/test/stack_trace@0.1.0-[..]:wasm-function[[..]]:0x[..])
      """

  Scenario: Explicit no-strip retains function names in release tests
    Given a versioned Wasm executable that aborts
    When I run the failing WasmGC test with flags "--release --no-strip"
    Then the command exits with status 2 on Unix and 2 on Windows
    And stderr is empty
    And stdout matches:
      """
      [test/stack_trace] test lib/main_test.mbt:1 ("release stack trace") failed: Error
          at @test/stack_trace/lib_blackbox_test.__test_6d61696e5f746573742e6d6274_0 [WORK_DIR]/lib/main_test.mbt:2 (wasm://wasm/test/stack_trace/lib@0.1.0-[..]:wasm-function[[..]]:0x[..])
      ...
      Total tests: 1, passed: 0, failed: 1.
      """

  Scenario Outline: Release tests strip function names <selection>
    Given a versioned Wasm executable that aborts
    When I run the failing WasmGC test with flags "<flags>"
    Then the command exits with status 2 on Unix and 2 on Windows
    And stderr is empty
    And stdout matches:
      """
      [test/stack_trace] test lib/main_test.mbt:1 ("release stack trace") failed: Error
          at test/stack_trace/lib@0.1.0 (wasm://wasm/test/stack_trace/lib@0.1.0-[..]:wasm-function[[..]]:0x[..])
      ...
      Total tests: 1, passed: 0, failed: 1.
      """

    Examples:
      | selection  | flags             |
      | by default | --release         |
      | explicitly | --release --strip |

  Scenario Outline: A <state> source map preserves demangled names and runtime locations
    Given a versioned Wasm executable that aborts
    When I build the WasmGC executable in release mode with symbols
    Then the command succeeds
    And stdout matches:
      """
      Artifacts:
        test/stack_trace  wasm-gc  [..]/_build/wasm-gc/release/build/stack_trace.wasm
      """
    And stderr is empty
    When I make the compiled source map <state>
    And I execute the compiled Wasm with moonrun
    Then the command exits with status 1 on Unix and 1 on Windows
    And stdout is empty
    And stderr matches:
      """
      RuntimeError: unreachable
          at __moonbit_main (wasm://wasm/test/stack_trace@0.1.0-[..]:wasm-function[[..]]:0x[..])
      """

    Examples:
      | state     |
      | missing   |
      | malformed |
