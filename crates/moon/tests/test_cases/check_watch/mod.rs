use std::{
    io::{BufRead, BufReader},
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use moonutil::{common::BUILD_DIR, constants::WATCH_MODE_DIR};

use super::*;

struct WatchProcess {
    child: Child,
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        if self.child.try_wait().expect("poll watch process").is_some() {
            return;
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn check_watch_recovers_when_nested_watch_target_is_deleted() {
    let dir = TestDir::new("hello");
    let watch_target = dir.as_ref().join(BUILD_DIR).join(WATCH_MODE_DIR);
    let main_file = dir.as_ref().join("main/main.mbt");

    let mut child = moon_process_cmd(&dir)
        .args(["check", "--watch"])
        .env("RUST_LOG", "moon::watch=debug")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn moon check --watch");

    let stdout = child.stdout.take().expect("watch process stdout");
    let stderr = child.stderr.take().expect("watch process stderr");
    let (tx, rx) = mpsc::channel();
    let stderr_tx = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if stderr_tx.send(format!("stderr: {line}")).is_err() {
                break;
            }
        }
    });

    let _watch_process = WatchProcess { child };

    wait_for_watch_success(&rx, "initial check");
    wait_for_watch_message(&rx, "watcher startup", |line| {
        line.contains("Watcher loop started")
    });
    assert!(watch_target.exists());

    std::fs::remove_dir_all(&watch_target).expect("delete nested watch target");
    std::fs::write(
        &main_file,
        r#"fn main {
  println("Hello, watch!")
}

test {

}
"#,
    )
    .expect("update source file");

    wait_for_watch_success(&rx, "rerun after deleting nested watch target");
    assert!(watch_target.exists());
}

fn wait_for_watch_success(rx: &mpsc::Receiver<String>, phase: &str) {
    wait_for_watch_message(rx, phase, |line| {
        line.contains("Success, waiting for filesystem changes...")
    });
}

fn wait_for_watch_message(
    rx: &mpsc::Receiver<String>,
    phase: &str,
    matches: impl Fn(&str) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut output = Vec::new();

    loop {
        let now = Instant::now();
        assert!(
            now < deadline,
            "timed out waiting for {phase}; output:\n{}",
            output.join("\n")
        );

        match rx.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(line) => {
                let matched = matches(&line);
                output.push(line);
                if matched {
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!(
                    "timed out waiting for {phase}; output:\n{}",
                    output.join("\n")
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!(
                    "watch process exited before {phase}; output:\n{}",
                    output.join("\n")
                );
            }
        }
    }
}
