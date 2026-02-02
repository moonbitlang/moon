use super::*;

#[test]
fn test_moon_cmd() {
    let dir = TestDir::new("moon_commands");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/list/lib.mbt -o ./_build/wasm-gc/release/build/lib/list/list.core -pkg design/lib/list -pkg-sources design/lib/list:./lib/list -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib/queue/lib.mbt -o ./_build/wasm-gc/release/build/lib/queue/queue.core -pkg design/lib/queue -i ./_build/wasm-gc/release/build/lib/list/list.mi:list -pkg-sources design/lib/queue:./lib/queue -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main2/main.mbt -o ./_build/wasm-gc/release/build/main2/main2.core -pkg design/main2 -is-main -i ./_build/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main2:./main2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/list/list.core ./_build/wasm-gc/release/build/lib/queue/queue.core ./_build/wasm-gc/release/build/main2/main2.core -main design/main2 -o ./_build/wasm-gc/release/build/main2/main2.wasm -pkg-config-path ./main2/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main2:./main2 -target wasm-gc
            moonc build-package ./main1/main.mbt -o ./_build/wasm-gc/release/build/main1/main1.core -pkg design/main1 -is-main -i ./_build/wasm-gc/release/build/lib/queue/queue.mi:queue -pkg-sources design/main1:./main1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/release/build/lib/list/list.core ./_build/wasm-gc/release/build/lib/queue/queue.core ./_build/wasm-gc/release/build/main1/main1.core -main design/main1 -o ./_build/wasm-gc/release/build/main1/main1.wasm -pkg-config-path ./main1/moon.pkg.json -pkg-sources design/lib/list:./lib/list -pkg-sources design/lib/queue:./lib/queue -pkg-sources design/main1:./main1 -target wasm-gc
        "#]],
    );
}

#[test]
fn test_moon_help() {
    let dir = TestDir::new_empty();
    check(
        get_stdout(&dir, ["help"]).replace("moon.exe", "moon"),
        expect![[r#"
            The build system and package manager for MoonBit.

            Usage: moon [OPTIONS] <COMMAND>

            Commands:
              new                    Create a new MoonBit module
              build                  Build the current package
              check                  Check the current package, but don't build object files
              run                    Run a main package
              test                   Test the current package
              clean                  Remove the target directory
              fmt                    Format source code
              doc                    Generate documentation or searching documentation for a symbol
              info                   Generate public interface (`.mbti`) files for all packages in the module
              bench                  Run benchmarks in the current package
              add                    Add a dependency
              remove                 Remove a dependency
              install                Install a binary package globally or install project dependencies (deprecated without args)
              tree                   Display the dependency tree
              fetch                  Download a package to .repos directory (unstable)
              login                  Log in to your account
              register               Register an account at mooncakes.io
              publish                Publish the current module
              package                Package the current module
              update                 Update the package registry index
              coverage               Code coverage utilities
              generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
              upgrade                Upgrade toolchains
              shell-completion       Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
              version                Print version information and exit
              help                   Print this message or the help of the given subcommand(s)

            Options:
              -h, --help  Print help

            Common Options:
              -C, --directory <SOURCE_DIR>
                      The source code directory. Defaults to the current directory
                  --target-dir <TARGET_DIR>
                      The target directory. Defaults to `source_dir/target`
              -q, --quiet
                      Suppress output
              -v, --verbose
                      Increase verbosity
                  --trace
                      Trace the execution of the program
                  --dry-run
                      Do not actually run the command
              -Z, --unstable-feature <UNSTABLE_FEATURE>
                      Unstable flags to MoonBuild [env: MOON_UNSTABLE=] [default: ]
        "#]],
    );
}

#[test]
fn test_moon_add_help_includes_no_update() {
    let dir = TestDir::new_empty();
    let out = get_stdout(&dir, ["add", "--help"]).replace("moon.exe", "moon");
    assert!(out.contains("--no-update"));
}

#[test]
#[ignore]
#[cfg(unix)]
fn test_bench4() {
    let dir = TestDir::new_empty();
    get_stdout(&dir, ["generate-build-matrix", "-n", "4", "-o", "bench4"]);
    check(
        get_stdout(
            &dir,
            [
                "run",
                "--source-dir",
                "./bench4",
                "--target-dir",
                "./bench4/target",
                "main",
            ],
        ),
        expect![[r#"
            ok
        "#]],
    );

    get_stdout(
        &dir,
        [
            "run",
            "--source-dir",
            "./bench4",
            "--target-dir",
            "./bench4/target",
            "--trace",
            "main",
        ],
    );

    let trace_file = dunce::canonicalize(dir.join("./trace.json")).unwrap();
    let t = std::fs::read_to_string(trace_file).unwrap();
    assert!(t.contains("moonbit::build::read"));
    assert!(t.contains(r#""name":"work.run""#));
    assert!(t.contains(r#""name":"run""#));
    assert!(t.contains(r#""name":"main""#));
}
