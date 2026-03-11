// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::util::moon_bin;

thread_local! {
    static SESSION: RefCell<Option<PersistentMoonSession>> = RefCell::new(None);
}

static RESPONSE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub(crate) struct PersistentMoonOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct TestServerRequest {
    cwd: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    response_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TestServerResponse {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

pub(crate) fn maybe_run_moon(
    dir: &Path,
    args: &[OsString],
    envs: &[(OsString, OsString)],
) -> Option<PersistentMoonOutput> {
    if !persistent_enabled() || !request_is_supported(args, envs) {
        return None;
    }

    Some(SESSION.with(|session| {
        let mut session = session.borrow_mut();
        send_with_retry(&mut session, dir, args, envs)
    }))
}

fn persistent_enabled() -> bool {
    std::env::var_os("MOON_TEST_PERSISTENT").is_some_and(|value| value != OsStr::new("0"))
}

fn request_is_supported(args: &[OsString], envs: &[(OsString, OsString)]) -> bool {
    !uses_tracing(args, envs)
}

fn uses_tracing(args: &[OsString], envs: &[(OsString, OsString)]) -> bool {
    args.iter().any(|arg| arg == OsStr::new("--trace"))
        || envs
            .iter()
            .any(|(name, _)| matches!(name.to_str(), Some("MOON_TRACE" | "RUST_LOG")))
}

fn send_with_retry(
    session: &mut Option<PersistentMoonSession>,
    dir: &Path,
    args: &[OsString],
    envs: &[(OsString, OsString)],
) -> PersistentMoonOutput {
    let first_attempt =
        ensure_session(session).and_then(|session| session.send_request(dir, args, envs));
    match first_attempt {
        Ok(output) => output,
        Err(first_err) => {
            *session = None;
            let second_attempt =
                ensure_session(session).and_then(|session| session.send_request(dir, args, envs));
            match second_attempt {
                Ok(output) => output,
                Err(second_err) => {
                    panic!(
                        "persistent moon test-server request failed after retry:\nfirst: {first_err}\nsecond: {second_err}"
                    );
                }
            }
        }
    }
}

fn ensure_session(
    session: &mut Option<PersistentMoonSession>,
) -> Result<&mut PersistentMoonSession, String> {
    if session.is_none() {
        *session = Some(PersistentMoonSession::spawn()?);
    }
    session
        .as_mut()
        .ok_or_else(|| "persistent moon test-server session disappeared".to_owned())
}

struct PersistentMoonSession {
    child: Child,
    stdin: ChildStdin,
}

impl PersistentMoonSession {
    fn spawn() -> Result<Self, String> {
        let mut child = Command::new(moon_bin())
            .args(["tool", "test-server"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("failed to spawn `moon tool test-server`: {err}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to take test-server stdin".to_owned())?;
        Ok(Self { child, stdin })
    }

    fn send_request(
        &mut self,
        dir: &Path,
        args: &[OsString],
        envs: &[(OsString, OsString)],
    ) -> Result<PersistentMoonOutput, String> {
        let response_path = next_response_path();
        let request = TestServerRequest {
            cwd: dir.to_path_buf(),
            args: args.iter().map(os_to_string).collect(),
            envs: envs
                .iter()
                .map(|(name, value)| (os_to_string(name), os_to_string(value)))
                .collect(),
            response_path: response_path.clone(),
        };

        let payload = serde_json::to_vec(&request)
            .map_err(|err| format!("failed to serialize test-server request: {err}"))?;
        self.stdin
            .write_all(&payload)
            .map_err(|err| format!("failed to write test-server request: {err}"))?;
        self.stdin
            .write_all(b"\n")
            .map_err(|err| format!("failed to terminate test-server request: {err}"))?;
        self.stdin
            .flush()
            .map_err(|err| format!("failed to flush test-server request: {err}"))?;

        self.wait_for_response(&response_path)
    }

    fn wait_for_response(&mut self, response_path: &Path) -> Result<PersistentMoonOutput, String> {
        loop {
            match std::fs::read(response_path) {
                Ok(bytes) => match serde_json::from_slice::<TestServerResponse>(&bytes) {
                    Ok(response) => {
                        let _ = std::fs::remove_file(response_path);
                        return Ok(PersistentMoonOutput {
                            exit_code: response.exit_code,
                            stdout: response.stdout.into_bytes(),
                            stderr: response.stderr.into_bytes(),
                        });
                    }
                    Err(err) if err.is_eof() => {}
                    Err(err) => {
                        return Err(format!(
                            "failed to decode test-server response {}: {err}",
                            response_path.display()
                        ));
                    }
                },
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(format!(
                        "failed to read test-server response {}: {err}",
                        response_path.display()
                    ));
                }
            }

            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|err| format!("failed to query test-server status: {err}"))?
            {
                return Err(format!("test-server exited before responding: {status}"));
            }

            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for PersistentMoonSession {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn next_response_path() -> PathBuf {
    let id = RESPONSE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "moon-test-server-response-{}-{id}.json",
        std::process::id()
    ))
}

fn os_to_string(value: impl AsRef<OsStr>) -> String {
    value.as_ref().to_string_lossy().into_owned()
}
