# Command-Line Help for `moon`

This document contains the help content for the `moon` command-line program.

**Command Overview:**

* [`moon`↴](#moon)
* [`moon new`↴](#moon-new)
* [`moon build`↴](#moon-build)
* [`moon check`↴](#moon-check)
* [`moon run`↴](#moon-run)
* [`moon test`↴](#moon-test)
* [`moon clean`↴](#moon-clean)
* [`moon fmt`↴](#moon-fmt)
* [`moon doc`↴](#moon-doc)
* [`moon info`↴](#moon-info)
* [`moon bench`↴](#moon-bench)
* [`moon add`↴](#moon-add)
* [`moon remove`↴](#moon-remove)
* [`moon install`↴](#moon-install)
* [`moon tree`↴](#moon-tree)
* [`moon login`↴](#moon-login)
* [`moon register`↴](#moon-register)
* [`moon publish`↴](#moon-publish)
* [`moon package`↴](#moon-package)
* [`moon update`↴](#moon-update)
* [`moon coverage`↴](#moon-coverage)
* [`moon coverage analyze`↴](#moon-coverage-analyze)
* [`moon coverage report`↴](#moon-coverage-report)
* [`moon coverage clean`↴](#moon-coverage-clean)
* [`moon generate-build-matrix`↴](#moon-generate-build-matrix)
* [`moon upgrade`↴](#moon-upgrade)
* [`moon shell-completion`↴](#moon-shell-completion)
* [`moon version`↴](#moon-version)

## `moon`

**Usage:** `moon <COMMAND>`

###### **Subcommands:**

* `new` — Create a new MoonBit module
* `build` — Build the current package
* `check` — Check the current package, but don't build object files
* `run` — Run a main package
* `test` — Test the current package
* `clean` — Remove the target directory
* `fmt` — Format source code
* `doc` — Generate documentation or searching documentation for a symbol
* `info` — Generate public interface (`.mbti`) files for all packages in the module
* `bench` — Run benchmarks in the current package
* `add` — Add a dependency
* `remove` — Remove a dependency
* `install` — Install dependencies
* `tree` — Display the dependency tree
* `login` — Log in to your account
* `register` — Register an account at mooncakes.io
* `publish` — Publish the current module
* `package` — Package the current module
* `update` — Update the package registry index
* `coverage` — Code coverage utilities
* `generate-build-matrix` — Generate build matrix for benchmarking (legacy feature)
* `upgrade` — Upgrade toolchains
* `shell-completion` — Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
* `version` — Print version information and exit



## `moon new`

Create a new MoonBit module

**Usage:** `moon new [OPTIONS] <PATH>`

###### **Arguments:**

* `<PATH>` — The path of the new project

###### **Options:**

* `--user <USER>` — The username of the module. Default to the logged-in username
* `--name <NAME>` — The name of the module. Default to the last part of the path



## `moon build`

Build the current package

**Usage:** `moon build [OPTIONS] [PATH]`

###### **Arguments:**

* `<PATH>` — The path to the package that should be built

###### **Options:**

* `--std` — Enable the standard library (default)
* `--nostd` — Disable the standard library
* `-g`, `--debug` — Emit debug information
* `--release` — Compile in release mode
* `--strip` — Enable stripping debug information
* `--no-strip` — Disable stripping debug information
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `--enable-coverage` — Enable coverage instrumentation
* `--sort-input` — Sort input files
* `--output-wat` — Output WAT instead of WASM
* `-d`, `--deny-warn` — Treat all warnings as errors
* `--no-render` — Don't render diagnostics (in raw human-readable format)
* `--output-json` — Output diagnostics in JSON format
* `--warn-list <WARN_LIST>` — Warn list config
* `--alert-list <ALERT_LIST>` — Alert list config
* `-j`, `--jobs <JOBS>` — Set the max number of jobs to run in parallel
* `--render-no-loc <MIN_LEVEL>` — Render no-location diagnostics starting from a certain level

  Default value: `error`

  Possible values: `info`, `warn`, `error`

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `-w`, `--watch` — Monitor the file system and automatically build artifacts



## `moon check`

Check the current package, but don't build object files

**Usage:** `moon check [OPTIONS] [PATH]`

###### **Arguments:**

* `<PATH>` — Check single file (.mbt or .mbt.md)

###### **Options:**

* `--std` — Enable the standard library (default)
* `--nostd` — Disable the standard library
* `-g`, `--debug` — Emit debug information
* `--release` — Compile in release mode
* `--strip` — Enable stripping debug information
* `--no-strip` — Disable stripping debug information
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `--enable-coverage` — Enable coverage instrumentation
* `--sort-input` — Sort input files
* `--output-wat` — Output WAT instead of WASM
* `-d`, `--deny-warn` — Treat all warnings as errors
* `--no-render` — Don't render diagnostics (in raw human-readable format)
* `--output-json` — Output diagnostics in JSON format
* `--warn-list <WARN_LIST>` — Warn list config
* `--alert-list <ALERT_LIST>` — Alert list config
* `-j`, `--jobs <JOBS>` — Set the max number of jobs to run in parallel
* `--render-no-loc <MIN_LEVEL>` — Render no-location diagnostics starting from a certain level

  Default value: `error`

  Possible values: `info`, `warn`, `error`

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `-w`, `--watch` — Monitor the file system and automatically check files
* `-p`, `--package-path <PACKAGE_PATH>` — The package(and it's deps) to check
* `--patch-file <PATCH_FILE>` — The patch file to check, Only valid when checking specified package
* `--no-mi` — Whether to skip the mi generation, Only valid when checking specified package
* `--explain` — Whether to explain the error code with details
* `--fmt` — Check whether the code is properly formatted



## `moon run`

Run a main package

**Usage:** `moon run [OPTIONS] <PACKAGE_OR_MBT_FILE> [ARGS]...`

###### **Arguments:**

* `<PACKAGE_OR_MBT_FILE>` — The package or .mbt file to run
* `<ARGS>` — The arguments provided to the program to be run

###### **Options:**

* `--std` — Enable the standard library (default)
* `--nostd` — Disable the standard library
* `-g`, `--debug` — Emit debug information
* `--release` — Compile in release mode
* `--strip` — Enable stripping debug information
* `--no-strip` — Disable stripping debug information
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `--enable-coverage` — Enable coverage instrumentation
* `--sort-input` — Sort input files
* `--output-wat` — Output WAT instead of WASM
* `-d`, `--deny-warn` — Treat all warnings as errors
* `--no-render` — Don't render diagnostics (in raw human-readable format)
* `--output-json` — Output diagnostics in JSON format
* `--warn-list <WARN_LIST>` — Warn list config
* `--alert-list <ALERT_LIST>` — Alert list config
* `-j`, `--jobs <JOBS>` — Set the max number of jobs to run in parallel
* `--render-no-loc <MIN_LEVEL>` — Render no-location diagnostics starting from a certain level

  Default value: `error`

  Possible values: `info`, `warn`, `error`

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `--build-only` — Only build, do not run the code



## `moon test`

Test the current package

**Usage:** `moon test [OPTIONS] [PATH]`

###### **Arguments:**

* `<PATH>` — Run test in single file or directory. If in a project, runs only this package (if matches a package path) or file (if matches a file in package); otherwise, runs in a temporary project

###### **Options:**

* `--std` — Enable the standard library (default)
* `--nostd` — Disable the standard library
* `-g`, `--debug` — Emit debug information
* `--release` — Compile in release mode
* `--strip` — Enable stripping debug information
* `--no-strip` — Disable stripping debug information
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `--enable-coverage` — Enable coverage instrumentation
* `--sort-input` — Sort input files
* `--output-wat` — Output WAT instead of WASM
* `-d`, `--deny-warn` — Treat all warnings as errors
* `--no-render` — Don't render diagnostics (in raw human-readable format)
* `--output-json` — Output diagnostics in JSON format
* `--warn-list <WARN_LIST>` — Warn list config
* `--alert-list <ALERT_LIST>` — Alert list config
* `-j`, `--jobs <JOBS>` — Set the max number of jobs to run in parallel
* `--render-no-loc <MIN_LEVEL>` — Render no-location diagnostics starting from a certain level

  Default value: `error`

  Possible values: `info`, `warn`, `error`

* `-p`, `--package <PACKAGE>` — Run test in the specified package
* `-f`, `--file <FILE>` — Run test in the specified file. Only valid when `--package` is also specified
* `-i`, `--index <INDEX>` — Run only the index-th test in the file. Only valid when `--file` is also specified. Implies `--include-skipped`
* `--doc-index <DOC_INDEX>` — Run only the index-th doc test in the file. Only valid when `--file` is also specified. Implies `--include-skipped`
* `-u`, `--update` — Update the test snapshot
* `-l`, `--limit <LIMIT>` — Limit of expect test update passes to run, in order to avoid infinite loops

  Default value: `256`
* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `--build-only` — Only build, do not run the tests
* `--no-parallelize` — Run the tests in a target backend sequentially
* `--test-failure-json` — Print failure message in JSON format
* `--patch-file <PATCH_FILE>` — Path to the patch file
* `--doc` — Run doc test
* `--include-skipped` — Include skipped tests. Automatically implied when `--[doc-]index` is set



## `moon clean`

Remove the target directory

**Usage:** `moon clean`



## `moon fmt`

Format source code

**Usage:** `moon fmt [OPTIONS] [ARGS]...`

###### **Arguments:**

* `<ARGS>`

###### **Options:**

* `--check` — Check only and don't change the source code
* `--sort-input` — Sort input files
* `--block-style <BLOCK_STYLE>` — Add separator between each segments

  Possible values: `false`, `true`

* `--warn` — Warn if code is not properly formatted



## `moon doc`

Generate documentation or searching documentation for a symbol

**Usage:** `moon doc [OPTIONS] [SYMBOL]`

###### **Arguments:**

* `<SYMBOL>` — The symbol to query documentation for, e.g. 'String::from*' or '@list.from*'

###### **Options:**

* `--serve` — Start a web server to serve the documentation
* `-b`, `--bind <BIND>` — The address of the server

  Default value: `127.0.0.1`
* `-p`, `--port <PORT>` — The port of the server

  Default value: `3000`
* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date



## `moon info`

Generate public interface (`.mbti`) files for all packages in the module

**Usage:** `moon info [OPTIONS] [PATH]`

###### **Arguments:**

* `<PATH>` — The file-system path to the package or file in package to emit `mbti` files for

   Conflicts with `--package`.

###### **Options:**

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `-p`, `--package <PACKAGE>` — The full or subset of name of the package to emit `mbti` files for



## `moon bench`

Run benchmarks in the current package

**Usage:** `moon bench [OPTIONS]`

###### **Options:**

* `--std` — Enable the standard library (default)
* `--nostd` — Disable the standard library
* `-g`, `--debug` — Emit debug information
* `--release` — Compile in release mode
* `--strip` — Enable stripping debug information
* `--no-strip` — Disable stripping debug information
* `--target <TARGET>` — Select output target

  Possible values: `wasm`, `wasm-gc`, `js`, `native`, `llvm`, `all`

* `--enable-coverage` — Enable coverage instrumentation
* `--sort-input` — Sort input files
* `--output-wat` — Output WAT instead of WASM
* `-d`, `--deny-warn` — Treat all warnings as errors
* `--no-render` — Don't render diagnostics (in raw human-readable format)
* `--output-json` — Output diagnostics in JSON format
* `--warn-list <WARN_LIST>` — Warn list config
* `--alert-list <ALERT_LIST>` — Alert list config
* `-j`, `--jobs <JOBS>` — Set the max number of jobs to run in parallel
* `--render-no-loc <MIN_LEVEL>` — Render no-location diagnostics starting from a certain level

  Default value: `error`

  Possible values: `info`, `warn`, `error`

* `-p`, `--package <PACKAGE>` — Run test in the specified package
* `-f`, `--file <FILE>` — Run test in the specified file. Only valid when `--package` is also specified
* `-i`, `--index <INDEX>` — Run only the index-th test in the file. Only valid when `--file` is also specified
* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `--build-only` — Only build, do not bench
* `--no-parallelize` — Run the benchmarks in a target backend sequentially



## `moon add`

Add a dependency

**Usage:** `moon add [OPTIONS] <PACKAGE_PATH>`

###### **Arguments:**

* `<PACKAGE_PATH>` — The package path to add

###### **Options:**

* `--bin` — Whether to add the dependency as a binary



## `moon remove`

Remove a dependency

**Usage:** `moon remove <PACKAGE_PATH>`

###### **Arguments:**

* `<PACKAGE_PATH>` — The package path to remove



## `moon install`

Install dependencies

**Usage:** `moon install`



## `moon tree`

Display the dependency tree

**Usage:** `moon tree`



## `moon login`

Log in to your account

**Usage:** `moon login`



## `moon register`

Register an account at mooncakes.io

**Usage:** `moon register`



## `moon publish`

Publish the current module

**Usage:** `moon publish [OPTIONS]`

###### **Options:**

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date



## `moon package`

Package the current module

**Usage:** `moon package [OPTIONS]`

###### **Options:**

* `--frozen` — Do not sync dependencies, assuming local dependencies are up-to-date
* `--list`



## `moon update`

Update the package registry index

**Usage:** `moon update`



## `moon coverage`

Code coverage utilities

**Usage:** `moon coverage <COMMAND>`

###### **Subcommands:**

* `analyze` — Run test with instrumentation and report coverage
* `report` — Generate code coverage report
* `clean` — Clean up coverage artifacts



## `moon coverage analyze`

Run test with instrumentation and report coverage

**Usage:** `moon coverage analyze [OPTIONS] [-- <EXTRA_FLAGS>...]`

###### **Arguments:**

* `<EXTRA_FLAGS>` — Extra flags passed directly to `moon_cove_report`

###### **Options:**

* `-p`, `--package <PACKAGE>` — Analyze coverage for a specific package



## `moon coverage report`

Generate code coverage report

**Usage:** `moon coverage report [args]... [COMMAND]`

###### **Arguments:**

* `<args>` — Arguments to pass to the coverage utility

###### **Options:**

* `-h`, `--help` — Show help for the coverage utility



## `moon coverage clean`

Clean up coverage artifacts

**Usage:** `moon coverage clean`



## `moon generate-build-matrix`

Generate build matrix for benchmarking (legacy feature)

**Usage:** `moon generate-build-matrix [OPTIONS] --output-dir <OUT_DIR>`

###### **Options:**

* `-n <NUMBER>` — Set all of `drow`, `dcol`, `mrow`, `mcol` to the same value
* `--drow <DIR_ROWS>` — Number of directory rows
* `--dcol <DIR_COLS>` — Number of directory columns
* `--mrow <MOD_ROWS>` — Number of module rows
* `--mcol <MOD_COLS>` — Number of module columns
* `-o`, `--output-dir <OUT_DIR>` — The output directory



## `moon upgrade`

Upgrade toolchains

**Usage:** `moon upgrade [OPTIONS]`

###### **Options:**

* `-f`, `--force` — Force upgrade
* `--dev` — Install the latest development version



## `moon shell-completion`

Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout

**Usage:** `moon shell-completion [OPTIONS]`

###### **Options:**

* `--shell <SHELL>` — The shell to generate completion for

  Default value: `<your shell>`

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




## `moon version`

Print version information and exit

**Usage:** `moon version [OPTIONS]`

###### **Options:**

* `--all` — Print all version information
* `--json` — Print version information in JSON format
* `--no-path` — Do not print the path



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>