use super::*;

#[test]
fn test_clean() {
    let dir = TestDir::new("clean/clean.in");
    let _ = get_stdout(&dir, ["build"]);

    assert!(dir.join("_build").exists());

    let _ = get_stdout(&dir, ["clean"]);

    assert!(!(dir.join("_build").exists()));
}

#[test]
fn test_clean_workspace() {
    let dir = TestDir::new("workspace_basic.in");
    let _ = get_stdout(&dir, ["build"]);

    assert!(dir.join("_build").exists());

    let _ = get_stdout(&dir, ["clean"]);

    assert!(!dir.join("_build").exists());
}

#[test]
fn test_clean_disabled_global_caches_without_project() {
    let dir = TestDir::new_empty();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", "off")
        .args(["clean", "--dep-cache"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    moon_cmd(&dir)
        .env("MOON_BUILD_CACHE", "off")
        .args(["clean", "--build-cache"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_clean_removes_owned_custom_cache_roots() {
    let dir = TestDir::new_empty();

    for (environment, flag, ownership) in [
        ("MOON_DEP_CACHE", "--dep-cache", "dependency-sources\n"),
        ("MOON_BUILD_CACHE", "--build-cache", "build-artifacts\n"),
    ] {
        let parent = tempfile::TempDir::new().unwrap();
        let cache = parent.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        std::fs::write(cache.join(".moon-cache"), ownership).unwrap();
        std::fs::write(
            cache.join("opaque-cache-data"),
            "contents are not CLI layout",
        )
        .unwrap();

        moon_cmd(&dir)
            .env(environment, &cache)
            .args(["clean", flag])
            .assert()
            .success()
            .stdout_eq("")
            .stderr_eq("");

        assert!(!cache.exists());
    }
}

#[test]
fn test_clean_combines_global_caches_without_cleaning_local_build() {
    let dir = TestDir::new_empty();
    let local_build = dir.join("_build");
    std::fs::create_dir(&local_build).unwrap();
    std::fs::write(local_build.join("must-survive"), "local build").unwrap();

    let dependency_cache = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dependency_cache.path().join(".moon-cache"),
        "dependency-sources\n",
    )
    .unwrap();
    let build_cache = tempfile::TempDir::new().unwrap();
    std::fs::write(build_cache.path().join(".moon-cache"), "build-artifacts\n").unwrap();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", dependency_cache.path())
        .env("MOON_BUILD_CACHE", build_cache.path())
        .args(["clean", "--dep-cache", "--build-cache"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    assert!(!dependency_cache.path().exists());
    assert!(!build_cache.path().exists());
    assert!(local_build.join("must-survive").exists());
}

#[test]
fn test_clean_rejects_relative_cache_roots() {
    let dir = TestDir::new_empty();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", "relative")
        .args(["clean", "--dep-cache"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq("Error: MOON_DEP_CACHE must be an absolute path or `off`\n");
}

#[test]
fn test_clean_refuses_unowned_custom_cache_root() {
    let dir = TestDir::new_empty();
    let cache = tempfile::TempDir::new().unwrap();
    std::fs::write(cache.path().join("user-data"), "must survive").unwrap();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", cache.path())
        .args(["clean", "--dep-cache"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq("Error: refusing to clean unrecognized Moon cache root `[..]`\n");

    assert_eq!(
        std::fs::read_to_string(cache.path().join("user-data")).unwrap(),
        "must survive"
    );
}

#[test]
fn test_clean_uses_default_cache_roots() {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().unwrap();

    for (environment, flag, relative, ownership) in [
        (
            "MOON_DEP_CACHE",
            "--dep-cache",
            "cache/deps",
            "dependency-sources\n",
        ),
        (
            "MOON_BUILD_CACHE",
            "--build-cache",
            "cache/build",
            "build-artifacts\n",
        ),
    ] {
        let cache = moon_home.path().join(relative);
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join(".moon-cache"), ownership).unwrap();
        std::fs::write(cache.join("opaque-cache-data"), "contents").unwrap();

        moon_cmd(&dir)
            .env("MOON_HOME", moon_home.path())
            .env_remove(environment)
            .args(["clean", flag])
            .assert()
            .success()
            .stdout_eq("")
            .stderr_eq("");

        assert!(!cache.exists());
    }
}

#[test]
#[cfg(unix)]
fn test_clean_refuses_symlinked_cache_root() {
    let dir = TestDir::new_empty();
    let parent = tempfile::TempDir::new().unwrap();
    let target = parent.path().join("owned-cache");
    let cache = parent.path().join("cache-link");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join(".moon-cache"), "dependency-sources\n").unwrap();
    std::fs::write(target.join("opaque-cache-data"), "must survive").unwrap();
    std::os::unix::fs::symlink(&target, &cache).unwrap();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", &cache)
        .args(["clean", "--dep-cache"])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq("Error: refusing to clean symlinked Moon cache root `[..]`\n");

    assert!(cache.exists());
    assert_eq!(
        std::fs::read_to_string(target.join("opaque-cache-data")).unwrap(),
        "must survive"
    );
}

#[test]
#[cfg(unix)]
fn test_clean_removes_descendant_symlink_without_following_it() {
    let dir = TestDir::new_empty();
    let cache = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let outside_file = outside.path().join("must-survive");
    std::fs::write(cache.path().join(".moon-cache"), "dependency-sources\n").unwrap();
    std::fs::write(&outside_file, "outside cache").unwrap();
    std::os::unix::fs::symlink(outside.path(), cache.path().join("outside-link")).unwrap();

    moon_cmd(&dir)
        .env("MOON_DEP_CACHE", cache.path())
        .args(["clean", "--dep-cache"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    assert!(!cache.path().exists());
    assert_eq!(
        std::fs::read_to_string(outside_file).unwrap(),
        "outside cache"
    );
}
