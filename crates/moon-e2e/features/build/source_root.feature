# Step definitions: ../../tests/behavior/conditional_compilation.rs
# Preview assertions: ../../tests/behavior/preview.rs
# Command assertions: ../../tests/behavior/command.rs
@source_root
Feature: Resolving source inputs relative to the declared module source root
  CLI used here: moon check --target wasm-gc --dry-run --sort-input.
  This module declares source = "." in moon.mod. Its packages live directly
  under lib/ and main/, so compiler inputs must not gain an implicit src/ prefix.
  These are compiler input paths, not a snapshot of the build directory layout.
  The table lists every check command and distinguishes blackbox doctest inputs.

  Scenario: Use the declared root for ordinary and blackbox source inputs
    Given a project declaring the module root as its source directory
    When I preview moon "check --target wasm-gc"
    Then the command succeeds
    And stderr is empty
    And the "check" commands use exactly these source inputs:
      | package                           | driver kind | directory | sources        | doctests  | generated drivers |
      | username/hello/lib                |             | lib       | hello.mbt      |           |                   |
      | username/hello/main               |             | main      | main.mbt       |           |                   |
      | username/hello/lib_blackbox_test  |             | lib       | hello_test.mbt | hello.mbt |                   |
      | username/hello/main_blackbox_test |             | main      |                | main.mbt  |                   |
