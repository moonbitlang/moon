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

use std::path::{Path, PathBuf};

use anyhow::Context;
use mooncake::registry::{RegistryClient, ResolvedExecutablePackage};
use moonutil::{MOON_HOME, locks::lock_directory, user_log::UserLog};

use crate::rr_build;

pub(crate) enum RegistryRunTarget {
    Wasm {
        experimental_policy: Option<PathBuf>,
        policy_relay: Option<moonutil::policy_transport::PolicyRelay>,
    },
    Native,
}

pub(super) fn ensure_cached_file(
    cache_path: &Path,
    user_log: &UserLog,
    produce: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<PathBuf> {
    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let parent = cache_path
        .parent()
        .context("registry cache path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create registry cache directory {}",
            parent.display()
        )
    })?;
    let _lock = lock_directory(parent, user_log)
        .with_context(|| format!("failed to lock cache directory {}", parent.display()))?;

    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let staged = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create cache file in {}", parent.display()))?;
    produce(staged.path())?;
    staged
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync cache file {}", staged.path().display()))?;
    staged
        .persist(cache_path)
        .with_context(|| format!("failed to publish cached file to {}", cache_path.display()))?;
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(cache_path.to_path_buf())
}

pub(crate) fn prepare(
    package: String,
    target: RegistryRunTarget,
    args: Vec<String>,
    quiet: bool,
    verbose: bool,
    user_log: &UserLog,
) -> anyhow::Result<super::process::ProcessAction> {
    let registry = RegistryClient::configured();
    match target {
        RegistryRunTarget::Wasm {
            experimental_policy,
            policy_relay,
        } => {
            let wasm_path = registry.acquire_executable_wasm(&package, user_log)?;
            prepare_artifact(
                crate::run::ExecutionMode::MoonRun,
                &wasm_path,
                experimental_policy.as_deref(),
                policy_relay,
                &args,
                user_log,
            )
        }
        RegistryRunTarget::Native => {
            let package = registry.resolve_executable_package(&package, user_log)?;
            let executable = cached_native_executable(&package, user_log, quiet, verbose)?;
            prepare_artifact(
                crate::run::ExecutionMode::Native,
                &executable,
                None,
                None,
                &args,
                user_log,
            )
        }
    }
}

fn cached_native_executable(
    package: &ResolvedExecutablePackage,
    user_log: &UserLog,
    quiet: bool,
    verbose: bool,
) -> anyhow::Result<PathBuf> {
    let cache_path = MOON_HOME.registry_executable_artifact_path(
        &package.module_name,
        &package.version,
        &package.package_path,
        &package.artifact_name(".exe"),
    );

    ensure_cached_file(&cache_path, user_log, |staged| {
        super::install_binary::build_registry_native_executable_to(
            &package.module_name,
            &package.version,
            &package.package_path,
            staged,
            quiet,
            verbose,
            user_log,
        )
    })
}

fn prepare_artifact(
    mode: crate::run::ExecutionMode<'_>,
    artifact: &Path,
    experimental_policy: Option<&Path>,
    policy_relay: Option<moonutil::policy_transport::PolicyRelay>,
    args: &[String],
    user_log: &UserLog,
) -> anyhow::Result<super::process::ProcessAction> {
    let mut run_cmd =
        crate::run::command_for_with_moonrun_policy(mode, artifact, None, experimental_policy);
    run_cmd.args(args);

    if user_log.is_enabled(log::Level::Info) {
        user_log.info(rr_build::format_dry_run_command(&run_cmd, Path::new(".")));
    }

    // Inheritance is attached immediately before delegation. Keep any ambient
    // marker out of the prepared command in both inherited and ordinary runs.
    run_cmd.env_remove(moonutil::constants::MOONRUN_INHERITED_POLICY);

    Ok(match policy_relay {
        Some(relay) => super::process::ProcessAction::DelegateWithPolicyRelay(run_cmd, relay),
        None => super::process::ProcessAction::Delegate(run_cmd),
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use anyhow::bail;

    use super::*;

    #[test]
    fn ordinary_registry_runs_remove_an_ambient_inherited_policy() {
        let action = prepare_artifact(
            crate::run::ExecutionMode::MoonRun,
            Path::new("artifact.wasm"),
            None,
            None,
            &[],
            &UserLog::new(log::LevelFilter::Warn),
        )
        .unwrap();
        let crate::cli::process::ProcessAction::Delegate(command) = action else {
            panic!("ordinary registry run unexpectedly carried an inherited policy")
        };
        assert!(command.get_envs().any(|(name, value)| {
            name == OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY) && value.is_none()
        }));
    }

    #[test]
    fn failed_production_does_not_publish_a_cache_entry() {
        let cache = tempfile::TempDir::new().unwrap();
        let final_path = cache.path().join("artifact");

        let error = ensure_cached_file(&final_path, &UserLog::new(log::LevelFilter::Warn), |_| {
            bail!("producer failed")
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "producer failed");
        assert!(!final_path.exists());
    }

    #[test]
    fn concurrent_cache_misses_produce_once_and_recheck_after_locking() {
        let cache = tempfile::TempDir::new().unwrap();
        let final_path = Arc::new(cache.path().join("artifact"));
        let start = Arc::new(Barrier::new(3));
        let production_count = Arc::new(AtomicUsize::new(0));

        let threads = (0..2)
            .map(|_| {
                let final_path = Arc::clone(&final_path);
                let start = Arc::clone(&start);
                let production_count = Arc::clone(&production_count);
                std::thread::spawn(move || {
                    start.wait();
                    ensure_cached_file(
                        &final_path,
                        &UserLog::new(log::LevelFilter::Warn),
                        |staged| {
                            production_count.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            std::fs::write(staged, b"artifact")?;
                            Ok(())
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        start.wait();

        for thread in threads {
            assert_eq!(
                std::fs::read(thread.join().unwrap().unwrap()).unwrap(),
                b"artifact"
            );
        }
        assert_eq!(production_count.load(Ordering::SeqCst), 1);
    }
}
