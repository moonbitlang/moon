use super::*;

#[test]
fn test_bench() {
    let dir = TestDir::new("bench2.in");
    let out = get_stdout(&dir, ["bench"]);
    assert!(out.contains("[username/bench2] bench bench2_test.mbt:23 (#2) ok"));
    assert!(out.contains("[username/bench2] bench bench2.mbt:23 (#0) ok"));

    let out = get_stdout(
        &dir,
        ["bench", "-p", "bench2", "--file", "bench2.mbt", "-i", "0"],
    );
    assert!(!(out.contains("[username/bench2] bench bench2_test.mbt:23 (#2) ok")));
    assert!(out.contains("[username/bench2] bench bench2.mbt:23 (#0) ok"));
}
