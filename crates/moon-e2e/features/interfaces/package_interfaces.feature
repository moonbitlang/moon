# Step definitions: ../../tests/behavior/package_interfaces.rs
# File assertions: ../../tests/behavior/files.rs
# Command assertions: ../../tests/behavior/command.rs
@package_interfaces
Feature: Generating package interfaces
  CLI used here: moon info.
  `moon info` writes the public interface beside each package's source files.

  Scenario Outline: <source layout>
    Given a module has the "<source layout>" source layout
    When I generate package interfaces
    Then the command succeeds
    And stdout is empty
    And stderr is exactly:
      """
      Finished. moon: ran <task count> tasks, now up to date
      """
    And the file "<interface path>" is exactly:
      """
      // Generated using `moon info`, DON'T EDIT IT
      package "username/hello/lib"

      // Values
      pub fn hello() -> String

      // Errors

      // Types and methods

      // Type aliases

      // Traits

      """

    Examples:
      | source layout                                 | task count | interface path             |
      | src directory with a library and main package | 4          | src/lib/pkg.generated.mbti |
      | project root containing a library package     | 2          | lib/pkg.generated.mbti     |

  Scenario: Generate an interface for a package with no public declarations
    Given a library package has no public declarations
    And the generated interface at "lib/pkg.generated.mbti" does not exist
    When I generate package interfaces
    Then the command succeeds
    And stdout is empty
    And stderr is exactly:
      """
      Finished. moon: ran 2 tasks, now up to date
      """
    And the file "lib/pkg.generated.mbti" is exactly:
      """
      // Generated using `moon info`, DON'T EDIT IT
      package "moonbitlang/hhh/lib"

      // Values

      // Errors

      // Types and methods

      // Type aliases

      // Traits

      """
