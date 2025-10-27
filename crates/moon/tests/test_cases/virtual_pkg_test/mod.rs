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
    snapbox::cmd::Command::new(moon_bin())
        .args(["test", "--target", "wasm"])
        .current_dir(&dir)
        .assert()
        .success();

    // Also test with wasm-gc target
    snapbox::cmd::Command::new(moon_bin())
        .args(["test", "--target", "wasm-gc"])
        .current_dir(&dir)
        .assert()
        .success();
}

/// Test that moon test works with virtual packages in internal tests
#[test]
fn test_virtual_internal_test() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    // Run tests on the middle package which has an internal test
    snapbox::cmd::Command::new(moon_bin())
        .args(["test", "src/middle", "--target", "wasm"])
        .current_dir(&dir)
        .assert()
        .success();
}

/// Test that moon test works with virtual packages in blackbox tests
#[test]
fn test_virtual_blackbox_test() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    // Run tests on main which should include blackbox tests
    snapbox::cmd::Command::new(moon_bin())
        .args(["test", "src/main", "--target", "wasm"])
        .current_dir(&dir)
        .assert()
        .success();
}

/// Ensure that all commands work well with virtual pkgs
#[test]
fn test_virtual_commands() {
    let dir = TestDir::new("virtual_pkg_test/virtual_with_transitive_dep");

    snapbox::cmd::Command::new(moon_bin())
        .args(["build"])
        .current_dir(&dir)
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .args(["check"])
        .current_dir(&dir)
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .args(["info"])
        .current_dir(&dir)
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .args(["test"])
        .current_dir(&dir)
        .assert()
        .success();
}
