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

use super::*;
use expect_test::expect;

#[test]
fn test_design() {
    let dir = TestDir::new("design");
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build"])
        .assert()
        .success();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
    check(
        get_stdout(&dir, ["run", "main1"]),
        expect![[r#"
            new_list
            new_queue
            new_list
            new_stack
            new_vector
            main1
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main2"]),
        expect![[r#"
            new_list
            new_queue
            main2
        "#]],
    );

    get_stdout(&dir, ["clean"]);
    check(
        get_stdout(&dir, ["run", "main2", "--target", "js", "--build-only"]),
        expect![[r#"
            {"artifacts_path":["$ROOT/target/js/debug/build/main2/main2.js"]}
        "#]],
    );
    assert!(dir.join("target/js/debug/build/main2/main2.js").exists());
}
