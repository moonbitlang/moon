mod build;
mod bundle;
mod check;
mod run;
mod test;
mod tree;

use crate::{TestDir, moon_bin, moon_cmd, util::cache_registry_package};

fn json_command_with_postadd(args: &[&str], postadd: &str) -> snapbox::cmd::OutputAssert {
    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().expect("failed to create temporary MOON_HOME");
    let moon_path = std::env::join_paths(
        std::iter::once(
            moon_bin()
                .parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("test PATH should be valid");
    std::fs::create_dir_all(dir.join("src/lib")).unwrap();
    std::fs::write(
        dir.join("moon.mod.json"),
        r#"{
            "name": "test/root",
            "version": "0.1.0",
            "source": "src",
            "deps": { "testuser/postadd": "1.0.0" }
        }"#,
    )
    .unwrap();
    std::fs::write(dir.join("src/lib/moon.pkg.json"), "{}").unwrap();
    std::fs::write(dir.join("src/lib/lib.mbt"), "pub fn answer() -> Int { 42 }").unwrap();

    let dependency_manifest = serde_json::json!({
        "name": "testuser/postadd",
        "version": "1.0.0",
        "scripts": { "postadd": postadd }
    })
    .to_string()
    .into_bytes();
    cache_registry_package(
        moon_home.path(),
        "testuser/postadd",
        "1.0.0",
        &[("moon.mod.json", dependency_manifest)],
    );

    moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("PATH", moon_path)
        .args(args)
        .assert()
}
