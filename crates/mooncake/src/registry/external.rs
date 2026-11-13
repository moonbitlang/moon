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

//! One-shot source acquisition by the external mooncake executable.

use super::credentials::RegistryCredentials;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

/// One source acquisition delegated to the external mooncake executable.
/// Sent through stdin only. Deliberately not Debug: it carries the login captured
/// by the caller, so metadata, authorization and download renewal use one token.
#[derive(Serialize, Deserialize)]
pub(super) struct RegistryDownloadRequest {
    pub api: String,
    pub module: String,
    pub version: semver::Version,
    pub checksum: String,
    pub destination: std::path::PathBuf,
    pub credentials: RegistryCredentials,
}

pub(super) fn download_archive(request: &RegistryDownloadRequest) -> anyhow::Result<()> {
    let mut child = Command::new(&*moonutil::toolchain::BINARIES.mooncake)
        .arg("--download-archive")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start mooncake for source download")?;
    // Closing stdin completes the request. The captured token is never placed
    // in argv, the environment or a temporary credentials file.
    let result = serde_json::to_writer(child.stdin.take().expect("piped stdin"), request);
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        bail!("failed to send the download request to mooncake; update the toolchain");
    }
    if !child.wait()?.success() {
        bail!(
            "mooncake failed to download {}@{}",
            request.module,
            request.version
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_request_preserves_wire_format() {
        // The installed mooncake reads this stdin protocol independently of
        // Moon's Rust types. Both old and API-bound logins must keep their shape.
        for credentials in [
            serde_json::json!({"token": "test-only-token"}),
            serde_json::json!({
                "token": "test-only-token",
                "username": "alice",
                "registry_api": "https://registry.example/prefix"
            }),
        ] {
            let wire = serde_json::json!({
                "api": "https://registry.example/prefix",
                "module": "alice/pkg",
                "version": "1.2.3",
                "checksum": "test-checksum",
                "destination": "archive.zip",
                "credentials": credentials
            });
            let request: RegistryDownloadRequest = serde_json::from_value(wire.clone()).unwrap();
            let mut encoded = Vec::new();
            serde_json::to_writer(&mut encoded, &request).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
                wire
            );
        }
    }
}
