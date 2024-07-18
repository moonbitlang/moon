[![codecov](https://codecov.io/github/moonbitlang/moon/graph/badge.svg?token=0Rzd0aDlCY)](https://codecov.io/github/moonbitlang/moon)
![check](https://github.com/moonbitlang/moon/actions/workflows/ci.yml/badge.svg)

# moon

The build system and package manager for MoonBit.


```bash
$ moon              
MoonBit's build system

Usage: moon [OPTIONS] <COMMAND>

Commands:
  new                    Create a new moonbit package
  build                  Build the current package
  check                  Check the current package, but don't build object files
  run                    Run WebAssembly module
  test                   Test the current package
  clean                  Clean the target directory
  fmt                    Format moonbit source code
  doc                    Generate documentation
  info                   Generate public interface (`.mbti`) files for all packages in the module
  add                    Add a dependency
  remove                 Remove a dependency
  install                Install dependencies
  tree                   Display the dependency tree
  login                  Log in to your account
  register               Register an account at mooncakes.io
  publish                Publish the current package
  update                 Update the package registry index
  coverage               Code coverage utilities
  generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
  upgrade                Upgrade toolchains
  version                Print version info and exit
  help                   Print this message or the help of the given subcommand(s)

Options:
      --source-dir <SOURCE_DIR>  The source code directory. Defaults to the current directory
      --target-dir <TARGET_DIR>  The target directory. Defaults to `source_dir/target`
  -q, --quiet                    Suppress output
  -v, --verbose                  Increase verbosity
      --trace                    Trace the execution of the program
      --dry-run                  Do not actually run the command
  -h, --help                     Print help
```

See tutorials at [MoonBit's Build System Tutorial](https://www.moonbitlang.com/docs/build-system-tutorial)

## Contributing

To contribute, please read the contribution guidelines at [docs/dev](./docs/dev/README.md).
