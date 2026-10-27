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

use anyhow::{Context, bail};
use moonutil::registry::RegistryDownloadRequest;
use std::process::{Command, Stdio};

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
