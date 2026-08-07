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

#[test]
fn test_moon_install_global_deprecated_warning() {
    // Test that running `moon install` without arguments shows deprecation warning
    let dir = TestDir::new("moon_install_global.in");

    // Running moon install without arguments should show deprecation warning
    let stderr = get_stderr(&dir, ["install"]);
    assert!(
        stderr.contains("deprecated"),
        "Expected deprecation warning in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_deprecated_uses_workspace_context() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_stderr(&dir, ["-C", "app", "install"]);
    assert!(
        stderr.contains("deprecated"),
        "Expected deprecation warning in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_local_path() {
    // Test installing from local path using --path
    let dir = TestDir::new("moon_install_global.in");

    // Create a temporary directory for installation
    let install_dir = dir.join("test_bin");
    std::fs::create_dir_all(&install_dir).unwrap();

    // Install using --path
    let _output = get_stdout(
        &dir,
        [
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    // Check that the binary was created
    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_uses_workspace_context() {
    let dir = TestDir::new("workspace_basic.in");
    let install_dir = dir.join("test_bin_install_workspace");
    std::fs::create_dir_all(&install_dir).unwrap();

    let _output = get_stdout(
        &dir,
        [
            "-C",
            "app",
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_renders_build_errors() {
    let dir = TestDir::new("moon_install_global_error.in");
    let install_dir = dir.join("test_bin");
    std::fs::create_dir_all(&install_dir).unwrap();

    let stderr = get_err_stderr(
        &dir,
        [
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    assert!(
        stderr.contains("Error: ["),
        "Expected rendered diagnostic in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("$ROOT/src/main/main.mbt"),
        "Expected source location in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("Expr Type Mismatch"),
        "Expected compile error message in stderr, got: {stderr}",
    );
    assert!(
        !stderr.contains("\"$message_type\":\"diagnostic\""),
        "Expected rendered diagnostics instead of raw JSON, got: {stderr}",
    );
}

#[test]
fn test_moon_install_global_defaults_to_moon_home_bin() {
    let dir = TestDir::new("moon_install_global.in");
    let moon_home = tempfile::tempdir().unwrap();

    let _output = get_stdout_with_envs(
        &dir,
        ["install", "--path", "src/main"],
        [("MOON_HOME", moon_home.path().to_string_lossy().into_owned())],
    );

    #[cfg(unix)]
    let binary_path = moon_home.path().join("bin").join("main");
    #[cfg(target_os = "windows")]
    let binary_path = moon_home.path().join("bin").join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_module_root_is_exact_path() {
    let dir = TestDir::new("moon_install_global.in");

    let stderr = get_err_stderr(&dir, ["install", "--path", "."]);
    assert!(
        stderr.contains("is not a main package"),
        "Expected exact local path behavior in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_local_source_wildcard_from_module_root() {
    let dir = TestDir::new("moon_install_global.in");
    let install_dir = dir.join("test_bin_wildcard_root");
    std::fs::create_dir_all(&install_dir).unwrap();

    let _output = get_stdout(
        &dir,
        ["install", "./...", "--bin", install_dir.to_str().unwrap()],
    );

    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_wildcard_with_path_flag_warns() {
    let dir = TestDir::new("moon_install_global.in");

    let stderr = get_err_stderr(&dir, ["install", "--path", "src/..."]);
    assert!(
        stderr.contains("does not support wildcard selectors like `src/...`"),
        "Expected wildcard warning in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("Use positional SOURCE for wildcard install: `moon install src/...`"),
        "Expected guidance for positional SOURCE in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_hosted_tree_url_rejects_extra_path() {
    let dir = TestDir::new_empty();

    let stderr = get_err_stderr(
        &dir,
        [
            "install",
            "https://github.com/owner/repo/tree/main/cmd/tool",
            "other/path",
        ],
    );
    assert!(
        stderr.contains(
            "PATH_IN_REPO must not be used when SOURCE already contains a /tree/... path"
        ),
        "Expected hosted tree URL ambiguity error in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_git_url_default_root_package() {
    // Test installing from git URL without PATH_IN_REPO.
    // Default behavior installs the module root package only.
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install root package only
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that only root package binary was created
    #[cfg(unix)]
    {
        assert!(install_path.join("install-test").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("tool1").exists());
        assert!(!install_path.join("tool2").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("install-test.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("tool1.exe").exists());
        assert!(!install_path.join("tool2.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_specific_package() {
    // Test installing specific package from git URL
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install only cmd/tool1
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "cmd/tool1",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that only tool1 was installed
    #[cfg(unix)]
    {
        assert!(install_path.join("tool1").exists());
        assert!(!install_path.join("tool2").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("install-test").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("tool1.exe").exists());
        assert!(!install_path.join("tool2.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("install-test.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_wildcard() {
    // Test installing with wildcard pattern from git URL
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install cmd/... (should install tool1 and tool2)
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "cmd/...",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that tool1 and tool2 were installed, but not others
    #[cfg(unix)]
    {
        assert!(install_path.join("tool1").exists());
        assert!(install_path.join("tool2").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("install-test").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("tool1.exe").exists());
        assert!(install_path.join("tool2.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("install-test.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_root_wildcard() {
    // Test installing all packages from git URL using /...
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "/...",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    #[cfg(unix)]
    {
        assert!(install_path.join("install-test").exists());
        assert!(install_path.join("hello").exists());
        assert!(install_path.join("tool1").exists());
        assert!(install_path.join("tool2").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("install-test.exe").exists());
        assert!(install_path.join("hello.exe").exists());
        assert!(install_path.join("tool1.exe").exists());
        assert!(install_path.join("tool2.exe").exists());
    }
}
