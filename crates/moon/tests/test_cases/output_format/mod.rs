use super::*;

#[test]
fn test_output_format() {
    let dir = TestDir::new("output_format");

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "-q"])
        .assert()
        .success();
    assert!(dir
        .join(format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(!dir
        .join(format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["build", "-q", "--output-wat"])
        .assert()
        .success();
    assert!(dir
        .join(format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(!dir
        .join(format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "main"])
        .assert()
        .success();
    assert!(!dir
        .join(format!(
            "target/{}/release/build/main/main.wat",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());
    assert!(dir
        .join(format!(
            "target/{}/release/build/main/main.wasm",
            TargetBackend::default().to_backend_ext()
        ))
        .exists());

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "main", "--output-wat"])
        .assert()
        .failure();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["clean"])
        .assert()
        .success();
}
