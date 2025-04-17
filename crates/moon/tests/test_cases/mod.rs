// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::io::Write;

use super::*;
use expect_test::expect;
use moonutil::{
    common::{
        get_cargo_pkg_version, CargoPathExt, StringExt, TargetBackend, DEP_PATH, MOON_MOD_JSON,
    },
    module::MoonModJSON,
};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

mod backend_config;
mod design;
mod diamond_pkg;
mod docs_examples;
mod expect_test;
mod extra_flags;
mod fancy_import;
mod hello;
mod inline_test;
mod mbti;
mod moon_bench;
mod moon_bundle;
mod moon_commands;
mod moon_new;
mod moon_test;
mod moon_version;
mod output_format;
mod packages;
mod simple_pkg;
mod target_backend;
mod test_error_report;
mod test_filter;

#[test]
fn test_need_link() {
    let dir = TestDir::new("need_link.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core -main username/hello/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );
}
#[test]
fn test_dummy_core() {
    let test_dir = TestDir::new("dummy-core.in");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/0",
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": {
                        "$ROOT/0/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/0/y_wbtest.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/1",
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": {
                        "$ROOT/1/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/1/x.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/1/x_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/2",
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": {
                        "$ROOT/2/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1",
                          "fspath": "$ROOT/1"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/char",
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/coverage",
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/iter",
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char",
                          "fspath": "$ROOT/char"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/iter/iter.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/prelude",
                      "root": "moonbitlang/core",
                      "rel": "prelude",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/prelude/prelude.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--target", "js", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/0",
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": {
                        "$ROOT/0/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/0/y_wbtest.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/1",
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": {
                        "$ROOT/1/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/1/x.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/1/x_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/2",
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": {
                        "$ROOT/2/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1",
                          "fspath": "$ROOT/1"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/char",
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/coverage",
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/iter",
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char",
                          "fspath": "$ROOT/char"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/iter/iter.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/prelude",
                      "root": "moonbitlang/core",
                      "rel": "prelude",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/prelude/prelude.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "js",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    };

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc check -o ./target/wasm/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc check -o ./target/wasm/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc check -o ./target/wasm/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc check ./2/lib.mbt -o ./target/wasm/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm.mbt -o ./target/wasm/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/js/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc check -o ./target/js/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc check -o ./target/js/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc check -o ./target/js/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc check ./2/lib.mbt -o ./target/js/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/js/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js -whitebox-test
            moonc check ./0/lib.mbt ./0/y.js.mbt ./0/y_wbtest.js.mbt ./0/y_wbtest.mbt -o ./target/js/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js -whitebox-test
            moonc check ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core ./target/wasm/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc link-core ./target/wasm/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core ./target/js/release/build/2/2.core -main moonbitlang/core/2 -o ./target/js/release/build/2/2.js -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core -main moonbitlang/core/1 -o ./target/js/release/build/1/1.js -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc link-core ./target/js/release/build/0/0.core -main moonbitlang/core/0 -o ./target/js/release/build/0/0.js -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/iter --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-config-path ./iter/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/coverage --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-config-path ./coverage/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/char --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-config-path ./char/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/2 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./target/wasm/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/iter --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package -o ./target/wasm/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm -g -O0
            moonc build-package ./target/wasm/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/coverage/coverage.core ./target/wasm/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-config-path ./iter/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/coverage --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./target/wasm/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-config-path ./coverage/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/char --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./target/wasm/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/coverage/coverage.core ./target/wasm/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm/debug/test/char/char.internal_test.wasm -test-mode -pkg-config-path ./char/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/2 --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm -g -O0
            moonc build-package ./2/lib.mbt ./target/wasm/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/1/1.core ./target/wasm/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm/debug/test/2/2.internal_test.wasm -test-mode -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt ./target/wasm/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt ./target/wasm/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm/debug/test/1/1.internal_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm.mbt ./target/wasm/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt ./target/wasm/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm/debug/test/0/0.internal_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/iter --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-config-path ./iter/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/coverage --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-config-path ./coverage/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/char --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-config-path ./char/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/2 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./target/js/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/js/debug/test/prelude/prelude.internal_test.js -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/iter --sort-input --target js --driver-kind internal --mode test
            moonc build-package -o ./target/js/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js -g -O0
            moonc build-package ./target/js/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/js/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/coverage/coverage.core ./target/js/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/js/debug/test/iter/iter.internal_test.js -test-mode -pkg-config-path ./iter/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/coverage --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./target/js/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/js/debug/test/coverage/coverage.internal_test.js -test-mode -pkg-config-path ./coverage/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/char --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./target/js/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/js/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/coverage/coverage.core ./target/js/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/js/debug/test/char/char.internal_test.js -test-mode -pkg-config-path ./char/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/2 --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/debug/test/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js -g -O0
            moonc build-package ./2/lib.mbt ./target/js/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/js/debug/test/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/1/1.core ./target/js/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/js/debug/test/2/2.internal_test.js -test-mode -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target js --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.js.mbt ./target/js/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/js/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target js -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/js/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/js/debug/test/1/1.whitebox_test.js -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./1/lib.mbt ./1/x.js.mbt ./target/js/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/js/debug/test/1/1.internal_test.js -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target js --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.js.mbt ./0/y_wbtest.js.mbt ./0/y_wbtest.mbt ./target/js/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/js/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target js -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/js/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/js/debug/test/0/0.whitebox_test.js -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./0/lib.mbt ./0/y.js.mbt ./target/js/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/js/debug/test/0/0.internal_test.js -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--enable-coverage", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=@self
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/iter --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./target/wasm-gc/debug/test/iter/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/iter/iter.internal_test.core -pkg moonbitlang/core/iter -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/iter/iter.internal_test.core -main moonbitlang/core/iter -o ./target/wasm-gc/debug/test/iter/iter.internal_test.wasm -test-mode -pkg-config-path ./iter/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/iter:./iter -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/coverage --sort-input --target wasm-gc --driver-kind internal --enable-coverage --coverage-package-override=@self --mode test
            moonc build-package ./target/wasm-gc/debug/test/coverage/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -pkg moonbitlang/core/coverage -is-main -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=@self -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.internal_test.core -main moonbitlang/core/coverage -o ./target/wasm-gc/debug/test/coverage/coverage.internal_test.wasm -test-mode -pkg-config-path ./coverage/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/char --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./target/wasm-gc/debug/test/char/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/char/char.internal_test.core -pkg moonbitlang/core/char -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/char/char.internal_test.core -main moonbitlang/core/char -o ./target/wasm-gc/debug/test/char/char.internal_test.wasm -test-mode -pkg-config-path ./char/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/char:./char -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/2 --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/debug/test/1/1.core -pkg moonbitlang/core/1 -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -enable-coverage
            moonc build-package ./2/lib.mbt ./target/wasm-gc/debug/test/2/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/2/2.internal_test.core -pkg moonbitlang/core/2 -is-main -i ./target/wasm-gc/debug/test/1/1.mi:1 -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/2:./2 -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/1/1.core ./target/wasm-gc/debug/test/2/2.internal_test.core -main moonbitlang/core/2 -o ./target/wasm-gc/debug/test/2/2.internal_test.wasm -test-mode -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --enable-coverage --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -enable-coverage -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/1/1.internal_test.core -pkg moonbitlang/core/1 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/1/1.internal_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.internal_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --enable-coverage --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -enable-coverage -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/0/0.internal_test.core -pkg moonbitlang/core/0 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/0/0.internal_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.internal_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc build-package -o ./target/wasm/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core ./target/wasm/release/bundle/prelude/prelude.core -o ./target/wasm/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc build-package -o ./target/js/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core ./target/js/release/bundle/prelude/prelude.core -o ./target/js/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "bundle",
                "--target",
                "all",
                "--dry-run",
                "--sort-input",
                "--serial",
            ],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc build-package -o ./target/wasm/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core ./target/wasm/release/bundle/prelude/prelude.core -o ./target/wasm/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc build-package -o ./target/js/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core ./target/js/release/bundle/prelude/prelude.core -o ./target/js/release/bundle/core.core
            moonc build-package ./1/lib.mbt -o ./target/native/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target native
            moonc build-package -o ./target/native/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target native
            moonc build-package ./0/lib.mbt -o ./target/native/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target native
            moonc build-package ./2/lib.mbt -o ./target/native/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/native/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target native
            moonc build-package -o ./target/native/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/native/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target native
            moonc build-package -o ./target/native/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/native/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target native
            moonc build-package -o ./target/native/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target native
            moonc bundle-core ./target/native/release/bundle/0/0.core ./target/native/release/bundle/1/1.core ./target/native/release/bundle/2/2.core ./target/native/release/bundle/coverage/coverage.core ./target/native/release/bundle/char/char.core ./target/native/release/bundle/iter/iter.core ./target/native/release/bundle/prelude/prelude.core -o ./target/native/release/bundle/core.core
        "#]],
    );
}

#[test]
#[ignore = "not implemented"]
fn test_backend_flag() {
    let dir = TestDir::new("backend-flag.in");

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/js/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc check ./main/main.mbt -o ./target/js/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc check ./lib/hello.mbt ./lib/hello_test.mbt -o ./target/js/release/check/lib/lib.underscore_test.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target/js/debug/test --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_test.mbt ./target/js/debug/test/lib/__generated_driver_for_underscore_test.mbt -o ./target/js/debug/test/lib/lib.underscore_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.underscore_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.underscore_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -g -ryu
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-sources username/hello/lib:./lib -target js -ryu
        "#]],
    );

    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib
            moonc build-package ./main/main.mbt -o ./target/js/release/bundle/main/main.core -pkg username/hello/main -is-main -i ./target/js/release/bundle/lib/lib.mi:lib -pkg-sources username/hello/main:./main
            moonc bundle-core ./target/js/release/bundle/lib/lib.core ./target/js/release/bundle/main/main.core -o ./target/js/release/bundle/hello.core
        "#]],
    );
}

#[test]
fn test_source_map() {
    let dir = TestDir::new("hello.in");

    // no -source-map in wasm backend
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources hello/main:./main -target wasm -g -O0
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/build/main/main.core -main hello/main -o ./target/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--debug",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/js/debug/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources hello/main:./main -target js -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/debug/build/main/main.core -main hello/main -o ./target/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_find_ancestor_with_mod() {
    let dir = TestDir::new("hello.in");

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_js_format() {
    let dir = TestDir::new("js_format.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js",
                "--sort-input",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib3/hello.mbt -o ./target/js/release/build/lib3/lib3.core -pkg username/hello/lib3 -pkg-sources username/hello/lib3:./lib3 -target js
            moonc link-core ./target/js/release/build/lib3/lib3.core -main username/hello/lib3 -o ./target/js/release/build/lib3/lib3.js -pkg-config-path ./lib3/moon.pkg.json -pkg-sources username/hello/lib3:./lib3 -target js -exported_functions=hello -js-format iife
            moonc build-package ./lib2/hello.mbt -o ./target/js/release/build/lib2/lib2.core -pkg username/hello/lib2 -pkg-sources username/hello/lib2:./lib2 -target js
            moonc link-core ./target/js/release/build/lib2/lib2.core -main username/hello/lib2 -o ./target/js/release/build/lib2/lib2.js -pkg-config-path ./lib2/moon.pkg.json -pkg-sources username/hello/lib2:./lib2 -target js -exported_functions=hello -js-format cjs
            moonc build-package ./lib1/hello.mbt -o ./target/js/release/build/lib1/lib1.core -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -target js
            moonc link-core ./target/js/release/build/lib1/lib1.core -main username/hello/lib1 -o ./target/js/release/build/lib1/lib1.js -pkg-config-path ./lib1/moon.pkg.json -pkg-sources username/hello/lib1:./lib1 -target js -exported_functions=hello -js-format esm
            moonc build-package ./lib0/hello.mbt -o ./target/js/release/build/lib0/lib0.core -pkg username/hello/lib0 -pkg-sources username/hello/lib0:./lib0 -target js
            moonc link-core ./target/js/release/build/lib0/lib0.core -main username/hello/lib0 -o ./target/js/release/build/lib0/lib0.js -pkg-config-path ./lib0/moon.pkg.json -pkg-sources username/hello/lib0:./lib0 -target js -exported_functions=hello -js-format esm
        "#]],
    );
    let _ = get_stdout(&dir, ["build", "--target", "js", "--nostd"]);
    let t = dir.join("target").join("js").join("release").join("build");
    check(
        std::fs::read_to_string(t.join("lib0").join("lib0.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib0$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib0$$hello as hello }
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib1").join("lib1.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib1$$hello() {
              return "Hello, world!";
            }
            export { username$hello$lib1$$hello as hello }
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib2").join("lib2.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            function username$hello$lib2$$hello() {
              return "Hello, world!";
            }
            exports.hello = username$hello$lib2$$hello;
        "#]],
    );
    check(
        std::fs::read_to_string(t.join("lib3").join("lib3.js"))
            .unwrap()
            .replace_crlf_to_lf(),
        expect![[r#"
            (() => {
              function username$hello$lib3$$hello() {
                return "Hello, world!";
              }
              globalThis.hello = username$hello$lib3$$hello;
            })();
        "#]],
    );
}

#[test]
fn test_warn_list_dry_run() {
    let dir = TestDir::new("warn_list.in");

    check(
        get_stdout(&dir, ["build", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -w -2 -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./lib1/hello.mbt -w -1 -o ./target/wasm-gc/release/build/lib1/lib1.core -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:./lib1 -target wasm-gc
            moonc build-package ./main/main.mbt -w -1-2 -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -i ./target/wasm-gc/release/build/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/lib1/lib1.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -w -2-29 -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./lib1/hello.mbt -w -1-29 -o ./target/wasm-gc/release/build/lib1/lib1.core -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:./lib1 -target wasm-gc
            moonc build-package ./main/main.mbt -w -1-2-29 -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -i ./target/wasm-gc/release/build/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/lib1/lib1.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["bundle", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -w -2 -o ./target/wasm-gc/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./lib1/hello.mbt -w -1 -o ./target/wasm-gc/release/bundle/lib1/lib1.core -pkg username/hello/lib1 -pkg-sources username/hello/lib1:./lib1 -target wasm-gc
            moonc build-package ./main/main.mbt -w -1-2 -o ./target/wasm-gc/release/bundle/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/bundle/lib/lib.mi:lib -i ./target/wasm-gc/release/bundle/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:./main -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/lib/lib.core ./target/wasm-gc/release/bundle/lib1/lib1.core ./target/wasm-gc/release/bundle/main/main.core -o ./target/wasm-gc/release/bundle/hello.core
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--sort-input"]);

    check(
        get_stdout(&dir, ["check", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -w -2 -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check ./lib1/hello.mbt -w -1 -o ./target/wasm-gc/release/check/lib1/lib1.mi -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:./lib1 -target wasm-gc
            moonc check ./main/main.mbt -w -1-2 -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:./main -target wasm-gc
            moonc check ./lib/hello_test.mbt -w -2 -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc check ./lib/hello.mbt -w -2-29 -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check ./lib1/hello.mbt -w -1-29 -o ./target/wasm-gc/release/check/lib1/lib1.mi -pkg username/hello/lib1 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib1:./lib1 -target wasm-gc
            moonc check ./main/main.mbt -w -1-2-29 -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -i ./target/wasm-gc/release/check/lib1/lib1.mi:lib1 -pkg-sources username/hello/main:./main -target wasm-gc
            moonc check ./lib/hello_test.mbt -w -2-29 -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test
        "#]],
    );
}

#[test]
fn test_warn_list_real_run() {
    let dir = TestDir::new("warn_list.in");

    check(
        get_stderr(&dir, ["build", "--sort-input", "--no-render"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    check(
        get_stderr(&dir, ["test", "--sort-input"])
            .lines()
            .filter(|it| !it.starts_with("Blocking waiting for file lock"))
            .collect::<String>(),
        expect![[r#""#]],
    );
    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stderr(&dir, ["bundle", "--sort-input", "--no-render"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--sort-input"]);

    check(
        get_stderr(&dir, ["check", "--sort-input", "--no-render"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_alert_list() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("alert_list.in");

    check(
        get_stderr(&dir, ["build", "--sort-input"]),
        expect![[r#"
            Warning: [2000]
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stderr(&dir, ["bundle", "--sort-input"]),
        expect![[r#"
            Warning: [2000]
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [2000]
               ╭─[$ROOT/main/main.mbt:3:3]
               │
             3 │   alert_2();
               │   ───┬───  
               │      ╰───── Warning (Alert alert_2): alert_2
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_mod_level_warn_alert_list() {
    let dir = TestDir::new("mod_level_warn&alert_list.in");

    check(
        get_stdout(&dir, ["check", "--dry-run"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -w -1 -alert -alert_1 -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check ./main/main.mbt -w -1-2 -alert -alert_1-alert_2 -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
        "#]],
    );
}

#[test]
fn test_no_work_to_do() {
    let dir = TestDir::new("moon_new.in");
    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("now up to date"));

    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("moon: no work to do"));

    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("now up to date"));
    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("moon: no work to do"));
}

#[test]
fn test_moon_test_release() {
    let dir = TestDir::new("test_release.in");

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--release", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind whitebox --release --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/release/test/lib/lib.whitebox_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -whitebox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.whitebox_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --release --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "--release", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_backtrace() {
    let dir = TestDir::new("backtrace.in");

    let out = get_err_stderr(&dir, ["run", "main"]);
    assert!(!out.contains("main.foo"));
    assert!(!out.contains("main.bar"));

    let out = get_err_stderr(&dir, ["run", "main", "--debug"]);
    assert!(out.contains("main.foo"));
    assert!(out.contains("main.bar"));
}

#[test]
fn test_deny_warn() {
    std::env::set_var("NO_COLOR", "1");
    let dir = TestDir::new("test_deny_warn.in");

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [2000]
                ╭─[$ROOT/lib/hello.mbt:13:3]
                │
             13 │   alert_1();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_1): alert_1
            ────╯
            Warning: [2000]
                ╭─[$ROOT/lib/hello.mbt:14:3]
                │
             14 │   alert_2();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_2): alert_2
            ────╯
            Warning: [1002]
               ╭─[$ROOT/lib/hello.mbt:4:7]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            Warning: [1002]
                ╭─[$ROOT/lib/hello.mbt:11:7]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning: Unused variable '中文'
            ────╯
            Warning: [1002]
                ╭─[$ROOT/lib/hello.mbt:12:7]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning: Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [1002]
               ╭─[$ROOT/main/main.mbt:2:7]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    check(
        get_err_stdout(&dir, ["check", "--deny-warn", "--sort-input"]),
        expect![[r#"
            failed: moonc check -error-format json -w @a-31-32 -alert @all-raise-throw-unsafe+deprecated $ROOT/lib/hello.mbt -o $ROOT/target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
        "#]],
    );

    check(
        get_stderr(&dir, ["build", "--sort-input"]),
        expect![[r#"
            Warning: [2000]
                ╭─[$ROOT/lib/hello.mbt:13:3]
                │
             13 │   alert_1();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_1): alert_1
            ────╯
            Warning: [2000]
                ╭─[$ROOT/lib/hello.mbt:14:3]
                │
             14 │   alert_2();
                │   ───┬───  
                │      ╰───── Warning (Alert alert_2): alert_2
            ────╯
            Warning: [1002]
               ╭─[$ROOT/lib/hello.mbt:4:7]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            Warning: [1002]
                ╭─[$ROOT/lib/hello.mbt:11:7]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning: Unused variable '中文'
            ────╯
            Warning: [1002]
                ╭─[$ROOT/lib/hello.mbt:12:7]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning: Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [1002]
               ╭─[$ROOT/main/main.mbt:2:7]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning: Unused variable 'a'
            ───╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        get_err_stdout(&dir, ["build", "--deny-warn", "--sort-input"]),
        expect![[r#"
            failed: moonc build-package -error-format json -w @a-31-32 -alert @all-raise-throw-unsafe+deprecated $ROOT/lib/hello.mbt -o $ROOT/target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc
        "#]],
    );
}

#[test]
fn test_moon_fmt() {
    let dir = TestDir::new("moon_fmt.in");
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
                pub fn hello() -> String { "Hello, world!" }
            "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
                fn main { println(@lib.hello()) }"#]],
    );
    let _ = get_stdout(&dir, ["fmt"]);
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
            ///|
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_moon_fmt_002() {
    let dir = TestDir::new("moon_fmt.in");
    let _ = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["fmt", "--check"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    check(
        read(dir.join("lib").join("hello.mbt")),
        expect![[r#"
            pub fn hello() -> String { "Hello, world!" }
        "#]],
    );
    check(
        read(dir.join("main").join("main.mbt")),
        expect![[r#"
            fn main { println(@lib.hello()) }"#]],
    );
    check(
        read(
            dir.join("target")
                .join(TargetBackend::default().to_dir_name())
                .join("release")
                .join("format")
                .join("lib")
                .join("hello.mbt"),
        ),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(
            dir.join("target")
                .join(TargetBackend::default().to_dir_name())
                .join("release")
                .join("format")
                .join("main")
                .join("main.mbt"),
        ),
        expect![[r#"
            ///|
            fn main {
              println(@lib.hello())
            }
        "#]],
    );
}

#[test]
fn test_moon_fmt_extra_args() {
    let dir = TestDir::new("moon_fmt.in");
    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
        "#]],
    );
    check(
        get_stdout(&dir, ["fmt", "--dry-run", "--sort-input", "--", "a", "b"]),
        expect![[r#"
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt a b
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt a b
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt a b
        "#]],
    );
    check(
        get_stdout(&dir, ["fmt", "--check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "fmt",
                "--check",
                "--sort-input",
                "--dry-run",
                "--",
                "c",
                "d",
            ],
        ),
        expect![[r#"
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt c d
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt c d
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt c d
        "#]],
    );
}

#[test]
fn test_moon_fmt_block_style() {
    let dir = TestDir::new("moon_fmt.in");
    check(
        get_stdout(&dir, ["fmt", "--block-style", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt -block-style
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt -block-style
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt -block-style
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["fmt", "--block-style=true", "--sort-input", "--dry-run"],
        ),
        expect![[r#"
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt -block-style
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt -block-style
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt -block-style
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["fmt", "--block-style=false", "--sort-input", "--dry-run"],
        ),
        expect![[r#"
            moonfmt ./lib/hello.mbt -w -o ./target/wasm-gc/release/format/lib/hello.mbt
            moonfmt ./lib/hello_wbtest.mbt -w -o ./target/wasm-gc/release/format/lib/hello_wbtest.mbt
            moonfmt ./main/main.mbt -w -o ./target/wasm-gc/release/format/main/main.mbt
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "fmt",
                "--block-style",
                "--check",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moon tool format-and-diff --old ./lib/hello.mbt --new ./target/wasm-gc/release/format/lib/hello.mbt --block-style
            moon tool format-and-diff --old ./lib/hello_wbtest.mbt --new ./target/wasm-gc/release/format/lib/hello_wbtest.mbt --block-style
            moon tool format-and-diff --old ./main/main.mbt --new ./target/wasm-gc/release/format/main/main.mbt --block-style
        "#]],
    );
}

#[test]
fn test_export_memory_name() {
    let dir = TestDir::new("export_memory.in");
    let _ = get_stdout(&dir, ["build", "--target", "wasm-gc", "--output-wat"]);
    let content = std::fs::read_to_string(
        dir.join("target")
            .join("wasm-gc")
            .join("release")
            .join("build")
            .join("main")
            .join("main.wat"),
    )
    .unwrap();
    assert!(content.contains("awesome_memory"));

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -export-memory-name awesome_memory
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
        "#]],
    );
}

#[test]
fn test_no_block_params() {
    let dir = TestDir::new("no_block_params.in");
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/js/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/js/release/bundle -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./main/main.mbt -o ./target/js/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/js/release/bundle -i ./target/js/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core ./target/js/release/build/lib/lib.core ./target/js/release/build/main/main.core -main username/hello/main -o ./target/js/release/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target js
        "#]],
    );
}

#[test]
fn test_panic() {
    let dir = TestDir::new("panic.in");
    let data = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["test"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    let out = String::from_utf8_lossy(&data).to_string();
    check(
        &out,
        expect![[r#"
            test username/hello/lib/hello_wbtest.mbt::panic failed: panic is expected
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_validate_import() {
    let dir = TestDir::new("validate_import.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["test"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
    check(
        get_err_stderr(&dir, ["bundle"]),
        expect![[r#"
            error: failed to read import path in "$ROOT/main/moon.pkg.json"

            Caused by:
                No matching module was found for mbt/core/set
        "#]],
    );
}

#[test]
fn test_multi_process() {
    use std::process::Command;
    use std::thread;

    let dir = TestDir::new("test_multi_process");
    let path: PathBuf = dir.as_ref().into();

    let (num_threads, inner_loop) = (16, 10);
    let mut container = vec![];

    let success = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

    for _ in 0..num_threads {
        let path = path.clone();
        let success = success.clone();
        let work = thread::spawn(move || {
            for _ in 0..inner_loop {
                let _ = std::fs::OpenOptions::new()
                    .append(true)
                    .open(path.join("lib/hello.mbt"))
                    .unwrap()
                    .write(b"\n")
                    .unwrap();

                let output = Command::new(moon_bin())
                    .arg("check")
                    .current_dir(path.clone())
                    .output()
                    .expect("Failed to execute command");

                if output.status.success() {
                    success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let out = String::from_utf8(output.stderr).unwrap();
                    assert!(out.contains("no work to do") || out.contains("now up to date"));
                } else {
                    println!("moon output: {:?}", String::from_utf8(output.stdout));
                    let error_message = String::from_utf8_lossy(&output.stderr);
                    println!("{}", error_message);
                }
            }
        });
        container.push(work);
    }

    for i in container {
        i.join().unwrap();
    }

    assert_eq!(
        success.load(std::sync::atomic::Ordering::SeqCst),
        num_threads * inner_loop
    );
}

#[test]
fn test_internal_package() {
    let dir = TestDir::new("internal_package.in");
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            error: $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/lib2`
            $ROOT/lib2/moon.pkg.json: cannot import internal package `username/hello/lib/internal/b` in `username/hello/lib2`
            $ROOT/main/moon.pkg.json: cannot import internal package `username/hello/lib/internal` in `username/hello/main`
        "#]],
    );
}

#[test]
fn test_nonexistent_package() {
    let dir = TestDir::new("nonexistent_package.in");
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            error: $ROOT/main/moon.pkg.json: cannot import `username/hello/lib/b` in `username/hello/main`, no such package
            $ROOT/main/moon.pkg.json: cannot import `username/hello/transient` in `username/hello/main`, no such package
            $ROOT/pkg/transient/moon.pkg.json: cannot import `username/transient/lib/b` in `username/transient`, no such package
        "#]],
    );
}

#[test]
fn mooncakes_io_smoke_test() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("hello.in");
    let _ = get_stdout(&dir, ["update"]);
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {
                "lijunchen/hello2": "0.1.0"
              }
            }"#]],
    );
    let _ = get_stdout(&dir, ["remove", "lijunchen/hello2"]);
    check(
        std::fs::read_to_string(dir.join("moon.mod.json")).unwrap(),
        expect![[r#"
            {
              "name": "hello",
              "deps": {}
            }"#]],
    );
    let _ = get_stdout(&dir, ["add", "lijunchen/hello2@0.1.0"]);
    std::fs::write(
        dir.join("main/main.mbt"),
        r#"fn main {
  println(@lib.hello2())
}
"#,
    )
    .unwrap();

    assert!(dir
        .join(DEP_PATH)
        .join("lijunchen")
        .join("hello")
        .join(MOON_MOD_JSON)
        .exists());

    std::fs::remove_dir_all(dir.join(DEP_PATH)).unwrap();
    let out = get_stdout(&dir, ["install"]);
    let mut lines = out.lines().collect::<Vec<_>>();
    lines.sort();
    check(
        lines.join("\n"),
        expect![[r#"
            Using cached lijunchen/hello2@0.1.0
            Using cached lijunchen/hello@0.1.0"#]],
    );

    std::fs::write(
        dir.join("main/moon.pkg.json"),
        r#"{
          "is-main": true,
          "import": [
            "lijunchen/hello2/lib"
          ]
        }
    "#,
    )
    .unwrap();

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
            Hello, world!Hello, world2!
        "#]],
    );
}

#[test]
#[ignore = "where to download mooncake?"]
fn mooncake_cli_smoke_test() {
    let dir = TestDir::new("hello.in");
    let out = std::process::Command::new(moon_bin())
        .env("RUST_BACKTRACE", "0")
        .current_dir(&dir)
        .args(["publish"])
        .output()
        .unwrap();
    let s = std::str::from_utf8(&out.stderr).unwrap().to_string();
    assert!(s.contains("failed to open credentials file"));
}

#[test]
fn bench2_test() {
    let dir = TestDir::new("bench2_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("ok[..]");
}

#[test]
fn cakenew_test() {
    let dir = TestDir::new("cakenew_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_matches("Hello,[..]");
}

#[test]
fn capture_abort_test() {
    let dir = super::TestDir::new("capture_abort_test.in");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["run", "main", "--nostd"])
        .assert()
        .failure();
}

#[test]
fn whitespace_test() {
    let dir = TestDir::new("whitespace_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    // unstable test
    // check(
    //     &get_stdout_with_args(&dir, ["check", "--dry-run", "--nostd"]),
    //     expect![[r#"
    //         moonc check './main lib/hello.mbt' './main lib/hello_test.mbt' -o './target/check/main lib/main lib.underscore_test.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main lib/hello.mbt' -o './target/check/main lib/main lib.mi' -pkg 'username/hello/main lib' -pkg-sources 'username/hello/main lib:./main lib'
    //         moonc check './main exe/main.mbt' -o './target/check/main exe/main exe.mi' -pkg 'username/hello/main exe' -is-main -i './target/check/main lib/main lib.mi:lib' -pkg-sources 'username/hello/main exe:./main exe'
    //     "#]],
    // );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main exe",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/debug/build/main lib/main lib.core" -pkg "username/hello/main lib" -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc -g -O0 -source-map
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/debug/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -i "./target/wasm-gc/debug/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonc link-core "./target/wasm-gc/debug/build/main lib/main lib.core" "./target/wasm-gc/debug/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/debug/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main exe/main exe.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main exe"]),
        expect![[r#"
            Hello, world!
        "#]],
    );

    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("moon: ran 3 tasks, now up to date"));
}

#[test]
fn test_whitespace_parent_space() -> anyhow::Result<()> {
    let tmp_dir = tempfile::TempDir::new()?;
    let path_with_space = tmp_dir.path().join("with space");
    std::fs::create_dir_all(&path_with_space)?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases")
        .join("whitespace_test.in");
    copy(&dir, &path_with_space)?;

    let canon = dunce::canonicalize(tmp_dir.path())?;
    let prefix = canon.as_path().display().to_string().replace('\\', "/");

    let out = get_stdout(
        &path_with_space,
        ["build", "--no-render", "--sort-input", "--dry-run"],
    );
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
            moonc build-package "./main lib/hello.mbt" -o "./target/wasm-gc/release/build/main lib/main lib.core" -pkg "username/hello/main lib" -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources "username/hello/main lib:./main lib" -target wasm-gc
            moonc build-package "./main exe/main.mbt" -o "./target/wasm-gc/release/build/main exe/main exe.core" -pkg "username/hello/main exe" -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i "./target/wasm-gc/release/build/main lib/main lib.mi:lib" -pkg-sources "username/hello/main exe:./main exe" -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core "./target/wasm-gc/release/build/main lib/main lib.core" "./target/wasm-gc/release/build/main exe/main exe.core" -main "username/hello/main exe" -o "./target/wasm-gc/release/build/main exe/main exe.wasm" -pkg-config-path "./main exe/moon.pkg.json" -pkg-sources "username/hello/main lib:./main lib" -pkg-sources "username/hello/main exe:./main exe" -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    let out = get_stderr(&path_with_space, ["build", "--no-render"]);
    let out = out.replace(&prefix, ".");
    let out = out.replace(
        &moonutil::moon_dir::home()
            .to_str()
            .unwrap()
            .replace('\\', "/"),
        "$MOON_HOME",
    );

    copy(&dir, &path_with_space)?;
    check(
        &out,
        expect![[r#"
        Finished. moon: ran 3 tasks, now up to date
    "#]],
    );
    Ok(())
}

#[test]
fn circle_pkg_test() {
    let dir = TestDir::new("circle_pkg_AB_001_test.in");
    let stderr = get_err_stderr(&dir, ["run", "main", "--nostd"]);
    assert!(stderr.contains("cyclic dependency"), "stderr: {}", stderr);
}

#[test]
fn debug_flag_test() {
    let dir = TestDir::new("debug_flag_test.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    check(
        get_stdout(&dir, ["check", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg hello/main -is-main -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--debug", "--nostd"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--dry-run",
                "--target",
                "wasm-gc",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["run", "main", "--target", "wasm-gc", "--dry-run", "--nostd"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--debug",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg hello/lib -pkg-sources hello/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg hello/main -is-main -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main hello/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/lib:./lib -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map
            moonrun ./target/wasm-gc/debug/build/main/main.wasm
        "#]],
    );

    // release should conflict with debug
    #[cfg(unix)]
    {
        check(
            get_err_stderr(&dir, ["test", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon test --release

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["build", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon build --debug

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["check", "--release", "--debug"]),
            expect![[r#"
                error: the argument '--release' cannot be used with '--debug'

                Usage: moon check --release [PACKAGE_PATH]

                For more information, try '--help'.
            "#]],
        );

        check(
            get_err_stderr(&dir, ["run", "main", "--debug", "--release"]),
            expect![[r#"
                error: the argument '--debug' cannot be used with '--release'

                Usage: moon run --debug <PACKAGE_OR_MBT_FILE> [ARGS]...

                For more information, try '--help'.
            "#]],
        );
    }
}

#[test]
fn test_check_failed_should_write_pkg_json() {
    let dir = TestDir::new("check_failed_should_write_pkg_json.in");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure();

    let pkg_json = dir.join("target/packages.json");
    assert!(pkg_json.exists());
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("moon_run_with_cli_args.in");

    check(
        get_stdout(&dir, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-package ./main/main_wasm.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--dry-run",
                "--",
                "中文",
                "😄👍",
                "hello",
                "1242",
            ],
        ),
        expect![[r#"
            moonc build-package ./main/main_wasm.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm -- 中文 😄👍 hello 1242
        "#]],
    );

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    assert!(s.contains("\"中文\", \"😄👍\", \"hello\", \"1242\", \"--flag\""));

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--target", "js", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    assert!(s.contains("\"中文\", \"😄👍\", \"hello\", \"1242\", \"--flag\""));
}

#[test]
fn test_third_party() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("third_party.in");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);
    get_stdout(&dir, ["build"]);
    get_stdout(&dir, ["clean"]);

    let actual = get_stderr(&dir, ["check"]);
    assert!(actual.contains("moon: ran 4 tasks, now up to date"));

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./.mooncakes/lijunchen/hello18/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core -pkg lijunchen/hello18/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -target wasm-gc -g -O0
            moonc build-package ./lib/test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.mi:lib -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/lijunchen/hello18/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources lijunchen/hello18/lib:./.mooncakes/lijunchen/hello18/lib -pkg-sources username/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Hello, world!
            Hello, world!
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let actual = get_stderr(&dir, ["build"]);
    assert!(actual.contains("moon: ran 3 tasks, now up to date"));

    let actual = get_stdout(&dir, ["run", "main"]);
    assert!(actual.contains("Hello, world!"));
}

#[test]
fn test_moonbitlang_x() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("test_moonbitlang_x.in");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);

    let build_output = get_stdout(&dir, ["build", "--dry-run", "--sort-input"]);

    check(
        &build_output,
        expect![[r#"
            moonc build-package ./.mooncakes/moonbitlang/x/stack/stack.mbt ./.mooncakes/moonbitlang/x/stack/types.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.core -pkg moonbitlang/x/stack -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -target wasm-gc
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -i ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/main:./src/main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./src/main/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    let test_output = get_stdout(&dir, ["test", "--dry-run", "--sort-input"]);
    check(
        &test_output,
        expect![[r#"
            moonc build-package ./.mooncakes/moonbitlang/x/stack/stack.mbt ./.mooncakes/moonbitlang/x/stack/types.mbt -w -a -alert -all -o ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core -pkg moonbitlang/x/stack -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0
            moonc build-package ./src/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./src/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.mi:stack -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/.mooncakes/moonbitlang/x/stack/stack.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources moonbitlang/x/stack:./.mooncakes/moonbitlang/x/stack -pkg-sources username/hello/lib:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    check(
        get_stdout(&dir, ["run", "src/main"]),
        expect![[r#"
            Some(123)
        "#]],
    );
}

#[test]
fn test_blackbox_success() {
    let dir = TestDir::new("blackbox_success_test.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
                "--nostd",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./D/hello.mbt -o ./target/wasm-gc/debug/test/D/D.core -pkg username/hello/D -pkg-sources username/hello/D:./D -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0
            moonc build-package ./C/hello.mbt -o ./target/wasm-gc/debug/test/C/C.core -pkg username/hello/C -pkg-sources username/hello/C:./C -target wasm-gc -g -O0
            moonc build-package ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -i ./target/wasm-gc/debug/test/A/A.mi:A -i ./target/wasm-gc/debug/test/D/D.mi:D -i ./target/wasm-gc/debug/test/C/C.mi:C -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/C/C.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A_blackbox_test -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/C:./C -pkg-sources username/hello/A:./A -pkg-sources username/hello/A_blackbox_test:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./B/hello.mbt -o ./target/wasm-gc/debug/test/B/B.core -pkg username/hello/B -pkg-sources username/hello/B:./B -target wasm-gc -g -O0
            moonc build-package ./A/hello.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/D/D.mi:D -i ./target/wasm-gc/debug/test/B/B.mi:B -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/B/B.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/B:./B -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/A --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./A/hello.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            self.a: 33
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc
            moonc check ./D/hello.mbt -o ./target/wasm-gc/release/check/D/D.mi -pkg username/hello/D -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/D:./D -target wasm-gc
            moonc check ./A/hello.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc
            moonc check ./C/hello.mbt -o ./target/wasm-gc/release/check/C/C.mi -pkg username/hello/C -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/C:./C -target wasm-gc
            moonc check ./A/hello_test.mbt -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -i ./target/wasm-gc/release/check/D/D.mi:D -i ./target/wasm-gc/release/check/C/C.mi:C -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test
            moonc check ./B/hello.mbt -o ./target/wasm-gc/release/check/B/B.mi -pkg username/hello/B -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/B:./B -target wasm-gc
            moonc check ./A/hello.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -i ./target/wasm-gc/release/check/B/B.mi:B -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test
        "#]],
    );

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/A",
                      "root": "username/hello",
                      "rel": "A",
                      "files": {
                        "$ROOT/A/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/A/hello_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "test-files": {
                        "$ROOT/A/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/D",
                          "alias": "D",
                          "fspath": "$ROOT/D"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "username/hello/B",
                          "alias": "B",
                          "fspath": "$ROOT/B"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "username/hello/C",
                          "alias": "C",
                          "fspath": "$ROOT/C"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/A/A.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/B",
                      "root": "username/hello",
                      "rel": "B",
                      "files": {
                        "$ROOT/B/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/B/B.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/C",
                      "root": "username/hello",
                      "rel": "C",
                      "files": {
                        "$ROOT/C/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/C/C.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/D",
                      "root": "username/hello",
                      "rel": "D",
                      "files": {
                        "$ROOT/D/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/D/D.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    }
}

#[test]
fn test_blackbox_failed() {
    let dir = TestDir::new("blackbox_failed_test.in");

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    // bbtest can not use private function in bbtest_import
    assert!(output.contains("Value _private_hello not found in package `A`"));
    // bbtest_import could no be used in _wbtest.mbt
    assert!(output.contains("Package \"C\" not found in the loaded packages."));

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("Warning: Unused variable 'a'"));
    assert!(output.contains("Warning: Unused variable 'b'"));
    assert!(output.contains("Value _private_hello not found in package `A`"));
    assert!(output.contains("Package \"C\" not found in the loaded packages."));
}

#[test]
fn test_blackbox_test_core_override() {
    let dir = TestDir::new("blackbox_test_core_override.in");
    let output = get_stdout(
        &dir,
        ["test", "--enable-coverage", "--dry-run", "--sort-input"],
    );
    check(
        &output,
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/prelude --sort-input --target wasm-gc --driver-kind internal --enable-coverage --mode test
            moonc build-package ./target/wasm-gc/debug/test/prelude/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -pkg moonbitlang/core/prelude -is-main -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -enable-coverage -no-mi
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.internal_test.core -main moonbitlang/core/prelude -o ./target/wasm-gc/debug/test/prelude/prelude.internal_test.wasm -test-mode -pkg-config-path ./prelude/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/builtin --sort-input --target wasm-gc --driver-kind blackbox --enable-coverage --coverage-package-override=@self --mode test
            moonc build-package ./builtin/main.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.core -pkg moonbitlang/core/builtin -pkg-sources moonbitlang/core/builtin:./builtin -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=@self
            moonc build-package -o ./target/wasm-gc/debug/test/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc -g -O0 -enable-coverage
            moonc build-package ./builtin/main_test.mbt ./target/wasm-gc/debug/test/builtin/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.core -pkg moonbitlang/core/builtin_blackbox_test -is-main -i ./target/wasm-gc/debug/test/builtin/builtin.mi:builtin -i ./target/wasm-gc/debug/test/prelude/prelude.mi:prelude -pkg-sources moonbitlang/core/builtin_blackbox_test:./builtin -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=moonbitlang/core/builtin -blackbox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/prelude/prelude.core ./target/wasm-gc/debug/test/builtin/builtin.core ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.core -main moonbitlang/core/builtin_blackbox_test -o ./target/wasm-gc/debug/test/builtin/builtin.blackbox_test.wasm -test-mode -pkg-config-path ./builtin/moon.pkg.json -pkg-sources moonbitlang/core/prelude:./prelude -pkg-sources moonbitlang/core/builtin:./builtin -pkg-sources moonbitlang/core/builtin_blackbox_test:./builtin -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/builtin --sort-input --target wasm-gc --driver-kind internal --enable-coverage --coverage-package-override=@self --mode test
            moonc build-package ./builtin/main.mbt ./target/wasm-gc/debug/test/builtin/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/builtin/builtin.internal_test.core -pkg moonbitlang/core/builtin -is-main -pkg-sources moonbitlang/core/builtin:./builtin -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=@self -no-mi
            moonc link-core ./target/wasm-gc/debug/test/builtin/builtin.internal_test.core -main moonbitlang/core/builtin -o ./target/wasm-gc/debug/test/builtin/builtin.internal_test.wasm -test-mode -pkg-config-path ./builtin/moon.pkg.json -pkg-sources moonbitlang/core/builtin:./builtin -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );

    let mut found = false;
    for line in output.lines() {
        // For the command compiling builtin's blackbox tests,
        if line.contains("moonc build-package") && line.contains("builtin_blackbox_test") {
            found = true;
            // it should have the -enable-coverage flag
            assert!(
                line.contains("-enable-coverage"),
                "No -enable-coverage flag found in the command: {}",
                line
            );
            // and -coverage-package-override to the original package
            assert!(
                line.contains("-coverage-package-override=moonbitlang/core/builtin"),
                "No -coverage-package-override=moonbitlang/core/builtin found in the command: {}",
                line
            );
            // and should not contain -coverage-package-override to itself
            assert!(
                !line.contains("-coverage-package-override=@self"),
                "Unexpected -coverage-package-override=@self found in the command: {}",
                line
            );
        }
    }
    assert!(found, "builtin's blackbox tests not found in the output");
}

#[test]
fn test_blackbox_dedup_alias() {
    std::env::set_var("RUST_BACKTRACE", "0");
    let dir = TestDir::new("blackbox_test_dedup_alias.in");
    let output = get_err_stderr(&dir, ["test"]);
    check(
        &output,
        expect![[r#"
            error: Duplicate alias `lib` at "$ROOT/lib/moon.pkg.json". "test-import" will automatically add "import" and current pkg as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias.
        "#]],
    );
}

#[test]
fn test_import_memory_and_heap_start() {
    let dir = TestDir::new("import_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -heap-start-address 65536
        "#]],
    );

    let dir = TestDir::new("import_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy
        "#]],
    );
}

#[test]
fn test_import_shared_memory() {
    let dir = TestDir::new("import_shared_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm
            moonc link-core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main username/hello/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65536 -shared-memory -heap-start-address 65536
        "#]],
    );

    let dir = TestDir::new("import_shared_memory.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -import-memory-module xxx -import-memory-name yyy -memory-limits-min 1 -memory-limits-max 65535 -shared-memory
        "#]],
    );
}

#[test]
fn test_many_targets() {
    let dir = TestDir::new("test_many_targets.in");
    check(
        get_stdout(&dir, ["test", "--target", "all", "--serial"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0. [wasm]
            Total tests: 0, passed: 0, failed: 0. [wasm-gc]
            Total tests: 0, passed: 0, failed: 0. [js]
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--target", "js,wasm", "--serial"]),
        expect![[r#"
            Total tests: 0, passed: 0, failed: 0. [wasm]
            Total tests: 0, passed: 0, failed: 0. [js]
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./link/hello.mbt -o ./target/wasm/release/check/link/link.mi -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc check ./lib/hello.mbt -o ./target/wasm/release/check/lib/lib.mi -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc check ./link/hello.mbt -o ./target/js/release/check/link/link.mi -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc check ./lib/hello.mbt -o ./target/js/release/check/lib/lib.mi -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./link/hello.mbt -o ./target/wasm/release/build/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc link-core ./target/wasm/release/build/link/link.core -main username/hello/link -o ./target/wasm/release/build/link/link.wasm -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -target wasm
            moonc build-package ./link/hello.mbt -o ./target/js/release/build/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc link-core ./target/js/release/build/link/link.core -main username/hello/link -o ./target/js/release/build/link/link.js -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -target js
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "bundle",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target wasm
            moonc build-package ./link/hello.mbt -o ./target/wasm/release/bundle/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target wasm
            moonc bundle-core ./target/wasm/release/bundle/lib/lib.core ./target/wasm/release/bundle/link/link.core -o ./target/wasm/release/bundle/hello.core
            moonc build-package ./lib/hello.mbt -o ./target/js/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-sources username/hello/lib:./lib -target js
            moonc build-package ./link/hello.mbt -o ./target/js/release/bundle/link/link.core -pkg username/hello/link -pkg-sources username/hello/link:./link -target js
            moonc bundle-core ./target/js/release/bundle/lib/lib.core ./target/js/release/bundle/link/link.core -o ./target/js/release/bundle/hello.core
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
                "-p",
                "username/hello/lib",
                "-f",
                "hello.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm-gc/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/wasm/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core ./target/wasm/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/wasm-gc/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/wasm-gc/debug/test/link/link.internal_test.wasm -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/link --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./link/hello.mbt ./target/js/debug/test/link/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/link/link.internal_test.core -pkg username/hello/link -is-main -pkg-sources username/hello/link:./link -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/link/link.internal_test.core -main username/hello/link -o ./target/js/debug/test/link/link.internal_test.js -test-mode -pkg-config-path ./link/moon.pkg.json -pkg-sources username/hello/link:./link -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target js --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/js/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/js/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -pkg-sources username/hello/lib:./lib -target js -g -O0 -no-mi
            moonc link-core ./target/js/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/js/debug/test/lib/lib.internal_test.js -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_001() {
    let dir = TestDir::new("test_many_targets_auto_update.in");
    let _ = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect!("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect!("wasm-gc", content="wasm-gc")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect!("js")
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
                test {
                  inspect!("native")
                }
            "#]],
    );
}

#[test]
fn test_many_targets_auto_update_002() {
    let dir = TestDir::new("test_many_targets_auto_update.in");
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect!("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect!("wasm-gc")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect!("js", content="js")
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect!("native")
            }
            "#]],
    );

    let _ = get_stdout(
        &dir,
        ["test", "--target", "native", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect!("native", content="native")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_003() {
    let dir = TestDir::new("test_many_targets_auto_update.in");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect!("wasm", content="wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect!("wasm-gc")
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect!("js", content="js")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_004() {
    let dir = TestDir::new("test_many_targets_auto_update.in");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect!("wasm", content="wasm")
            }
        "#]],
    );
    let _ = get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect!("wasm-gc", content="wasm-gc")
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect!("js", content="js")
            }
        "#]],
    );
}

#[test]
fn test_many_targets_expect_failed() {
    let dir = TestDir::new("test_many_targets_expect_failed.in");
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "all", "--serial", "--sort-input"],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:32
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.wasm-gc.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm-gc.mbt:2:3-2:35
            Diff:
            ----
            1wasm-gc
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm-gc]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:30
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
        "#]],
    );
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "js,wasm", "--sort-input", "--serial"],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:32
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:30
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,native",
                "--sort-input",
                "--serial",
            ],
        ),
        expect![[r#"
            test username/hello/lib/x.wasm.mbt::0 failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:32
            Diff:
            ----
            0wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            test username/hello/lib/x.js.mbt::0 failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:30
            Diff:
            ----
            2js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
            test username/hello/lib/x.native.mbt::0 failed
            expect test failed at $ROOT/lib/x.native.mbt:2:3-2:34
            Diff:
            ----
            3native
            ----

            Total tests: 1, passed: 0, failed: 1. [native]
        "#]],
    );
}

#[test]
fn test_moon_run_single_mbt_file() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "js",
            "--build-only",
            "--dry-run",
        ],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/js/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.js -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/target/single.js"]}
        "#]],
    );
    assert!(dir.join("a/b/target/single.js").exists());

    let output = get_stdout(&dir, ["run", "a/b/single.mbt", "--dry-run"]);
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.wasm -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target wasm-gc
            moonrun $ROOT/a/b/target/single.wasm
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--dry-run"],
    );
    check(
        &output,
        expect![[r#"
            moonc build-package $ROOT/a/b/single.mbt -o $ROOT/a/b/target/single.core -std-path $MOON_HOME/lib/core/target/js/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target js
            moonc link-core $MOON_HOME/lib/core/target/js/release/bundle/core.core $ROOT/a/b/target/single.core -o $ROOT/a/b/target/single.js -pkg-sources moon/run/single:$ROOT/a/b -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target js
            node $ROOT/a/b/target/single.js
        "#]],
    );

    let output = get_stdout(&dir, ["run", "a/b/single.mbt"]);
    check(
        &output,
        expect![[r#"
        I am OK
    "#]],
    );

    let output = get_stdout(&dir.join("a").join("b").join("c"), ["run", "../single.mbt"]);
    check(
        &output,
        expect![[r#"
            I am OK
            "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "js"],
    );
    check(
        &output,
        expect![[r#"
        I am OK
        "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "native"],
    );
    // cl have other output
    assert!(output.contains("I am OK"));
}

#[test]
fn test_moon_check_json_output() {
    let dir = TestDir::new("alert_list.in");

    #[cfg(unix)]
    {
        check(
            get_stdout(&dir, ["check", "--output-json", "-q"]),
            expect![[r#"
                {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/main/main.mbt","start":{"line":3,"col":3},"end":{"line":3,"col":10}},"message":"Warning (Alert alert_2): alert_2","error_code":2000}
            "#]],
        );
        check(
            get_stderr(&dir, ["check", "--output-json", "-q"]),
            expect![""],
        );
        check(
            get_stderr(&dir, ["check", "--output-json"]),
            expect![[r#"
                Finished. moon: ran 1 task, now up to date
            "#]],
        );
        check(
            get_stderr(&dir, ["check", "--output-json"]),
            expect![[r#"
                Finished. moon: ran 1 task, now up to date
            "#]],
        );
    }

    // windows crlf(\r\n)
    #[cfg(windows)]
    {
        check(
            get_stdout(&dir, ["check", "--output-json", "-q"]),
            expect![[r#"
            {"$message_type":"diagnostic","level":"warning","loc":{"path":"$ROOT/main/main.mbt","start":{"line":3,"col":3},"end":{"line":3,"col":10}},"message":"Warning (Alert alert_2): alert_2","error_code":2000}
        "#]],
        );
        check(
            get_stderr(&dir, ["check", "--output-json", "-q"]),
            expect![""],
        );
        check(
            get_stderr(&dir, ["check", "--output-json"]),
            expect![[r#"
                Finished. moon: ran 1 task, now up to date
            "#]],
        );
        check(
            get_stderr(&dir, ["check", "--output-json"]),
            expect![[r#"
                Finished. moon: ran 1 task, now up to date
            "#]],
        );
    }
}

#[test]
fn test_moon_run_single_mbt_file_inside_a_pkg() {
    let dir = TestDir::new("run_single_mbt_file_inside_pkg.in");

    let output = get_stdout(&dir, ["run", "main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir, ["run", "lib/main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(&dir.join("lib"), ["run", "../main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib"), ["run", "main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib").join("main_in_lib"), ["run", "main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );
}

#[test]
fn moon_test_parallelize_should_success() {
    let dir = TestDir::new("test_filter_pkg_with_test_imports.in");

    let output = get_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));

    let output = get_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 14, passed: 14, failed: 0."));

    let dir = TestDir::new("test_filter.in");

    let output = get_err_stdout(&dir, ["test"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_err_stdout(&dir, ["test", "--target", "native"]);
    assert!(output.contains("Total tests: 13, passed: 11, failed: 2."));

    let output = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));

    let output = get_stdout(
        &dir,
        ["test", "-u", "--no-parallelize", "--target", "native"],
    );
    assert!(output.contains("Total tests: 13, passed: 13, failed: 0."));
}

#[test]
fn test_specify_source_dir_001() {
    let dir = TestDir::new("specify_source_dir_001.in");
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./src/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc
            moonc check ./src/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc
            moonc check ./src/lib/hello_test.mbt -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -blackbox-test
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc
            moonc build-package ./src/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./src/main/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/main:./src/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./src/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0
            moonc build-package ./src/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./src/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./src/lib/moon.pkg.json -pkg-sources username/hello/lib:./src/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/lib",
                      "root": "username/hello",
                      "rel": "lib",
                      "files": {
                        "$ROOT/src/lib/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {
                        "$ROOT/src/lib/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/lib/lib.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/src/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        )
    }
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_002() {
    let dir = TestDir::new("specify_source_dir_002.in");
    check(
        get_err_stdout(&dir, ["test"]),
        expect![[r#"
            test username/hello/lib/hello_test.mbt::hello failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:2:3-2:25
            Diff:
            ----
            Hello, world!
            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-u", "--no-parallelize"]),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src").join("lib").join("hello_test.mbt")),
        expect![[r#"
            test "hello" {
              inspect!(@lib.hello(), content="Hello, world!")
            }
        "#]],
    );
}

#[test]
fn test_specify_source_dir_003() {
    let dir = TestDir::new("specify_source_dir_003_empty_string.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 1 task, now up to date
        "#]],
    );
}

#[test]
fn test_specify_source_dir_004() {
    let dir = TestDir::new("specify_source_dir_004.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    get_stdout(&dir, ["clean"]);
    check(
        get_stdout(
            &dir,
            ["run", "nes/t/ed/src/main", "--target", "js", "--build-only"],
        ),
        expect![[r#"
            {"artifacts_path":["$ROOT/target/js/release/build/main/main.js"]}
        "#]],
    );
    assert!(dir.join("target/js/release/build/main/main.js").exists());

    check(
        get_stdout(&dir, ["run", "nes/t/ed/src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_005() {
    let dir = TestDir::new("specify_source_dir_005_bad.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: failed to load `$ROOT/moon.mod.json`

            Caused by:
                0: `source` bad format
                1: `source` not a subdirectory of the parent directory
        "#]],
    );
}

#[test]
fn test_specify_source_dir_with_deps() {
    let dir = TestDir::new("specify_source_dir_with_deps_001.in");
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./anyhow/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc
            moonc check ./deps/hello19/source/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/check/.mooncakes/just/hello19/hello19.mi -pkg just/hello19 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19:./deps/hello19/source -target wasm-gc
            moonc check ./deps/hello19/source/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/check/.mooncakes/just/hello19/lib/lib.mi -pkg just/hello19/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19/lib:./deps/hello19/source/lib -target wasm-gc
            moonc check ./anyhow/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:emmm -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/release/check/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc
            moonc check ./anyhow/lib/hello_test.mbt -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -target wasm-gc -blackbox-test
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./anyhow/lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc
            moonc build-package ./deps/hello19/source/top.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.core -pkg just/hello19 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19:./deps/hello19/source -target wasm-gc
            moonc build-package ./deps/hello19/source/lib/hello.mbt -w -a -alert -all -o ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.core -pkg just/hello19/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources just/hello19/lib:./deps/hello19/source/lib -target wasm-gc
            moonc build-package ./anyhow/main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:emmm -i ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.mi:hello19 -i ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.mi:lib -pkg-sources username/hello/main:./anyhow/main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/.mooncakes/just/hello19/hello19.core ./target/wasm-gc/release/build/.mooncakes/just/hello19/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main username/hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./anyhow/main/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources just/hello19:./deps/hello19/source -pkg-sources just/hello19/lib:./deps/hello19/source/lib -pkg-sources username/hello/main:./anyhow/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./anyhow/lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -g -O0
            moonc build-package ./anyhow/lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg username/hello/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main username/hello/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./anyhow/lib/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources username/hello/lib_blackbox_test:./anyhow/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package username/hello/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./anyhow/lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg username/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./anyhow/lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main username/hello/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./anyhow/lib/moon.pkg.json -pkg-sources username/hello/lib:./anyhow/lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./anyhow/main"]),
        expect![[r#"
            Hello, world!
            hello
            world
        "#]],
    );
}

#[test]
fn test_specify_source_dir_with_deps_002() {
    let dir = TestDir::new("specify_source_dir_with_deps_002.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 9 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 10 tasks, now up to date
        "#]],
    );
    check(get_stdout(&dir, ["test"]), expect![""]);
    check(
        get_stdout(&dir, ["run", "./anyhow"]),
        expect![[r#"
            a!b!c!d!
            one!two!three!four!
        "#]],
    );
}

#[test]
fn test_snapshot_test() {
    let dir = TestDir::new("snapshot_testing.in");
    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            test username/hello/lib/hello_test.mbt::snapshot in blackbox test failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff:
            ----
            Hello, world!
            ----

            test username/hello/lib/hello.mbt::test inspect 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:16
            Diff:
            ----
            a
            ----

            test username/hello/lib/hello.mbt::test snapshot 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff:
            ----
            hello
            snapshot
            testing

            ----

            test username/hello/lib/hello.mbt::test inspect 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:16
            Diff:
            ----
            c
            ----

            test username/hello/lib/hello.mbt::test snapshot 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff:
            ----
            should
            be
            work

            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--sort-input",
                "--no-parallelize",
                "--target",
                "native",
            ],
        ),
        expect![[r#"
            test username/hello/lib/hello_test.mbt::snapshot in blackbox test failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff:
            ----
            Hello, world!
            ----

            test username/hello/lib/hello.mbt::test inspect 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:16
            Diff:
            ----
            a
            ----

            test username/hello/lib/hello.mbt::test snapshot 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff:
            ----
            hello
            snapshot
            testing

            ----

            test username/hello/lib/hello.mbt::test inspect 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:16
            Diff:
            ----
            c
            ----

            test username/hello/lib/hello.mbt::test snapshot 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff:
            ----
            should
            be
            work

            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "-u", "--no-parallelize"]),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect!("a", content="a")
              inspect!("b", content="b")
            }

            test "test snapshot 1" (it : @test.T) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect!("c", content="c")
              inspect!("d", content="d")
            }

            test "test snapshot 2" (it : @test.T) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
    );
}

#[test]
fn test_snapshot_test_target_js() {
    let dir = TestDir::new("snapshot_testing.in");
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "js", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test username/hello/lib/hello_test.mbt::snapshot in blackbox test failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:9:3
            Diff:
            ----
            Hello, world!
            ----

            test username/hello/lib/hello.mbt::test inspect 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:6:3-6:16
            Diff:
            ----
            a
            ----

            test username/hello/lib/hello.mbt::test snapshot 1 failed
            expect test failed at $ROOT/src/lib/hello.mbt:14:3
            Diff:
            ----
            hello
            snapshot
            testing

            ----

            test username/hello/lib/hello.mbt::test inspect 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:18:3-18:16
            Diff:
            ----
            c
            ----

            test username/hello/lib/hello.mbt::test snapshot 2 failed
            expect test failed at $ROOT/src/lib/hello.mbt:26:3
            Diff:
            ----
            should
            be
            work

            ----

            Total tests: 6, passed: 1, failed: 5.
        "#]],
    );
    assert!(dir.join("target/js/debug/test/package.json").exists());
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "js",
                "-u",
                "--sort-input",
                "--no-parallelize",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            pub fn hello() -> String {
              "Hello, world!"
            }

            test "test inspect 1" {
              inspect!("a", content="a")
              inspect!("b", content="b")
            }

            test "test snapshot 1" (it : @test.T) {
              it.writeln("hello")
              it.writeln("snapshot")
              it.writeln("testing")
              it.snapshot!(filename="001.txt")
            }

            test "test inspect 2" {
              inspect!("c", content="c")
              inspect!("d", content="d")
            }

            test "test snapshot 2" (it : @test.T) {
              it.writeln("should")
              it.writeln("be")
              it.writeln("work")
              it.snapshot!(filename="002.txt")
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/001.txt")),
        expect![[r#"
        hello
        snapshot
        testing
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/002.txt")),
        expect![[r#"
        should
        be
        work
    "#]],
    );
    check(
        read(dir.join("src/lib/__snapshot__/003.txt")),
        expect!["Hello, world!"],
    );
}

#[test]
fn moon_test_with_failure_json() {
    let dir = TestDir::new("test_with_failure_json");

    let output = get_err_stdout(&dir, ["test", "--test-failure-json"]);
    check(
        &output,
        // should keep in this format, it's used in ide test explorer
        expect![[r#"
            {"package":"username/hello/lib1","filename":"hello.mbt","index":"0","test_name":"test_1","message":"FAILED: $ROOT/src/lib1/hello.mbt:7:3-7:25 test_1 failed"}
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_js() {
    let dir = TestDir::new("test_filter.in");

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "-f",
            "hello_wbtest.mbt",
            "-i",
            "1",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test hello_1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_doc_dry_run() {
    let dir = TestDir::new("moon_doc.in");
    check(
        get_stdout(&dir, ["doc", "--dry-run"]),
        expect![[r#"
            moonc check ./src/lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./src/lib -target wasm-gc
            moonc check ./src/main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./src/main -target wasm-gc
            moonc check ./src/lib/hello_test.mbt -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/lib_blackbox_test:./src/lib -target wasm-gc -blackbox-test
            moondoc $ROOT -o $ROOT/target/doc -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -packages-json $ROOT/target/packages.json
        "#]],
    );
}

#[test]
fn test_moon_doc() {
    let dir = TestDir::new("moon_doc.in");
    let _ = get_stderr(&dir, ["doc"]);
    check(
        read(dir.join("target/doc/username/hello/lib/members.md")),
        expect![[r#"
            # Documentation
            |Value|description|
            |---|---|
            |[hello](#hello)||

            ## hello

            ```moonbit
            :::source,username/hello/lib/hello.mbt,1:::fn hello() -> String
            ```

        "#]],
    );
    check(
        read(dir.join("target/doc/username/hello/main/members.md")),
        expect!["# Documentation"],
    );
    check(
        read(dir.join("target/doc/username/hello/_sidebar.md")),
        expect![[r#"
            - [username/hello](username/hello/)
            - **In this module**
              - [lib](username/hello/lib/members)
              - [main](username/hello/main/members)
            - **Dependencies**
              - [moonbitlang/core](moonbitlang/core/)"#]],
    );
}

#[test]
fn test_failed_to_fill_whole_buffer() {
    let dir = TestDir::new("hello.in");
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 1 task, now up to date
        "#]],
    );
    let moon_db_path = dir.join("./target/wasm-gc/release/check/check.moon_db");
    if moon_db_path.exists() {
        std::fs::remove_file(&moon_db_path).unwrap();
    }
    std::fs::write(&moon_db_path, "").unwrap();
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            error: internal error

            Caused by:
                0: failed to open n2 database
                1: failed to open $ROOT/target/wasm-gc/release/check/check.moon_db
                2: failed to read
                3: failed to fill whole buffer
        "#]],
    );
}

#[test]
fn test_moon_update_failed() {
    if std::env::var("CI").is_err() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let moon_home = dir;
    let out = std::process::Command::new(moon_bin())
        .current_dir(dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
        Registry index cloned successfully
    "#]],
    );

    let _ = std::process::Command::new("git")
        .args([
            "-C",
            dir.join("registry").join("index").to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            "whatever",
        ])
        .output()
        .unwrap();

    let out = std::process::Command::new(moon_bin())
        .current_dir(dir)
        .env("MOON_HOME", moon_home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["update"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    check(
        &out,
        expect![[r#"
            Registry index is not cloned from the same URL, re-cloning
            Registry index re-cloned successfully
        "#]],
    );
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceResult(Vec<TraceEvent>);

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pid: u64,
    name: String,
    ts: u64,
    tid: u64,
    ph: String,
    dur: u64,
}

#[test]
fn test_trace_001() {
    let dir = TestDir::new("hello.in");
    let _ = get_stdout(&dir, ["build", "--trace"]);
    let s = replace_dir(&read(dir.join("trace.json")), &dir);
    let j: TraceResult = serde_json::from_str(&s).unwrap();
    let event_names = j.0.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    check(
        format!("{:#?}", event_names),
        expect![[r#"
            [
                "moonbit::build::read",
                "moonc build-package -error-format json $ROOT/main/main.mbt -o $ROOT/target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:$ROOT/main -target wasm-gc",
                "moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/wasm-gc/release/build/main/main.core -main hello/main -o $ROOT/target/wasm-gc/release/build/main/main.wasm -pkg-config-path $ROOT/main/moon.pkg.json -pkg-sources hello/main:$ROOT/main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc",
                "work.run",
                "main",
            ]"#]],
    );
    for e in j.0.iter() {
        assert!(e.dur > 0);
    }
}

#[test]
fn no_main_just_init() {
    let dir = TestDir::new("no_main_just_init.in");
    get_stdout(&dir, ["build"]);
    let file = dir.join("target/wasm-gc/release/build/lib/lib.wasm");
    assert!(file.exists());

    let out = snapbox::cmd::Command::new("moonrun")
        .current_dir(&dir)
        .args(["./target/wasm-gc/release/build/lib/lib.wasm"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            I am in fn init { ... }
        "#]],
    );
}

#[test]
fn test_pre_build() {
    let dir = TestDir::new("pre_build.in");

    // replace CRLF with LF on Windows
    let b_txt_path = dir.join("src/lib/b.txt");
    std::fs::write(&b_txt_path, read(&b_txt_path)).unwrap();

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Executed 3 pre-build tasks, now up to date
            Warning: [1002]
               ╭─[$ROOT/src/lib/a.mbt:4:5]
               │
             4 │ let resource : String =
               │     ────┬───  
               │         ╰───── Warning: Unused toplevel variable 'resource'. Note if the body contains side effect, it will not happen. Use `fn init { .. }` to wrap the effect.
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Warning: [1002]
               ╭─[$ROOT/src/lib/a.mbt:4:5]
               │
             4 │ let resource : String =
               │     ────┬───  
               │         ╰───── Warning: Unused toplevel variable 'resource'. Note if the body contains side effect, it will not happen. Use `fn init { .. }` to wrap the effect.
            ───╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        read(dir.join("src/lib/a.mbt")),
        expect![[r#"
            // Generated by `moon tool embed --text`, do not edit.

            ///|
            let resource : String =
              #|hello,
              #|world
              #|
        "#]],
    );
    let content = read(dir.join("src/lib/b.mbt"));
    check(
        content,
        expect![[r#"
            // Generated by `moon tool embed --binary`, do not edit.

            ///|
            let _b : Bytes = Bytes::of([
              0x4d, 0x6f, 0x6f, 0x6e, 0x42, 0x69, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 
              0x6e, 0x20, 0x65, 0x6e, 0x64, 0x2d, 0x74, 0x6f, 0x2d, 0x65, 0x6e, 0x64, 
              0x20, 0x70, 0x72, 0x6f, 0x67, 0x72, 0x61, 0x6d, 0x6d, 0x69, 0x6e, 0x67, 
              0x20, 0x6c, 0x61, 0x6e, 0x67, 0x75, 0x61, 0x67, 0x65, 0x20, 0x74, 0x6f, 
              0x6f, 0x6c, 0x63, 0x68, 0x61, 0x69, 0x6e, 0x20, 0x66, 0x6f, 0x72, 0x20, 
              0x63, 0x6c, 0x6f, 0x75, 0x64, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x65, 0x64, 
              0x67, 0x65, 0x0a, 0x63, 0x6f, 0x6d, 0x70, 0x75, 0x74, 0x69, 0x6e, 0x67, 
              0x20, 0x75, 0x73, 0x69, 0x6e, 0x67, 0x20, 0x57, 0x65, 0x62, 0x41, 0x73, 
              0x73, 0x65, 0x6d, 0x62, 0x6c, 0x79, 0x2e, 0x20, 0x54, 0x68, 0x65, 0x20, 
              0x49, 0x44, 0x45, 0x20, 0x65, 0x6e, 0x76, 0x69, 0x72, 0x6f, 0x6e, 0x6d, 
              0x65, 0x6e, 0x74, 0x20, 0x69, 0x73, 0x20, 0x61, 0x76, 0x61, 0x69, 0x6c, 
              0x61, 0x62, 0x6c, 0x65, 0x20, 0x61, 0x74, 0x0a, 0x68, 0x74, 0x74, 0x70, 
              0x73, 0x3a, 0x2f, 0x2f, 0x74, 0x72, 0x79, 0x2e, 0x6d, 0x6f, 0x6f, 0x6e, 
              0x62, 0x69, 0x74, 0x6c, 0x61, 0x6e, 0x67, 0x2e, 0x63, 0x6f, 0x6d, 0x20, 
              0x77, 0x69, 0x74, 0x68, 0x6f, 0x75, 0x74, 0x20, 0x61, 0x6e, 0x79, 0x20, 
              0x69, 0x6e, 0x73, 0x74, 0x61, 0x6c, 0x6c, 0x61, 0x74, 0x69, 0x6f, 0x6e, 
              0x3b, 0x20, 0x69, 0x74, 0x20, 0x64, 0x6f, 0x65, 0x73, 0x20, 0x6e, 0x6f, 
              0x74, 0x20, 0x72, 0x65, 0x6c, 0x79, 0x20, 0x6f, 0x6e, 0x20, 0x61, 0x6e, 
              0x79, 0x0a, 0x73, 0x65, 0x72, 0x76, 0x65, 0x72, 0x20, 0x65, 0x69, 0x74, 
              0x68, 0x65, 0x72, 0x2e, 
            ])
        "#]],
    );
    check(
        read(dir.join("src/lib/c.mbt")),
        expect![[r#"
            // Generated by `moon tool embed --text`, do not edit.

            ///|
            let _c : String =
              #|hello,
              #|world
              #|
        "#]],
    );
}

#[test]
fn test_moon_coverage() {
    let dir = TestDir::new("test_coverage.in");

    get_stdout(&dir, ["test", "--enable-coverage", "--target", "wasm-gc"]);
    // just get the last line, since other output contains path sequence which is not stable
    check(
        get_stdout(&dir, ["coverage", "report", "-f", "summary"])
            .lines()
            .last()
            .unwrap(),
        expect!["Total: 3/6"],
    );

    get_stdout(&dir, ["clean"]);
    get_stdout(&dir, ["test", "--enable-coverage", "--target", "wasm"]);
    check(
        get_stdout(&dir, ["coverage", "report", "-f", "summary"])
            .lines()
            .last()
            .unwrap(),
        expect!["Total: 3/6"],
    );

    get_stdout(&dir, ["clean"]);
    get_stdout(&dir, ["test", "--enable-coverage", "--target", "js"]);
    check(
        get_stdout(&dir, ["coverage", "report", "-f", "summary"])
            .lines()
            .last()
            .unwrap(),
        expect!["Total: 3/6"],
    );
}

#[test]
fn test_bad_version() {
    let dir = TestDir::new("general.in");
    let content = std::fs::read_to_string(dir.join("moon.mod.json")).unwrap();
    let mut moon_mod: MoonModJSON = serde_json::from_str(&content).unwrap();
    moon_mod.version = Some("0.0".to_string());
    std::fs::write(
        dir.join("moon.mod.json"),
        serde_json::to_string(&moon_mod).unwrap(),
    )
    .unwrap();
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
        error: failed to load `$ROOT/moon.mod.json`

        Caused by:
            0: `version` bad format
            1: unexpected end of input while parsing minor version number
    "#]],
    );
}

#[test]
fn test_moonfmt() {
    let dir = TestDir::new("general.in");
    let oneline = r#"pub fn hello() -> String { "Hello, world!" }"#;

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();

    let out = std::process::Command::new("moonfmt")
        .args(["./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let out = String::from_utf8(out.stdout).unwrap().replace_crlf_to_lf();
    check(
        &out,
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );

    std::fs::write(dir.join("src/lib/hello.mbt"), oneline).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"pub fn hello() -> String { "Hello, world!" }"#]],
    );

    let out = std::process::Command::new("moonfmt")
        .args(["-i", "./src/lib/hello.mbt", "-o", "./src/lib/hello.txt"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = String::from_utf8(out.stdout).unwrap();
    check(
        read(dir.join("src/lib/hello.mbt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
    check(
        read(dir.join("src/lib/hello.txt")),
        expect![[r#"
            ///|
            pub fn hello() -> String {
              "Hello, world!"
            }
        "#]],
    );
}

#[cfg(unix)]
#[test]
fn test_use_cc_for_native_release() {
    let dir = TestDir::new("moon_test_hello_exec_fntest.in");
    // build
    {
        check(
            get_stdout(
                &dir,
                [
                    "build",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
            "#]],
        );
        // if --release is not specified, it should not use cc
        check(
            get_stdout(
                &dir,
                ["build", "--target", "native", "--sort-input", "--dry-run"],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
            "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "build",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0
                moonc build-package ./main/main.mbt -o ./target/native/debug/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/build/lib/lib.core ./target/native/debug/build/main/main.core -main moonbitlang/hello/main -o ./target/native/debug/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native -g -O0
                cc -o ./target/native/debug/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/debug/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g -fwrapv -fno-strict-aliasing -Og $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/build/main/main.c ./target/native/debug/build/runtime.o -lm
            "#]],
        );
    }

    // run
    {
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
                ./target/native/release/build/main/main.exe
            "#]],
        );
        // if --release is not specified, it should not use cc
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native
                moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moonbitlang/hello/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
                cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o -lm
                ./target/native/release/build/main/main.exe
            "#]],
        );
        check(
            get_stdout(
                &dir,
                [
                    "run",
                    "main",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moonc build-package ./lib/hello.mbt -o ./target/native/debug/build/lib/lib.core -pkg moonbitlang/hello/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0
                moonc build-package ./main/main.mbt -o ./target/native/debug/build/main/main.core -pkg moonbitlang/hello/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/debug/build/lib/lib.mi:lib -pkg-sources moonbitlang/hello/main:./main -target native -g -O0
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/build/lib/lib.core ./target/native/debug/build/main/main.core -main moonbitlang/hello/main -o ./target/native/debug/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native -g -O0
                cc -o ./target/native/debug/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/debug/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g -fwrapv -fno-strict-aliasing -Og $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/build/main/main.c ./target/native/debug/build/runtime.o -lm
                ./target/native/debug/build/main/main.exe
            "#]],
        );
    }

    // test
    {
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--release",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind whitebox --release --mode test
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/release/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/release/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -whitebox-test -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/release/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
                cc -o ./target/native/release/test/lib/lib.whitebox_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/lib/lib.whitebox_test.c ./target/native/release/test/runtime.o -lm
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind internal --release --mode test
                moonc build-package ./lib/hello.mbt ./target/native/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/release/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/release/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native
                cc -o ./target/native/release/test/lib/lib.internal_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/test/lib/lib.internal_test.c ./target/native/release/test/runtime.o -lm
            "#]],
        );

        // use tcc for debug test
        #[cfg(target_os = "macos")]
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind whitebox --mode test
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -whitebox-test -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind internal --mode test
                moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                cc -o ./target/native/debug/test/libruntime.dylib -I$MOON_HOME/include -L$MOON_HOME/lib -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            "#]],
        );
        #[cfg(target_os = "linux")]
        check(
            get_stdout(
                &dir,
                [
                    "test",
                    "--target",
                    "native",
                    "--debug",
                    "--sort-input",
                    "--dry-run",
                ],
            ),
            expect![[r#"
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind whitebox --mode test
                moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/native/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/native/debug/test/lib/lib.whitebox_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -whitebox-test -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.whitebox_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.whitebox_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/hello/lib --sort-input --target native --driver-kind internal --mode test
                moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moonbitlang/hello/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moonbitlang/hello/lib:./lib -target native -g -O0 -no-mi
                moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moonbitlang/hello/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moonbitlang/hello/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
                cc -o ./target/native/debug/test/libruntime.so -I$MOON_HOME/include -L$MOON_HOME/lib -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            "#]],
        );
    }
}

#[test]
fn test_moon_package_list() {
    let dir = TestDir::new("test_publish.in");
    check(
        get_stderr(&dir, ["package", "--list"]),
        expect![[r#"
            Running moon check ...
            Finished. moon: ran 3 tasks, now up to date
            Check passed
            README.md
            moon.mod.json
            src
            src/lib
            src/lib/hello.mbt
            src/lib/hello_test.mbt
            src/lib/moon.pkg.json
            src/main
            src/main/main.mbt
            src/main/moon.pkg.json
            Package to $ROOT/target/publish/username-hello-0.1.0.zip
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_cc_flags() {
    let dir = TestDir::new("native_backend_cc_flags.in");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core -main moon_new/lib -o ./target/wasm-gc/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target native --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c ./lib/stub.c stubccflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            cc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            cc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g $MOON_HOME/lib/libmoonbitrun.o ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(&dir, ["test", "--target", "wasm", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moon_new/lib:./lib -target wasm -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );
    // don't pass native cc flags for no native backend
    check(
        get_stdout(
            &dir,
            [
                "run",
                "main",
                "--target",
                "wasm",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -pkg-sources moon_new/lib:./lib -target wasm
            moonc build-package ./main/main.mbt -o ./target/wasm/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm/release/bundle -i ./target/wasm/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core ./target/wasm/release/build/main/main.core -main moon_new/main -o ./target/wasm/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm
            moonc link-core $MOON_HOME/lib/core/target/wasm/release/bundle/core.core ./target/wasm/release/build/lib/lib.core -main moon_new/lib -o ./target/wasm/release/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm
            moonrun ./target/wasm/release/build/lib/lib.wasm
            moonrun ./target/wasm/release/build/main/main.wasm
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_cc_flags_with_env_override() {
    let dir = TestDir::new("native_backend_cc_flags.in");
    check(
        get_stdout_with_envs(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target native --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c ./lib/stub.c stubccflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
            [("MOON_CC", "x86_64-unknown-fake_os-fake_libc-gcc")],
        ),
        expect![[r#"
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target native --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c ./lib/stub.c stubccflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/debug/test/lib/liblib.a ./target/native/debug/test/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/debug/test/lib/lib.internal_test.exe -I$MOON_HOME/include -L$MOON_HOME/lib -g ./target/native/debug/test/lib/lib.internal_test.c ./target/native/debug/test/runtime.o ./target/native/debug/test/lib/liblib.a -lm ccflags fasd cclinkflags
        "#]],
    );

    check(
        get_stdout_with_envs(
            &dir,
            [
                "run",
                "main",
                "--target",
                "native",
                "--dry-run",
                "--sort-input",
            ],
            [
                (
                    "MOON_CC",
                    "/some/path/A/x86_64-unknown-fake_os-fake_libc-gcc",
                ),
                (
                    "MOON_AR",
                    "/other/path/B/x86_64-unknown-fake_os-fake_libc-ar",
                ),
            ],
        ),
        expect![[r#"
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            /other/path/B/x86_64-unknown-fake_os-fake_libc-ar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            /some/path/A/x86_64-unknown-fake_os-fake_libc-gcc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm ccflags fasd cclinkflags
            ./target/native/release/build/lib/lib.exe
            ./target/native/release/build/main/main.exe
        "#]],
    );
}

#[test]
#[cfg(unix)]
fn test_native_backend_tcc_run() {
    let dir = TestDir::new("native_backend_tcc_run.in");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/release/build/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -c ./lib/stub.c stubccflags
            moonc build-package ./lib/hello.mbt -o ./target/native/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native
            moonc build-package ./main/main.mbt -o ./target/native/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -i ./target/native/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target native
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core ./target/native/release/build/main/main.core -main moon_new/main -o ./target/native/release/build/main/main.c -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/runtime.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c
            stubar -r -c -s ./target/native/release/build/lib/liblib.a ./target/native/release/build/lib/stub.o
            cc -o ./target/native/release/build/main/main.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/main/main.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/release/build/lib/lib.core -main moon_new/lib -o ./target/native/release/build/lib/lib.c -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target native
            cc -o ./target/native/release/build/lib/lib.exe -I$MOON_HOME/include -L$MOON_HOME/lib -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/libmoonbitrun.o ./target/native/release/build/lib/lib.c ./target/native/release/build/runtime.o ./target/native/release/build/lib/liblib.a -lm
        "#]],
    );

    #[cfg(target_os = "macos")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            cc -o ./target/native/debug/test/libruntime.dylib -I$MOON_HOME/include -L$MOON_HOME/lib -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/liblib.dylib -L$MOON_HOME/lib -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target native --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );

    #[cfg(target_os = "linux")]
    check(
        get_stdout(
            &dir,
            ["test", "--target", "native", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            stubcc -o ./target/native/debug/test/lib/stub.o -I$MOON_HOME/include -L$MOON_HOME/lib -g -c -fPIC -DMOONBIT_USE_SHARED_RUNTIME ./lib/stub.c stubccflags
            cc -o ./target/native/debug/test/libruntime.so -I$MOON_HOME/include -L$MOON_HOME/lib -g -shared -fPIC -fwrapv -fno-strict-aliasing -O2 $MOON_HOME/lib/runtime.c -lm
            stubcc -o ./target/native/debug/test/lib/liblib.so -L$MOON_HOME/lib -L./target/native/debug/test -shared -fPIC ./target/native/debug/test/lib/stub.o -lm -lruntime -Wl,-rpath,./target/native/debug/test stubcclinkflags
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target native --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/native/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/native/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/native/release/bundle -pkg-sources moon_new/lib:./lib -target native -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/native/release/bundle/core.core ./target/native/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/native/debug/test/lib/lib.internal_test.c -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target native -g -O0
        "#]],
    );
}

#[test]
fn test_moon_check_filter_package() {
    let dir = TestDir::new("test_check_filter.in");

    check(
        get_stdout(&dir, ["check", "A", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc
            moonc check ./A/hello_test.mbt -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "lib", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );
}

#[test]
fn test_moon_check_package_with_patch() {
    let dir = TestDir::new("test_check_filter.in");

    // A has no deps
    check(
        get_stdout(
            &dir,
            [
                "check",
                "A",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check -patch-file /path/to/patch.json ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc
            moonc check ./A/hello_test.mbt -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "A",
                "--patch-file",
                "/path/to/patch_wbtest.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc
            moonc check ./A/hello_test.mbt -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test
            moonc check -patch-file /path/to/patch_wbtest.json ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "A",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc
            moonc check -patch-file /path/to/patch_test.json ./A/hello_test.mbt -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test
        "#]],
    );

    // lib has dep lib2
    check(
        get_stdout(
            &dir,
            [
                "check",
                "lib",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check -patch-file /path/to/patch.json ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "lib",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc
        "#]],
    );

    // main has dep lib
    check(
        get_stdout(
            &dir,
            [
                "check",
                "main",
                "--patch-file",
                "/path/to/patch.json",
                "--no-mi",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check -patch-file /path/to/patch.json -no-mi ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
        "#]],
    );
}

#[test]
fn test_no_mi_for_test_pkg() {
    let dir = TestDir::new("test_check_filter.in");

    get_stdout(&dir, ["test", "-p", "username/hello/A"]);

    // .mi should not be generated for test package
    let mi_path = dir.join("target/wasm-gc/debug/test/A/A.internal_test.mi");
    assert!(!mi_path.exists());

    // .core should be generated for test package
    let core_path = dir.join("target/wasm-gc/debug/test/A/A.internal_test.core");
    assert!(core_path.exists());
}

#[test]
fn test_moon_test_patch() {
    let dir = TestDir::new("moon_test_patch.in");
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind internal --patch-file ./patch.json --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -no-mi -patch-file ./patch.json
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
            ],
        ),
        expect![[r#"
            hello from patch.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind blackbox --mode test
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -blackbox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind whitebox --patch-file ./patch_wbtest.json --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -whitebox-test -no-mi -patch-file ./patch_wbtest.json
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from patch_wbtest.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
                "--dry-run",
                "--sort-input",
                "--nostd",
            ],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind blackbox --patch-file ./patch_test.json --mode test
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0
            moonc build-package ./lib/hello_test.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -i ./target/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -blackbox-test -no-mi -patch-file ./patch_test.json
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.core ./target/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./target/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./lib/hello.mbt ./lib/hello_wbtest.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -whitebox-test -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.whitebox_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.whitebox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --sort-input --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
            ],
        ),
        expect![[r#"
            hello from patch_test.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // no _test.mbt and _wbtest.mbt in original package
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "-f",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_test.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "-f",
                "hello_2_wbtest.mbt",
                "--patch-file",
                "./2.patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_wbtest.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_render_diagnostic_in_patch_file() {
    let dir = TestDir::new("moon_test_patch.in");
    check(
        get_stderr(&dir, ["check", "lib", "--patch-file", "./patch_test.json"]),
        expect![[r#"
            Warning: [1002]
               ╭─[hello_2_test.mbt:2:6]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning: Unused variable 'unused_in_patch_test_json'
            ───╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(
            &dir,
            ["check", "lib", "--patch-file", "./patch_wbtest.json"],
        ),
        expect![[r#"
            Warning: [1002]
               ╭─[hello_1_wbtest.mbt:2:6]
               │
             2 │  let unused_in_patch_wbtest_json = 1;
               │      ─────────────┬─────────────  
               │                   ╰─────────────── Warning: Unused variable 'unused_in_patch_wbtest_json'
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "lib", "--patch-file", "./patch.json"]),
        expect![[r#"
            Warning: [1002]
               ╭─[hello_0.mbt:2:6]
               │
             2 │  let unused_in_patch_json = 1;
               │      ──────────┬─────────  
               │                ╰─────────── Warning: Unused variable 'unused_in_patch_json'
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    // check --explain
    check(
        get_stderr(
            &dir,
            [
                "check",
                "lib",
                "--patch-file",
                "./patch_test.json",
                "--explain",
            ],
        ),
        expect![[r#"
            Warning: # E1002

            Unused variable.

            This variable is unused by any other part of your code, nor marked with `pub`
            visibility.

            Note that this warning might uncover other bugs in your code. For example, if
            there are two variables in your codebase that has similar name, you might just
            use the other variable by mistake.

            Specifically, if the variable is at the toplevel, and the body of the module
            contains side effects, the side effects will not happen.

            ## Erroneous example

            ```moonbit
            let p : Int = {
            //  ^ Warning: Unused toplevel variable 'p'.
            //             Note if the body contains side effect, it will not happen.
            //             Use `fn init { .. }` to wrap the effect.
              println("Side effect")
              42
            }

            fn main {
              let x = 42 // Warning: Unused variable 'x'
            }
            ```

            ## Suggestion

            There are multiple ways to fix this warning:

            - If the variable is indeed useless, you can remove the definition of the
              variable.
            - If this variable is at the toplevel (i.e., not local), and is part of the
              public API of your module, you can add the `pub` keyword to the variable.

              ```moonbit
              pub let p = 42
              ```

            - If you made a typo in the variable name, you can rename the variable to the
              correct name at the use site.
            - If your code depends on the side-effect of the variable, you can wrap the
              side-effect in a `fn init` block.

              ```moonbit
              fn init {
                println("Side effect")
              }
              ```

            There are some cases where you might want to keep the variable private and
            unused at the same time. In this case, you can call `ignore()` on the variable
            to force the use of it.

            ```moonbit
            let p : Int = {
              println("Side effect")
              42
            }

            fn init {
              ignore(p)
            }

            fn main {
              let x = 42
              ignore(x)
            }
            ```

               ╭─[hello_2_test.mbt:2:6]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning: Unused variable 'unused_in_patch_test_json'
            ───╯
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_add_mi_if_self_not_set_in_test_imports() {
    let dir = TestDir::new("self-pkg-in-test-import.in");

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./target/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib:./lib -target wasm-gc
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc
            moonc check ./lib3/hello.mbt -o ./target/wasm-gc/release/check/lib3/lib3.mi -pkg username/hello/lib3 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib3:./lib3 -target wasm-gc
            moonc check ./lib3/hello_test.mbt -o ./target/wasm-gc/release/check/lib3/lib3.blackbox_test.mi -pkg username/hello/lib3_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib3/lib3.mi:lib3 -pkg-sources username/hello/lib3_blackbox_test:./lib3 -target wasm-gc -blackbox-test
            moonc check ./lib2/hello.mbt -o ./target/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/lib2:./lib2 -target wasm-gc
            moonc check ./lib2/hello_test.mbt -o ./target/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib2/lib2.mi:lib2 -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test
            moonc check ./lib/hello_test.mbt -o ./target/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/lib/lib.mi:lll -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test
        "#]],
    );

    check(get_stdout(&dir, ["check"]), expect![""]);
    get_stdout(&dir, ["clean"]);
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 7 tasks, now up to date
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--no-parallelize", "--sort-input"]),
        expect![[r#"
            Hello, world! lib
            Hello, world! lib2
            Hello, world! lib3
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_run_doc_test() {
    let dir = TestDir::new("run_doc_test.in");

    // `moon test --doc` run doc test only
    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--doc"]),
        expect![[r#"
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt
            test username/hello/lib/hello.mbt::doc_test hello.mbt 9 4 failed
            expect test failed at $ROOT/src/lib/hello.mbt:12:5-12:19
            Diff:
            ----
            1256
            ----

            test username/hello/lib/hello.mbt::doc_test hello.mbt 19 4 failed: FAILED: $ROOT/src/lib/hello.mbt:22:5-22:31 this is a failure
            test username/hello/lib/greet.mbt::2 failed
            expect test failed at $ROOT/src/lib/greet.mbt:22:7-22:21
            Diff:
            ----
            1256
            ----

            test username/hello/lib/greet.mbt::3 failed: FAILED: $ROOT/src/lib/greet.mbt:31:7-31:31 another failure
            test username/hello/lib/greet.mbt::doc_test greet.mbt 92 38 failed
            expect test failed at $ROOT/src/lib/greet.mbt:96:5-96:41
            Diff:
            ----
            b"/x54/x00/x65/x00/x73/x00/x74/x00"
            ----

            Total tests: 13, passed: 8, failed: 5.
        "#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--sort-input", "--doc", "--update"]),
        expect![[r#"
            doc_test 1 from hello.mbt
            doc_test 2 from hello.mbt
            doc_test 3 from hello.mbt
            doc_test
            doc_test 1 from greet.mbt
            test block 1
            test block 2
            test block 3
            doc_test 3 from greet.mbt
            test block 4
            test block 5
            doc_test 5 from greet.mbt

            Auto updating expect tests and retesting ...

            doc_test 2 from hello.mbt
            doc_test 2 from hello.mbt
            test username/hello/lib/hello.mbt::doc_test hello.mbt 19 4 failed: FAILED: $ROOT/src/lib/hello.mbt:22:5-22:31 this is a failure
            test block 2
            test block 2
            test username/hello/lib/greet.mbt::3 failed: FAILED: $ROOT/src/lib/greet.mbt:31:7-31:31 another failure
            Total tests: 13, passed: 11, failed: 2.
        "#]],
    );

    // `moon test` will not run doc test
    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[cfg(unix)]
#[test]
#[ignore = "moon query may no support anymore"]
fn test_moon_query() {
    let dir = TestDir::new("test_filter_pkg_with_test_imports.in");

    check(
        get_stdout(&dir, ["build", "--show-artifacts"]),
        // need topological order
        expect![[r#"
            [["username/hello/lib6","$ROOT/target/wasm-gc/release/build/lib6/lib6.mi","$ROOT/target/wasm-gc/release/build/lib6/lib6.core"],["username/hello/lib7","$ROOT/target/wasm-gc/release/build/lib7/lib7.mi","$ROOT/target/wasm-gc/release/build/lib7/lib7.core"],["username/hello/lib3","$ROOT/target/wasm-gc/release/build/lib3/lib3.mi","$ROOT/target/wasm-gc/release/build/lib3/lib3.core"],["username/hello/lib1","$ROOT/target/wasm-gc/release/build/lib1/lib1.mi","$ROOT/target/wasm-gc/release/build/lib1/lib1.core"],["username/hello/lib5","$ROOT/target/wasm-gc/release/build/lib5/lib5.mi","$ROOT/target/wasm-gc/release/build/lib5/lib5.core"],["username/hello/lib4","$ROOT/target/wasm-gc/release/build/lib4/lib4.mi","$ROOT/target/wasm-gc/release/build/lib4/lib4.core"],["username/hello/lib2","$ROOT/target/wasm-gc/release/build/lib2/lib2.mi","$ROOT/target/wasm-gc/release/build/lib2/lib2.core"],["username/hello/lib","$ROOT/target/wasm-gc/release/build/lib/lib.mi","$ROOT/target/wasm-gc/release/build/lib/lib.core"],["username/hello/main","$ROOT/target/wasm-gc/release/build/main/main.mi","$ROOT/target/wasm-gc/release/build/main/main.core"]]
        "#]],
    );

    get_stdout(&dir, ["query", "moonbitlang/x"]);
}

#[test]
#[allow(clippy::just_underscores_and_digits)]
fn test_moon_install_bin() {
    let top_dir = TestDir::new("moon_install_bin.in");
    let dir = top_dir.join("user.in");

    let mut _1 = PathBuf::from("");
    let mut _2 = PathBuf::from("");
    let mut _3 = PathBuf::from("");
    let mut _4 = PathBuf::from("");
    let mut _5 = PathBuf::from("");

    #[cfg(unix)]
    {
        _1 = top_dir.join("author2.in").join("author2-native");
        _2 = top_dir.join("author2.in").join("author2-js");
        _3 = top_dir.join("author2.in").join("author2-wasm");
        _4 = top_dir.join("author1.in").join("this-is-wasm");
        _5 = top_dir.join("author1.in").join("main-js");
    }

    #[cfg(target_os = "windows")]
    {
        _1 = top_dir.join("author2.in").join("author2-native.ps1");
        _2 = top_dir.join("author2.in").join("author2-js.ps1");
        _3 = top_dir.join("author2.in").join("author2-wasm.ps1");
        _4 = top_dir.join("author1.in").join("this-is-wasm.ps1");
        _5 = top_dir.join("author1.in").join("main-js.ps1");
    }

    // moon check should auto install bin deps
    get_stdout(&dir, ["check"]);
    assert!(_1.exists());
    assert!(_2.exists());
    assert!(_3.exists());
    assert!(_4.exists());
    assert!(_5.exists());

    {
        // delete all bin files
        std::fs::remove_file(&_1).unwrap();
        std::fs::remove_file(&_2).unwrap();
        std::fs::remove_file(&_3).unwrap();
        std::fs::remove_file(&_4).unwrap();
        std::fs::remove_file(&_5).unwrap();
        assert!(!_1.exists());
        assert!(!_2.exists());
        assert!(!_3.exists());
        assert!(!_4.exists());
        assert!(!_5.exists());
    }

    // moon install should install bin deps
    get_stdout(&dir, ["install"]);

    assert!(_1.exists());
    assert!(_2.exists());
    assert!(_3.exists());
    assert!(_4.exists());
    assert!(_5.exists());

    let content = get_stderr(&dir, ["build", "--sort-input"]);
    // ignore some cl warnings
    let mut lines = content.lines().rev().take(5).collect::<Vec<_>>();
    lines.reverse();
    check(
        lines.join("\n"),
        expect![[r#"
            main-js
            lib Hello, world!
            ()
            Executed 1 pre-build task, now up to date
            Finished. moon: ran 3 tasks, now up to date"#]],
    );
}

#[test]
fn test_strip_debug() {
    let dir = TestDir::new("strip_debug.in");

    check(
        get_stdout(&dir, ["build", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/build/lib/lib.core ./target/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./target/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm-gc --driver-kind internal --release --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--no-strip", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm-gc --driver-kind internal --release --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moon_new/lib --target wasm-gc --driver-kind internal --mode test
            moonc build-package ./lib/hello.mbt ./target/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./target/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_tracing_value() {
    let dir = TestDir::new("tracing_value.in");

    // main.mbt in package
    check(
        get_stdout(
            &dir,
            [
                "run",
                "./main/main.mbt",
                "--enable-value-tracing",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./target/wasm-gc/release/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -enable-value-tracing
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/lib/lib.core ./target/wasm-gc/release/build/main/main.core -main moon_new/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
            moonrun ./target/wasm-gc/release/build/main/main.wasm
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./main/main.mbt", "--enable-value-tracing"]),
        expect![[r#"
            Hello, world!
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"a","value":"1","line":"3","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"b","value":"2","line":"4","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"c","value":"3","line":"5","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            3
        "#]],
    );

    // single file
    check(
        get_stdout(
            &dir,
            ["run", "./main.mbt", "--enable-value-tracing", "--dry-run"],
        ),
        expect![[r#"
            moonc build-package $ROOT/main.mbt -o $ROOT/target/main.core -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target wasm-gc -enable-value-tracing
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/main.core -o $ROOT/target/main.wasm -pkg-sources moon/run/single:$ROOT -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target wasm-gc
            moonrun $ROOT/target/main.wasm
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./main.mbt", "--enable-value-tracing"]),
        expect![[r#######"
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"a","value":"1","line":"2","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"b","value":"2","line":"3","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            ######MOONBIT_VALUE_TRACING_START######
            {"name":"c","value":"3","line":"4","start_column":"7","end_column":"8"}
            ######MOONBIT_VALUE_TRACING_END######
            3
        "#######]],
    );
}

#[test]
fn test_diff_mbti() {
    let dir = TestDir::new("diff_mbti.in");
    let content = get_err_stdout(&dir, ["info", "--target", "all"]);
    assert!(content.contains("$ROOT/target/wasm-gc/release/check/lib/lib.mbti"));
    assert!(content.contains("$ROOT/target/js/release/check/lib/lib.mbti"));
    assert!(content.contains("-fn aaa() -> String"));
    assert!(content.contains("+fn a() -> String"));
}

#[test]
fn moon_info_specific_package() {
    let dir = TestDir::new("moon_new.in");

    // exact match
    get_stdout(&dir, ["info", "--package", "moon_new/main"]);
    assert!(dir.join("main/main.mbti").exists());
    assert!(!dir.join("lib/lib.mbti").exists());

    // fuzzy match
    get_stdout(&dir, ["info", "--package", "lib"]);
    assert!(dir.join("lib/lib.mbti").exists());

    let content = get_err_stderr(&dir, ["info", "--package", "moon_new/does_not_exist"]);
    assert!(content.contains("package `moon_new/does_not_exist` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)"));
}

#[test]
fn test_exports_in_native_backend() {
    let dir = TestDir::new("native_exports.in");
    let _ = get_stdout(&dir, ["build", "--target", "native"]);
    assert!(!dir
        .join("target")
        .join("native")
        .join("release")
        .join("build")
        .join("lib")
        .join("lib.c")
        .exists());
    let lib2_c = read(
        dir.join("target")
            .join("native")
            .join("release")
            .join("build")
            .join("lib2")
            .join("lib2.c"),
    );
    assert!(lib2_c.contains("$username$hello$lib2$hello()"));

    // alias not works
    let lib3_c = read(
        dir.join("target")
            .join("native")
            .join("release")
            .join("build")
            .join("lib3")
            .join("lib3.c"),
    );
    assert!(lib3_c.contains("$username$hello$lib3$hello()"));
}

#[test]
fn test_diag_loc_map() {
    let dir = TestDir::new("diag_loc_map.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            Error: [4014]
                ╭─[$ROOT/parser.mbty:25:37]
                │
             25 │   : lhs=add "+" rhs=factor  { lhs + "x" + rhs }
                │                                     ─┬─  
                │                                      ╰─── Expr Type Mismatch
                    has type : String
                    wanted   : Int
            ────╯
            error: failed when checking
        "#]],
    );
}

#[test]
fn test_dont_link_third_party() {
    let dir = TestDir::new("dont_link_third_party.in");

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./target/wasm-gc/release/build/main/main.core -pkg hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources hello/main:./main -target wasm-gc
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core ./target/wasm-gc/release/build/main/main.core -main hello/main -o ./target/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc
        "#]],
    );
}

#[test]
fn test_supported_backends_in_pkg_json() {
    let dir = TestDir::new("supported_backends_in_pkg_json");
    let pkg1 = dir.join("pkg1.in");
    // let pkg2 = dir.join("pkg2.in");
    let pkg3 = dir.join("pkg3.in");

    check(
        get_err_stderr(&pkg1, ["build"]),
        expect![[r#"
            error: cannot find a common supported backend for the deps chain: "username/hello1/main: [js, wasm-gc] -> username/hello1/lib: [native]"
        "#]],
    );

    // check(
    //     get_err_stderr(&pkg2, ["build"]),
    //     expect![[r#"
    //         error: deps chain: "username/hello2/main: [js, wasm-gc] -> username/hello2/lib: [js]" supports backends `[js]`, while the current target backend is wasm-gc
    //     "#]],
    // );

    check(
        get_err_stderr(&pkg3, ["check"]),
        expect![[r#"
            error: cannot find a common supported backend for the deps chain: "username/hello/main: [wasm-gc] -> username/hello/lib: [js, llvm, native, wasm, wasm-gc] -> username/hello/lib1: [wasm-gc] -> username/hello/lib3: [wasm-gc] -> username/hello/lib7: [js, wasm]"
        "#]],
    );
}

#[test]
fn test_update_expect_failed() {
    let dir = TestDir::new("test_expect_with_escape.in");
    let _ = get_stdout(&dir, ["test", "-u"]);
    check(
        read(dir.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            test {
              inspect!("\x0b", content="\x0b")
              inspect!("a\x0b", content="a\x0b")
              inspect!("a\x00b", content="a\x00b")
              inspect!("a\x00b\x19", content="a\x00b\x19")
              inspect!("\na\n\x00\nb\n\x19", content=
                "\x0aa\x0a\x00\x0ab\x0a\x19")
              inspect!("\n\"a\n\x00\nb\"\n\x19", content=
                "\x0a\"a\x0a\x00\x0ab\"\x0a\x19")
            }

            ///|
            test {
              inspect!("\"abc\"", content=#|"abc"
              )
              inspect!("\"a\nb\nc\"", content=
                #|"a
                #|b
                #|c"
              )
              inspect!("\x0b\"a\nb\nc\"", content=
                "\x0b\"a\x0ab\x0ac\"")
            }
        "#]],
    );
}

#[test]
fn test_native_stub_in_pkg_json() {
    let dir = TestDir::new("native_stub.in");

    let native_1 = dir.join("native_1.in");
    let native_2 = dir.join("native_2.in");
    let native_3 = dir.join("native_3.in");

    check(
        get_stdout(&native_1, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_1, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_2, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_2, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_2/libb/stub.c!!!
        "#]],
    );

    check(
        get_stdout(&native_3, ["test", "--target", "native", "--sort-input"]),
        expect![[r#"
            Hello world from native_1/lib/stub.c!!!
            Hello world from native_2/libb/stub.c!!!
            Hello world from native_3/libbb/stub.c!!!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&native_3, ["run", "main", "--target", "native"]),
        expect![[r#"
            Hello world from native_3/libbb/stub.c!!!
        "#]],
    );
}

#[test]
fn test_run_md_test() {
    let dir = TestDir::new("run_md_test.in");

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [1002]
                ╭─[$ROOT/src/lib/1.mbt.md:31:9]
                │
             31 │     let a = 1
                │         ┬  
                │         ╰── Warning: Unused variable 'a'
            ────╯
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--md", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            Hello, world 2!
            test username/hello/lib/hello_test.mbt::inspect in bbtest failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:12:3-12:32
            Diff:
            ----
            inspect in bbtest
            ----

            test username/hello/lib/1.mbt.md::2 failed
            expect test failed at $ROOT/src/lib/1.mbt.md:33:5-33:21
            Diff:
            ----
            4234
            ----

            test username/hello/lib/1.mbt.md::3 failed
            expect test failed at $ROOT/src/lib/1.mbt.md:50:5-50:16
            Diff:
            ----
             all
             wishes

             come
             true

            ----

            Total tests: 7, passed: 4, failed: 3.
        "#]],
    );

    // test filter in md test
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--md",
                "--sort-input",
                "-p",
                "lib",
                "-f",
                "1.mbt.md",
                "-i",
                "1",
            ],
        ),
        expect![[r#"
            Hello, world 3!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--md", "--update", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            Hello, world 2!

            Auto updating expect tests and retesting ...

            fn in md test
            fn in md test
            fn in md test
            Hello, world 2!
            Hello, world 2!
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    #[cfg(unix)]
    {
        get_stdout(&dir, ["check", "--sort-input"]);
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/lib",
                      "root": "username/hello",
                      "rel": "lib",
                      "files": {
                        "$ROOT/src/lib/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {
                        "$ROOT/src/lib/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "mbt-md-files": {
                        "$ROOT/src/lib/1.mbt.md": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/src/lib/2.mbt.md": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/src/lib/3.mbt.md": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/lib/lib.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/src/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        );
    }
}

#[test]
#[cfg(unix)]
fn test_pre_build_dirty() {
    let dir = TestDir::new("pre_build_dirty.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            fn init {}
            Executed 1 pre-build task, now up to date
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: no work to do
        "#]],
    );
    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: no work to do
        "#]],
    );
}

#[test]
fn native_backend_test_filter() {
    let dir = TestDir::new("native_backend_test_filter.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "3",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test C
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "2",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "1",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello_wbtest.mbt",
                "-i",
                "0",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test hello_0
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "4",
                "--sort-input",
            ],
        ),
        expect![[r#"
            test username/hello/lib/hello.mbt::D failed
            expect test failed at $ROOT/lib/hello.mbt:24:3
            Diff:
            ----
            test D

            ----

            Total tests: 1, passed: 0, failed: 1.
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "native",
                "-p",
                "lib",
                "-f",
                "hello.mbt",
                "-i",
                "4",
                "-u",
                "--sort-input",
            ],
        ),
        expect![[r#"

            Auto updating expect tests and retesting ...

            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_upgrade() -> anyhow::Result<()> {
    if std::env::var("CI").is_err() {
        return Ok(());
    }
    let tmp_dir = tempfile::TempDir::new()?;
    let _ = std::process::Command::new(moon_bin())
        .env("MOON_HOME", tmp_dir.path().to_str().unwrap())
        .arg("upgrade")
        .arg("--force")
        .arg("--non-interactive")
        .arg("--base-url")
        .arg("https://cli.moonbitlang.com")
        .output()?;
    #[cfg(unix)]
    let xs = [
        tmp_dir.path().join("bin").join("moon").exists(),
        tmp_dir.path().join("bin").join("moonc").exists(),
    ];
    #[cfg(windows)]
    let xs = [
        tmp_dir.path().join("bin").join("moon.exe").exists(),
        tmp_dir.path().join("bin").join("moonc.exe").exists(),
    ];
    check(format!("{:?}", xs), expect!["[true, true]"]);
    Ok(())
}

#[test]
fn test_no_warn_deps() {
    let dir = TestDir::new("no_warn_deps.in");
    let dir = dir.join("user.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "--deny-warn"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_postadd_script() {
    if std::env::var("CI").is_err() {
        return;
    }
    let dir = TestDir::new("test_postadd_script.in");
    let output = get_stdout(&dir, ["add", "lijunchen/test_postadd"]);
    assert!(output.contains(".mooncakes/lijunchen/test_postadd"));

    let _ = get_stdout(&dir, ["remove", "lijunchen/test_postadd"]);

    let out = std::process::Command::new(moon_bin())
        .current_dir(&dir)
        .env("MOON_IGNORE_POSTADD", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(["add", "lijunchen/test_postadd"])
        .output()
        .unwrap();
    let out = String::from_utf8(out.stderr).unwrap();
    assert!(!out.contains(".mooncakes/lijunchen/test_postadd"));
}

#[test]
fn test_ambiguous_pkg() {
    let dir = TestDir::new("ambiguous_pkg.in");
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            error: Ambiguous package name: my/name/is/ambiguous
            Candidates:
              ambiguous in my/name/is ($ROOT/deps/ambiguous/src/ambiguous)
              is/ambiguous in my/name ($ROOT/src/is/ambiguous)
        "#]],
    );
}
