// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
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
            {"artifacts_path":["$ROOT/_build/js/debug/build/main2/main2.js"]}
        "#]],
    );
    assert!(dir.join("_build/js/debug/build/main2/main2.js").exists());
}
