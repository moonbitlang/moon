use super::*;

#[test]
fn test_bench_driver_build() {
    let dir = TestDir::new("moon_bench");
    check(get_stderr(&dir, ["bench", "--build-only"]), expect![""]);
}
