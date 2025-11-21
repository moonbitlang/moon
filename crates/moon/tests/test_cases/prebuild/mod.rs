use super::*;

#[test]
fn test_moonlex() {
    if std::env::var("CI").is_err() {
        return;
    }
    if !moonutil::moon_dir::MOON_DIRS
        .moon_bin_path
        .join("moonlex.wasm")
        .exists()
    {
        panic!("`moonlex` should comes with installation of moonbit toolchain")
    }
    let dir = TestDir::new("prebuild/moonlex");
    let _ = get_stdout(&dir, ["check"]);
    assert!(dir.join("src/main/fortytwolexer.mbt").exists());
}
