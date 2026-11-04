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

/// Test that moon test correctly handles virtual packages with transitive dependencies.
///
/// This test case verifies the fix for issue #1124 where the DFS used by moon test
/// didn't properly traverse into virtual package implementations, causing it to miss
/// transitive dependencies.
///
/// Structure:
/// - main: depends on middle and virtual, overrides virtual with impl
/// - middle: depends on virtual
/// - virtual: virtual package interface
/// - impl: implements virtual, depends on dep (transitive dependency)
/// - dep: the transitive dependency that was being missed before the fix
#[test]
fn test_virtual_with_transitive_dep() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    // Test that all tests pass, including those that use the transitive dependency
    moon_cmd(&dir)
        .args(["test", "--target", "wasm"])
        .assert()
        .success();

    // Also test with wasm-gc target
    moon_cmd(&dir)
        .args(["test", "--target", "wasm-gc"])
        .assert()
        .success();
}

/// Test that moon test works with virtual packages in internal tests
#[test]
fn test_virtual_internal_test() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    // Run tests on the middle package which has an internal test
    moon_cmd(&dir)
        .args(["test", "src/middle", "--target", "wasm"])
        .assert()
        .success();
}

/// Test that moon test works with virtual packages in blackbox tests
#[test]
fn test_virtual_blackbox_test() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    // Run tests on main which should include blackbox tests
    moon_cmd(&dir)
        .args(["test", "src/main", "--target", "wasm"])
        .assert()
        .success();
}

/// Ensure that all commands work well with virtual pkgs
#[test]
fn test_virtual_commands() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    moon_cmd(&dir).args(["build"]).assert().success();

    moon_cmd(&dir).args(["check"]).assert().success();

    moon_cmd(&dir).args(["info"]).assert().success();

    moon_cmd(&dir).args(["test"]).assert().success();
}
