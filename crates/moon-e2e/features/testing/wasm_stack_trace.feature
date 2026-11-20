# Step definitions: ../../tests/behavior/wasm_stack_trace.rs
# Command assertions: ../../tests/behavior/command.rs
# Source fixture: ../../../moon/tests/test_cases/wasm_stack_trace
@wasm_stack_trace
Feature: Wasm stack trace rendering

  Background:
    V8 forms a Wasm frame's display name from the available module and function names.
    Moonrun demangles recognized MoonBit functions and adds source locations from
    source maps. Frames have the form:
    at <display name> [<source file>:<line>] (<runtime location>)
    The source file and line appear when a source map supplies them.

    The fixture defines module test/stack_trace at version 0.1.0.
    Its main.mbt and lib/main_test.mbt both call abort.

    Given the MoonBit sources in fixture "wasm_stack_trace"

  Scenario: An aborting release executable shows demangled functions and module locations
    When I build the main package for WasmGC in release mode with symbols
    Then the command succeeds
    And stdout matches:
      """
      Artifacts:
        test/stack_trace  wasm-gc  [..]/_build/wasm-gc/release/build/stack_trace.wasm
      """
    And stderr is empty
    When I execute the compiled Wasm with moonrun
    Then the command exits with status 1 on Unix and 1 on Windows
    And stdout is empty
    And stderr matches:
      """
      RuntimeError: unreachable
          at __moonbit_main [WORK_DIR]/main.mbt:2 (wasm://wasm/test/stack_trace@0.1.0-[..]:wasm-function[[..]]:0x[..])
      """

  Scenario Outline: Release tests retain function names <selection>
    When I compile and run the tests for WasmGC with flags "<flags>"
    Then the command exits with status 2 on Unix and 2 on Windows
    And stderr is empty
    And stdout matches:
      """
      [test/stack_trace] test lib/main_test.mbt:1 ("release stack trace") failed: Error
          at @test/stack_trace/lib_blackbox_test.__test_6d61696e5f746573742e6d6274_0 [WORK_DIR]/lib/main_test.mbt:2 (wasm://wasm/test/stack_trace/lib@0.1.0-[..]:wasm-function[[..]]:0x[..])
      ...
      Total tests: 1, passed: 0, failed: 1.
      """

    Examples:
      | selection  | flags                |
      | by default | --release            |
      | explicitly | --release --no-strip |

  Scenario: Explicit strip removes function names in release tests
    With function names stripped, V8 uses the Wasm module name as the frame's
    display name.

    When I compile and run the tests for WasmGC with flags "--release --strip"
    Then the command exits with status 2 on Unix and 2 on Windows
    And stderr is empty
    And stdout matches:
      """
      [test/stack_trace] test lib/main_test.mbt:1 ("release stack trace") failed: Error
          at test/stack_trace/lib@0.1.0 (wasm://wasm/test/stack_trace/lib@0.1.0-[..]:wasm-function[[..]]:0x[..])
      ...
      Total tests: 1, passed: 0, failed: 1.
      """

  Scenario Outline: Stack traces retain names and runtime locations with a <state> source map
    When I build the main package for WasmGC in release mode with symbols
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

  Scenario: Source maps supply positions but do not replace Wasm function names
    When I build the main package for WasmGC in release mode with symbols
    Then the command succeeds
    And stdout matches:
      """
      Artifacts:
        test/stack_trace  wasm-gc  [..]/_build/wasm-gc/release/build/stack_trace.wasm
      """
    And stderr is empty
    # AAAAA maps Wasm offset 0 to source/name index 0 and source line/column 0.
    # The source location in the output confirms that this replacement map was read.
    When I replace the compiled source map with:
      """
      {"version":3,"sources":["source_map_only.mbt"],"names":["source_map_only_function"],"mappings":"AAAAA"}
      """
    And I execute the compiled Wasm with moonrun
    Then the command exits with status 1 on Unix and 1 on Windows
    And stdout is empty
    And stderr matches:
      """
      RuntimeError: unreachable
          at __moonbit_main source_map_only.mbt:1 (wasm://wasm/test/stack_trace@0.1.0-[..]:wasm-function[[..]]:0x[..])
      """
