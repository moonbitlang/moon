# Step definitions: ../../tests/behavior/packaging.rs
# Command assertions: ../../tests/behavior/command.rs
@packaging
Feature: Selecting package archive contents with ignore rules
  CLI used here: moon package --list.
  These projects declare username/hello version 0.1.0 and contain four check targets.
  Packaging checks the project, then lists the archive contents in order.
  The --list output contains file paths only; directory entries are omitted.
  Each project's .moonignore controls which files
  are packaged; later matching patterns override earlier ones.
  The old moon.test files in the exclusion fixtures remain ordinary input files:
  they are themselves part of the archive list being tested.
  The complete stderr below checks the four-task check summary,
  file order and archive destination together. [WORK_DIR] is the private project
  directory; no other output is omitted.

  Scenario: Re-including directories keeps their complete contents
    .moonignore ignores all root entries, then re-includes /moon.mod, /src/ and /test/.

    Given a packaging project with "re-included directories"
    When I list the package archive contents
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Running moon check ...
      Finished. moon: ran 4 tasks, now up to date
      Check passed
      moon.mod
      src/lib/hello.mbt
      src/lib/hello_test.mbt
      src/lib/moon.pkg
      src/main/main.mbt
      src/main/moon.pkg
      test/a/a.txt
      test/b/b.txt
      test/c/c.md
      Package to [WORK_DIR]/_build/publish/username-hello-0.1.0.zip
      """

  Scenario: Re-including extensions selects matching files below subdirectories
    .moonignore ignores files, allows directory traversal, then re-includes
    moon.mod, *.mbt and *.txt. Build and dependency directories remain ignored.

    Given a packaging project with "re-included extensions"
    When I list the package archive contents
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Running moon check ...
      Finished. moon: ran 4 tasks, now up to date
      Check passed
      moon.mod
      src/lib/hello.mbt
      src/lib/hello_test.mbt
      src/main/main.mbt
      test/a/a.txt
      test/b/b.txt
      Package to [WORK_DIR]/_build/publish/username-hello-0.1.0.zip
      """

  Scenario: A later negated pattern restores an ignored file
    .moonignore re-includes moon.mod, *.mbt and *.txt, then ignores test/a/a.txt.
    The later !test/a/a.txt pattern restores that file to the archive.

    Given a packaging project with "a later file re-inclusion"
    When I list the package archive contents
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Running moon check ...
      Finished. moon: ran 4 tasks, now up to date
      Check passed
      moon.mod
      src/lib/hello.mbt
      src/lib/hello_test.mbt
      src/main/main.mbt
      test/a/a.txt
      test/b/b.txt
      Package to [WORK_DIR]/_build/publish/username-hello-0.1.0.zip
      """

  Scenario: Ignoring a directory omits all of its contents
    .moonignore ignores test/a, so none of that directory's files are packaged.

    Given a packaging project with "an ignored directory"
    When I list the package archive contents
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Running moon check ...
      Finished. moon: ran 4 tasks, now up to date
      Check passed
      README.md
      moon.mod
      moon.test
      src/lib/hello.mbt
      src/lib/hello_test.mbt
      src/lib/moon.pkg
      src/main/main.mbt
      src/main/moon.pkg
      test/b/b.txt
      Package to [WORK_DIR]/_build/publish/username-hello-0.1.0.zip
      """

  Scenario: Ignoring an extension omits matches below subdirectories
    .moonignore ignores *.txt at any depth.

    Given a packaging project with "an ignored extension"
    When I list the package archive contents
    Then the command succeeds
    And stdout is empty
    And stderr matches:
      """
      Running moon check ...
      Finished. moon: ran 4 tasks, now up to date
      Check passed
      README.md
      moon.mod
      moon.test
      src/lib/hello.mbt
      src/lib/hello_test.mbt
      src/lib/moon.pkg
      src/main/main.mbt
      src/main/moon.pkg
      test/c/c.md
      Package to [WORK_DIR]/_build/publish/username-hello-0.1.0.zip
      """
