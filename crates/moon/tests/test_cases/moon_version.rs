use super::*;

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
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert!(items.items[0].path.is_some());

    let output = get_stdout(&dir, ["version", "--all", "--json"]);
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert_eq!(items.items.len(), 3);
    assert_eq!(items.items[0].name, "moon");
    assert!(items.items[0].version.contains(&get_cargo_pkg_version()));
    assert_eq!(items.items[1].name, "moonc");

    let output = get_stdout(&dir, ["version", "--all", "--json", "--no-path"]);
    let items: moonutil::common::VersionItems = serde_json_lenient::from_str(&output)?;
    assert!(items.items[0].path.is_none());

    Ok(())
}
