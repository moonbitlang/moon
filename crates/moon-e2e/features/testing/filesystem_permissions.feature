# Step definitions: ../../tests/behavior/filesystem_permissions.rs
# Assertions: ../../tests/behavior/command.rs, ../../tests/behavior/files.rs
@filesystem_permissions
Feature: Directory permissions for existing files

  Background:
    Each script uses an inline permission header. Directory grants are recursive;
    write includes read, and the closest configured directory determines access.
    The scripts compile with moonbitlang/async 0.22.10 and execute through moonrun.

    Given an isolated workspace containing these files:
      | path              | contents |
      | work/file         | original |
      | work/private/file | keep     |

  Scenario: A writable directory permits reading and truncating an existing file
    When I compile and run this MoonBit script for Wasm:
      """
      ----
      import { "moonbitlang/async@0.22.10/fs" }
      permission(dirs: { "work": ["write"] })
      ----
      async fn main {
        println(@fs.read_file("work/file").text())
        @fs.write_file("work/file", "new", create_mode=TruncateExisting)
        let file = @fs.open("work/file", mode=ReadOnly)
        defer file.close()
        println(file.size())
      }
      """
    Then the command succeeds
    And stdout is exactly:
      """
      original
      3
      """
    And stderr is empty
    And the file "work/file" is exactly:
      """
      new
      """

  Scenario: A parent directory handle cannot bypass a read-only child
    When I compile and run this MoonBit script for Wasm:
      """
      ----
      import { "moonbitlang/async@0.22.10/fs" }
      permission(dirs: { "work": ["write"], "work/private": ["read"] })
      ----
      async fn main {
        let directory = @fs.opendir("work")
        defer directory.close()
        try {
          let file = directory.open("private/file", mode=ReadWrite)
          file.close()
          println("opened")
        } catch { _ => println("denied") }
      }
      """
    Then the command succeeds
    And stdout is exactly:
      """
      denied
      """
    And stderr matches:
      """
      Sandbox policy blocked file write: "private[..]file"
      """
    And the file "work/private/file" is exactly:
      """
      keep
      """

  Scenario: Create-or-truncate is rejected before an existing file is changed
    When I compile and run this MoonBit script for Wasm:
      """
      ----
      import { "moonbitlang/async@0.22.10/fs" }
      permission(dirs: { "work": ["write"] })
      ----
      async fn main {
        try {
          @fs.write_file("work/file", "replaced")
          println("written")
        } catch { _ => println("denied") }
      }
      """
    Then the command succeeds
    And stdout is exactly:
      """
      denied
      """
    And stderr is empty
    And the file "work/file" is exactly:
      """
      original
      """

  Scenario: Guest rename is rejected until directory changes are coordinated
    When I compile and run this MoonBit script for Wasm:
      """
      ----
      import { "moonbitlang/async@0.22.10/fs" }
      permission(dirs: { "work": ["write"] })
      ----
      async fn main {
        try {
          @fs.rename("work/file", "work/moved")
          println("renamed")
        } catch { _ => println("denied") }
      }
      """
    Then the command succeeds
    And stdout is exactly:
      """
      denied
      """
    And stderr is empty
    And the file "work/file" is exactly:
      """
      original
      """
