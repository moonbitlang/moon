use crate::{TestDir, get_stdout, get_stdout_with_envs};

/// Ensure that the C stub linking order is stable and does not change between runs.
///
/// This have 3 native stubs, so there are 3! = 6 possible linking orders.
/// By running it 5 times, the chance of it being unstable but lucky enough is
/// (1/6)^4 ~= 0.08%, which is acceptable for a test.
#[test]
fn test_native_stub_linking_order_stability() {
    let dir = TestDir::new("native_stub_stability");

    let mut stdouts = vec![];
    for _ in 0..5 {
        let stdout = get_stdout(&dir, ["run", "main", "--target", "native", "--dry-run"]);
        stdouts.push(stdout);
    }

    let stdout1 = &stdouts[0];
    for stdout in &stdouts[1..] {
        assert_eq!(
            stdout1, stdout,
            "dry run result differs between runs: \n{}\n{}",
            stdout1, stdout
        );
    }
}

#[cfg(unix)]
#[test]
fn test_native_stub_archive_drops_removed_members_without_clean() {
    let dir = TestDir::new("native_stub_archive_membership");
    let cc = which::which("cc").expect("C compiler should be available");
    let ar = which::which("ar").expect("ar should be available");
    let envs = [("MOON_CC", cc.as_os_str()), ("MOON_AR", ar.as_os_str())];

    assert_eq!(
        get_stdout_with_envs(&dir, ["run", "main", "--target", "native"], envs),
        "1\n"
    );

    std::fs::write(
        dir.join("lib/moon.pkg.json"),
        r#"{
  "native-stub": [
    "entry.c",
    "new.c"
  ]
}
"#,
    )
    .unwrap();

    assert_eq!(
        get_stdout_with_envs(&dir, ["run", "main", "--target", "native"], envs),
        "2\n"
    );
}
