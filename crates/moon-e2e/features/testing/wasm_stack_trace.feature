# Step definitions: ../../tests/behavior/wasm_stack_trace.rs
# Command assertions: ../../tests/behavior/command.rs
@wasm_stack_trace
Feature: Demangling module-qualified Wasm stack frames
  CLI used here: moon run . --release --no-strip --target wasm-gc --sort-input
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
