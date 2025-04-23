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
        // curl -L -o ~/.moon/bin/moonlex.wasm https://github.com/moonbitlang/moonlex/releases/download/ulex-v0.2.0/moonlex.wasm
        std::process::Command::new("curl")
            .arg("-L")
            .arg("-o")
            .arg(
                moonutil::moon_dir::MOON_DIRS
                    .moon_bin_path
                    .join("moonlex.wasm"),
            )
            .arg(
                "https://github.com/moonbitlang/moonlex/releases/download/ulex-v0.2.0/moonlex.wasm",
            )
            .output()
            .expect("Failed to download moonlex.wasm");
    }
    let dir = TestDir::new("prebuild/moonlex");
    let _ = get_stdout(&dir, ["check"]);
    assert!(dir.join("src/main/fortytwolexer.mbt").exists());
}
