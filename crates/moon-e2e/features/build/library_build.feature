# Step definitions: ../../tests/behavior/library_build.rs
# Preview assertions: ../../tests/behavior/preview.rs
# Command assertions: ../../tests/behavior/command.rs
@library_build
Feature: Building a root library that imports a nested library
  CLI used here: moon build --target wasm-gc --dry-run --sort-input, then
  moon build --target wasm-gc --sort-input.
  The preview must compile the dependency first and pass its interface to the
  root package. Both packages are libraries, so no link step is requested.
  A subsequent real build verifies that the compiler accepts this arrangement.
  Source inputs are filenames relative to the directory column. SDK inputs,
  code generation defaults and compiled .core contents are not this contract.

  Scenario: Preview the library dependency and then build both packages
    Given a root package importing a nested library
    When I preview moon "build --target wasm-gc"
    Then the command succeeds
    And stderr is empty
    And the "build-package" commands use exactly these source inputs:
      | package            | driver kind | directory | sources   | doctests | generated drivers |
      | username/hello/lib |             | src/lib   | hello.mbt |          |                   |
      | username/hello     |             | src       | top.mbt   |          |                   |
    And "username/hello/lib" is compiled before "username/hello" and imported as "lib"
    And the preview has 0 "link-core" commands
    When I build the libraries
    Then the command succeeds
    And stdout is empty
    And stderr is exactly:
      """
      Finished. moon: ran 2 tasks, now up to date
      """
