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

use std::{
    io::{Read, Seek},
    path::Path,
};

/// CPU work runs on a dedicated, bounded worker pool. This runtime has no
/// network tasks; source downloads finish before extraction starts.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn extract_archives(
    archives: Vec<(std::fs::File, std::path::PathBuf)>,
    jobs: Option<usize>,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use tracing::Instrument;

    let workers = jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from))
        .max(1)
        .min(archives.len());
    if workers <= 1 {
        for (archive, directory) in archives {
            extract_zip_to_dir(&directory, archive)?;
        }
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .build()
        .context("failed to start source extraction workers")?;
    // Runtime destruction waits for running synchronous jobs before callers
    // can clean up staging directories, including after an extraction fails.
    runtime.block_on(async {
        let mut tasks = tokio::task::JoinSet::new();
        for (archive, directory) in archives {
            tasks.spawn(async move { extract_zip_to_dir(&directory, archive) }.in_current_span());
        }
        while let Some(result) = tasks.join_next().await {
            result.context("source extraction task failed")??;
        }
        Ok(())
    })
}

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn extract_zip_to_dir(dest: &Path, archive: impl Read + Seek) -> anyhow::Result<()> {
    let mut zip = zip::ZipArchive::new(archive)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let outpath = dest.join(file.mangled_name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;

            // Preserve file permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
                }
            }
        }
    }
    Ok(())
}
