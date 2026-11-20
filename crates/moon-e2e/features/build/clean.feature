# Step definitions: ../../tests/behavior/clean.rs
# Command assertions: ../../tests/behavior/command.rs
@clean
Feature: Cleaning a project before rebuilding it
  Each scenario uses a private copy of the source fixture.
  A real build creates the project's default _build directory. Cleaning removes
  that directory, and another real build succeeds without the previous outputs.
  Directory removal is the clean contract; its internal layout is not asserted.

  Scenario Outline: Clean and rebuild a <project> project
    Given a "<project>" project to clean
    When I build the project
    Then the command succeeds
    And stdout matches:
      """
      Artifacts:
        <package>  wasm  [WORK_DIR]/_build/[..].wasm
      """
    And stderr is exactly:
      """
      Finished. moon: ran <task count> tasks, now up to date
      """
    And the project build directory "_build" exists
    When I clean the project
    Then the command succeeds
    And stdout is empty
    And stderr is empty
    And the project build directory "_build" does not exist
    When I build the project
    Then the command succeeds
    And stdout matches:
      """
      Artifacts:
        <package>  wasm  [WORK_DIR]/_build/[..].wasm
      """
    And stderr is exactly:
      """
      Finished. moon: ran <task count> tasks, now up to date
      """
    And the project build directory "_build" exists

    Examples:
      | project       | package                 | task count |
      | single-module | username/clean/cmd/main | 3          |
      | workspace     | alice/app/main         | 3          |
