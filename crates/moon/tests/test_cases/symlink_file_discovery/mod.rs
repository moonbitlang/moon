#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn test_symlink_file_discovery() {
    let dir = TestDir::new("symlink_file_discovery/symlink_file_discovery.in");

    let target = dir.join("links/helper.mbt");
    let link = dir.join("main/helper.mbt");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    check(
        get_stdout(&dir, ["run", "main"]),
        expect![[r#"
    "#]],
    )
}
