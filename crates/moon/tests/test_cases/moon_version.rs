use super::*;

#[test]
fn test_moon_version_abbreviates_toolchain_paths() {
    let dir = TestDir::new_empty();
    let toolchain = dunce::canonicalize(toolchain_root_for_tests()).unwrap();
    // Use the installed toolchain as this child process's home so the expected
    // paths do not depend on the account or toolchain layout running the test.
    let command = || {
        moon_cmd(&dir)
            .env("HOME", &toolchain)
            .env("USERPROFILE", &toolchain)
            .env_remove("MOONC_OVERRIDE")
            .env_remove("MOONRUN_OVERRIDE")
    };
    let exe = std::env::consts::EXE_SUFFIX;
    command()
        .args(["version", "--all"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
moon [..]
moonc [..] ~/bin/moonc[EXE]
moonrun [..] ~/bin/moonrun[EXE]
...
"#]]);
    let output = command()
        .args(["version", "--all", "--json"])
        .assert()
        .success();
    let versions: moonutil::version::VersionItems =
        serde_json_lenient::from_slice(&output.get_output().stdout).unwrap();
    for name in ["moonc", "moonrun"] {
        let item = versions
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap();
        assert_eq!(
            item.path.as_deref(),
            Some(
                Path::new("~")
                    .join("bin")
                    .join(format!("{name}{exe}"))
                    .to_str()
                    .unwrap()
            ),
        );
    }
}

#[test]
fn test_moon_version() {
    let dir = TestDir::new_empty();
    let output = get_stdout(&dir, ["version"]);
    let expected_moon_version = format!("moon {}", get_cargo_pkg_version());

    assert!(output.contains(&expected_moon_version));

    let output = get_stdout(&dir, ["version", "--all"]);
    assert!(output.contains(&expected_moon_version));
    assert!(output.contains("moonc"));
    assert!(output.contains("moonrun"));

    let output = get_stdout(&dir, ["version", "--all", "--no-path"]);
    assert!(output.contains(&expected_moon_version));
    assert!(output.contains("moonc"));
    assert!(output.contains("moonrun"));
}

#[test]
fn test_moon_version_json() -> anyhow::Result<()> {
    let dir = TestDir::new_empty();

    let output = get_stdout(&dir, ["version", "--json"]);
    let items: moonutil::version::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert!(items.items[0].path.is_some());

    let output = get_stdout(&dir, ["version", "--all", "--json"]);
    let items: moonutil::version::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 3);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert_eq!(items.items[1].name, "moonc");

    let output = get_stdout(&dir, ["version", "--all", "--json", "--no-path"]);
    let items: moonutil::version::VersionItems = serde_json_lenient::from_str(&output)?;
    assert!(items.items[0].path.is_none());

    Ok(())
}
