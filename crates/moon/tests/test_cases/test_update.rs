use crate::test_cases::*;

#[test]
fn update_limit_stops_further_reruns() {
    let dir = TestDir::new("test_update/update_limit");

    moon_cmd(&dir)
        .args(["test", "--update", "--limit", "2", "--no-parallelize"])
        .assert()
        .code(2)
        .stderr_eq("Warning: reached the limit of 2 update passes, stopping further updates.\n");

    assert_eq!(
        read(dir.join("inspect.mbt")),
        "test {\n  inspect(1, content=(#|1\n  ))\n  inspect(2, content=(#|2\n  ))\n  inspect(3)\n}\n"
    );
}
