# Step definitions: ../../tests/behavior/snapshot_updates.rs
# File assertions: ../../tests/behavior/files.rs
# Command assertions: ../../tests/behavior/command.rs
@snapshot_updates
Feature: Updating inline test expectations for selected backends
  CLI used here: moon test --target <backend> -u --no-parallelize.
  After each update, moon test --target <backend> --no-parallelize runs
  without updating.
  The project has four files: lib/x.wasm.mbt, lib/x.wasm-gc.mbt,
  lib/x.js.mbt, and lib/x.native.mbt. Each contains one test calling
  inspect("<backend>") without a saved expectation; only its backend selects it.
  Updating saves the inspect content in the selected source file. Other files,
  including snapshots saved by earlier updates, must stay byte-for-byte unchanged.
  One private project is updated through Wasm, WasmGC, JavaScript, and Native.
  This exercises the real Wasm runtimes, Node.js, and native compiler; each
  rerun verifies that the saved expectation persists.
  Native builds can print compiler/linker progress on stderr, such as MSVC's
  source filenames and import-library messages. Native command expectations
  cover exit status and the test result on stdout.

  Scenario: Update each backend while preserving the other source files
    Given a project with one inline snapshot for each backend
    And the file "lib/x.wasm.mbt" is exactly:
      """
      test {
        inspect("wasm")
      }

      """
    And the file "lib/x.wasm-gc.mbt" is exactly:
      """
      test {
        inspect("wasm-gc")
      }

      """
    And the file "lib/x.js.mbt" is exactly:
      """
      test {
        inspect("js")
      }

      """
    And the file "lib/x.native.mbt" is exactly:
      """
      test {
        inspect("native")
      }

      """

    When I update inline snapshots for wasm
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """
    And the file "lib/x.wasm.mbt" is exactly:
      """
      test {
        inspect("wasm", content=(#|wasm
        ))
      }

      """
    And these source files are unchanged by the update:
      | source file       |
      | lib/x.wasm-gc.mbt |
      | lib/x.js.mbt      |
      | lib/x.native.mbt  |
    When I test the saved inline snapshots for wasm
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """

    When I update inline snapshots for wasm-gc
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """
    And the file "lib/x.wasm-gc.mbt" is exactly:
      """
      test {
        inspect("wasm-gc", content=(#|wasm-gc
        ))
      }

      """
    And these source files are unchanged by the update:
      | source file       |
      | lib/x.wasm.mbt    |
      | lib/x.js.mbt      |
      | lib/x.native.mbt  |
    When I test the saved inline snapshots for wasm-gc
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """

    When I update inline snapshots for js
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """
    And the file "lib/x.js.mbt" is exactly:
      """
      test {
        inspect("js", content=(#|js
        ))
      }

      """
    And these source files are unchanged by the update:
      | source file       |
      | lib/x.wasm.mbt    |
      | lib/x.wasm-gc.mbt |
      | lib/x.native.mbt  |
    When I test the saved inline snapshots for js
    Then the command succeeds
    And stderr is empty
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """

    When I update inline snapshots for native
    Then the command succeeds
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """
    And the file "lib/x.native.mbt" is exactly:
      """
      test {
        inspect("native", content=(#|native
        ))
      }

      """
    And these source files are unchanged by the update:
      | source file       |
      | lib/x.wasm.mbt    |
      | lib/x.wasm-gc.mbt |
      | lib/x.js.mbt      |
    When I test the saved inline snapshots for native
    Then the command succeeds
    And stdout is exactly:
      """
      Total tests: 1, passed: 1, failed: 0.
      """
