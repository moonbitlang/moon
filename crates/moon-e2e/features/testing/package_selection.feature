# Step definitions: ../../tests/behavior/package_selection.rs
# Command assertions: ../../tests/behavior/command.rs
@package_selection
Feature: Selecting tests by package name
  CLI used here: moon test -p <selector> [--file <file>]
  The -p option selects packages by name; --file supplies a file filter.
  An exact full package name selects that package alone.
  A shorter or abbreviated name can select several packages.
  Each selected package runs its blackbox, inline, and whitebox tests in order.
  Tests print their package label (x/y uses x_y) and test kind to stdout,
  followed by the summary. The full output excludes skipped or extra tests.
  The steps also pass --sort-input --no-parallelize to keep output order stable.

  Background:
    Given module "username/hello" has test packages "x", "x/y", and "y"

  Rule: Package selectors determine which tests actually run
    Scenario Outline: <selection>
      When I test packages matching "<selector>"
      Then the command succeeds
      And stderr is empty
      And stdout is exactly:
        """
        <label> blackbox test
        <label> inline test
        <label> whitebox test
        Total tests: 3, passed: 3, failed: 0.
        """

      Examples:
        | selection                      | selector         | label |
        | Exact full name                | username/hello/x | x     |
        | Nested package path            | x/y              | x_y   |
        | Abbreviated module and package | u/h/x/y          | x_y   |

    Scenario Outline: <selection>
      When I test packages matching "<selector>"
      Then the command succeeds
      And stderr is empty
      And stdout is exactly:
        """
        <first label> blackbox test
        <first label> inline test
        <first label> whitebox test
        <second label> blackbox test
        <second label> inline test
        <second label> whitebox test
        Total tests: 6, passed: 6, failed: 0.
        """

      Examples:
        | selection                           | selector | first label | second label |
        | Short name matches several packages | x        | x           | x_y          |
        | Leaf name matches distinct paths    | y        | x_y         | y            |

  Rule: An unmatched selector warns without failing the command
    Scenario: No package matches
      When I test packages matching "asdf"
      Then the command succeeds
      And stdout is exactly:
        """
        Total tests: 0, passed: 0, failed: 0.
        """
      And stderr is exactly:
        """
        Warning: package `asdf` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)
        Warning: no test entry found.
        """

  Rule: A file filter requires exactly one selected package
    Scenario: A file filter is ambiguous across matched packages
      When I test packages matching "y" with file filter "asdf"
      Then the command exits with status 255
      And stdout is empty
      And stderr is exactly:
        """
        Error: Cannot filter by file or index when multiple packages are specified. Matched packages: ["username/hello/x/y", "username/hello/y"]
        """
