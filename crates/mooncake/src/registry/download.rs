// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use anyhow::Context;

// Resource downloads share this client's connection pool within one operation.
// FIXME: Measure negotiated HTTP versions and connection reuse separately from
// concurrency before tuning HTTP/2 or extending pool lifetime across operations.
pub(crate) fn client_with_retry() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")))
        .retry(retry_policy())
        .build()
        .context("failed to create HTTP client")
}

fn retry_policy() -> reqwest::retry::Builder {
    // Retry across redirects to other hosts.
    // Reqwest has no public all-hosts retry scope, so use this wrapper.
    struct AnyHost;
    impl PartialEq<&str> for AnyHost {
        fn eq(&self, _: &&str) -> bool {
            true
        }
    }

    reqwest::retry::for_host(AnyHost)
        .max_retries_per_request(3)
        .classify_fn(|response| {
            if response.error().is_some()
                || matches!(
                    response.status().map(|status| status.as_u16()),
                    Some(408 | 429 | 500 | 502 | 503 | 504)
                )
            {
                response.retryable()
            } else {
                response.success()
            }
        })
}

/// Wait for an archive lock without blocking the thread driving downloads.
/// Like `lock_file`, the returned open file owns the lock and the lock file
/// remains on disk. Contention must yield so other downloads can finish and
/// release their own locks, including when processes request different orders.
pub(super) async fn lock_archive(
    path: &std::path::Path,
    user_log: &moonutil::user_log::UserLog,
) -> std::io::Result<std::fs::File> {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    let started = std::time::Instant::now();
    let mut reported = false;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                if !reported && started.elapsed() >= std::time::Duration::from_secs(1) {
                    user_log.status(format!(
                        "Blocking waiting for file lock {} ...",
                        std::path::Path::new(&lock_path).display()
                    ));
                    reported = true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };

    #[tokio::test]
    async fn retries_after_cross_host_redirect() {
        for (status, attempts) in [("503 Service Unavailable", 4), ("404 Not Found", 1)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                for attempt in 0..=attempts {
                    let (mut stream, _) = listener.accept().unwrap();
                    for line in BufReader::new(&stream).lines() {
                        if line.unwrap().is_empty() {
                            break;
                        }
                    }
                    let response = if attempt == 0 {
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://localhost:{}/download\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            address.port()
                        )
                    } else {
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{attempt}"
                        )
                    };
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
            let response = super::client_with_retry()
                .unwrap()
                .get(format!("http://{address}/download"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), attempts.to_string());
            server.join().unwrap();
        }
    }
}
