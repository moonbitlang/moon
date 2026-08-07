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

use std::path::PathBuf;

#[test]
fn virtual_contract_uses_lifecycle_interface_dependencies() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let dir = moon_test_util::test_dir::TestDir::from_case_root(
        case_root,
        "virtual_pkg_test/virtual_artifact_lifecycle.in",
        true,
    );

    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["build", "src/virtual", "--target", "wasm-gc", "--dry-run"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(&dir)
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonc build-package ./src/dep/dep.mbt -o ./_build/wasm-gc/debug/build/dep/dep.core -pkg virtual_artifact_lifecycle/dep -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources virtual_artifact_lifecycle/dep:./src/dep -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
moonc build-interface ./src/virtual/pkg.mbti -o ./_build/wasm-gc/debug/build/virtual/virtual.mi -i ./_build/wasm-gc/debug/build/dep/dep.mi:dep -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg virtual_artifact_lifecycle/virtual -pkg-sources virtual_artifact_lifecycle/virtual:./src/virtual -virtual -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -error-format json

"#]]);

    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["check", "src/virtual", "--target", "wasm-gc", "--dry-run"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(&dir)
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moonc check ./src/dep/dep.mbt -o ./_build/wasm-gc/debug/check/dep/dep.mi -pkg virtual_artifact_lifecycle/dep -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources virtual_artifact_lifecycle/dep:./src/dep -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
moonc build-interface ./src/virtual/pkg.mbti -o ./_build/wasm-gc/debug/check/virtual/virtual.mi -i ./_build/wasm-gc/debug/check/dep/dep.mi:dep -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg virtual_artifact_lifecycle/virtual -pkg-sources virtual_artifact_lifecycle/virtual:./src/virtual -virtual -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -error-format json

"#]]);
}
