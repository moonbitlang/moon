use crate::{TestDir, get_stdout};

#[test]
fn test_driver_allows_local_map_type() {
    let dir = TestDir::new("test_driver_map_collision");
    let stdout = get_stdout(&dir, ["test"]);
    assert!(stdout.contains("Total tests: 1, passed: 1, failed: 0."));
}
