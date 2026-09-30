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

//! Content-based execution and cached action results.
//!
//! Each digest owns a directory with a stable lock file. A waiter checks the
//! result after acquiring that lock, so it can reuse the previous owner's work.
//! The caller separately holds the target lock while mutable outputs are used.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
};

use anyhow::{Context, bail, ensure};
use moonbuild_rupes_recta::execution_plan::{
    ExecutionAction, InputObservation, LoweredCommandExecution,
};
use moonutil::{
    cache::{CacheKind, resolve_cache_root},
    locks::lock_directory,
    target::TargetBackend,
    user_log::UserLog,
};
use serde::{Deserialize, Serialize};

use super::{
    BuildConfig, BuildInput, CapturedActionOutput, CapturedBuildExecution, ResultCatcher,
    action_identity::{ActionDigest, ActionIdentityContext, compute_action_identities},
    resolve_parallelism,
};

#[tracing::instrument(skip_all)]
pub(super) fn execute<'a>(
    cfg: &BuildConfig,
    input: &BuildInput,
    outputs: impl IntoIterator<Item = &'a Path>,
    user_log: &UserLog,
) -> anyhow::Result<CapturedBuildExecution> {
    let plan = input.execution_plan.as_ref();
    // TODO: Enable native backends after their implicit SDK/header inputs and
    // directory-shaped debug outputs have a reusable execution model.
    ensure!(
        input.action_backends.values().all(|backend| {
            backend.is_none_or(|backend| {
                matches!(backend, TargetBackend::Wasm | TargetBackend::WasmGC)
            })
        }),
        "the experimental hash engine currently supports only wasm and wasm-gc backends"
    );
    let parallelism = resolve_parallelism(cfg.parallelism);
    ensure!(
        parallelism > 0,
        "build parallelism must be greater than zero"
    );

    let dependencies = plan
        .action_ids()
        .map(|id| {
            let producers = plan
                .action(id)
                .inputs()
                .iter()
                .filter_map(|observation| match observation {
                    InputObservation::File(path) => {
                        plan.declared_output(path).map(|out| out.producer())
                    }
                    InputObservation::StandardLibraryInterfaces(_) => None,
                })
                .collect::<HashSet<_>>();
            (id, producers)
        })
        .collect::<HashMap<_, _>>();
    let mut pending = outputs
        .into_iter()
        .map(|path| {
            plan.declared_output(path)
                .map(|out| out.producer())
                .with_context(|| format!("unknown requested build output: {}", path.display()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut wanted = HashSet::new();
    while let Some(id) = pending.pop() {
        if wanted.insert(id) {
            pending.extend(&dependencies[&id]);
        }
    }
    let actions = plan
        .action_ids()
        .filter(|id| wanted.contains(id))
        .collect::<Vec<_>>();
    let context =
        ActionIdentityContext::new(std::env::current_dir()?, std::env::vars_os().collect());
    let identities = compute_action_identities(plan, &context, &actions)?;
    let identities = actions
        .iter()
        .copied()
        .zip(identities)
        .collect::<HashMap<_, _>>();
    let configured_cache = resolve_cache_root(CacheKind::BuildArtifacts)?;
    let cache_root = configured_cache.initialize()?.map(|root| root.join("v1"));
    if let Some(root) = &cache_root {
        fs::create_dir_all(root)?;
    }

    let mut remaining = actions
        .iter()
        .map(|id| (*id, dependencies[id].len()))
        .collect::<HashMap<_, _>>();
    let mut consumers = HashMap::<_, Vec<_>>::new();
    for &id in &actions {
        for &producer in &dependencies[&id] {
            consumers.entry(producer).or_default().push(id);
        }
    }
    let mut ready = actions
        .iter()
        .copied()
        .filter(|id| remaining[id] == 0)
        .collect::<VecDeque<_>>();
    let mut executed = 0;
    let mut completed = 0;
    let mut failures = 0;
    let mut captured = Vec::new();
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let (sender, receiver) = mpsc::channel();
        let mut running = 0;
        loop {
            while running < parallelism && failures < 10 {
                let Some(id) = ready.pop_front() else { break };
                let sender = sender.clone();
                let action = plan.action(id);
                let identity = identities[&id];
                let cache_dir =
                    cache_root
                        .as_ref()
                        .filter(|_| identity.is_cacheable())
                        .map(|root| {
                            let digest = identity.digest().to_hex();
                            root.join(&digest[..2]).join(&digest[2..])
                        });
                let context = &context;
                scope.spawn(move || {
                    let result = run_action(
                        action,
                        cache_dir.as_deref(),
                        identity.digest(),
                        context,
                        cfg,
                        user_log,
                    )
                    .with_context(|| format!("failed to execute {}", action.description()));
                    // A scheduler error still joins running children before releasing
                    // its caller's target lock. Their completed cache results remain valid.
                    let _ = sender.send((id, result));
                });
                running += 1;
            }
            if running == 0 {
                break;
            }
            let (id, result) = receiver
                .recv()
                .context("hash executor worker disconnected")?;
            running -= 1;
            let result = result?;
            executed += usize::from(result.executed);
            completed += 1;
            let mut content = ResultCatcher::default();
            for line in String::from_utf8_lossy(&result.output)
                .split('\n')
                .filter(|line| !line.is_empty())
            {
                content.append_content(line);
            }
            captured.push(CapturedActionOutput {
                target_backend: input.action_backends.get(&id).copied().flatten(),
                content,
            });
            if result.exit_code != Some(0) {
                failures += 1;
                // Signal termination must not start additional work or publish a
                // success. Ordinary compiler failures still allow independent actions.
                if result.exit_code.is_none() {
                    failures = 10;
                }
                continue;
            }
            for consumer in consumers.get(&id).into_iter().flatten() {
                let count = remaining
                    .get_mut(consumer)
                    .expect("consumer belongs to requested closure");
                *count -= 1;
                if *count == 0 {
                    ready.push_back(*consumer);
                }
            }
        }
        ensure!(
            failures != 0 || completed == actions.len(),
            "execution action graph contains a cycle"
        );
        Ok(())
    })?;
    Ok(CapturedBuildExecution {
        n_tasks_executed: (failures == 0).then_some(executed),
        action_outputs: captured,
    })
}

struct ActionResult {
    // None denotes signal/abnormal termination. Failed launches are I/O errors.
    exit_code: Option<i32>,
    output: Vec<u8>,
    executed: bool,
}

fn run_action(
    action: &ExecutionAction,
    cache_dir: Option<&Path>,
    digest: ActionDigest,
    context: &ActionIdentityContext,
    cfg: &BuildConfig,
    user_log: &UserLog,
) -> anyhow::Result<ActionResult> {
    let _lock = if let Some(directory) = cache_dir {
        fs::create_dir_all(directory)?;
        let lock = lock_directory(directory, user_log)?;
        if let Some(result) = restore_result(directory, digest, action.outputs())? {
            return Ok(result);
        }
        Some(lock)
    } else {
        None
    };

    // A success must produce every declared file during this execution; an old
    // output must not make an incomplete command look publishable.
    if cache_dir.is_some() {
        for output in action.outputs() {
            match fs::remove_file(output) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    let lowered = action.command();
    let cwd = context
        .inherited_working_directory
        .join(lowered.cwd().unwrap_or(Path::new("")));
    for path in action.outputs() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    let command_line = match lowered.execution() {
        LoweredCommandExecution::Inline(command) => command,
        LoweredCommandExecution::ResponseFile { command, file } => {
            let path = cwd.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, &file.content)?;
            command
        }
    };
    if !cfg.suppress_progress {
        user_log.status(if cfg.verbose {
            command_line
        } else {
            action.description()
        });
    }
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", command_line]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let (program, arguments) = moonutil::shlex::split_argv0_windows(command_line);
        let mut command = Command::new(program);
        command.raw_arg(arguments.trim_start());
        command
    };
    // Match the lowered transport and give hashing and execution the same
    // inherited process state. One pipe retains stdout/stderr byte ordering.
    let (mut reader, writer) = std::io::pipe()?;
    let mut child = command
        .current_dir(cwd)
        .env_clear()
        .envs(context.inherited_environment.iter().cloned())
        .envs(lowered.env().iter().cloned())
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .spawn()?;
    drop(command); // Drop the parent's write handles before draining to EOF.
    let mut output = Vec::new();
    if let Err(error) = reader.read_to_end(&mut output) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    let status = child.wait()?;
    // Shell statuses 126/127 denote launch errors, and 128+ can represent
    // signal termination. Neither is a reusable compiler result.
    if let Some(exit_code @ 0..=125) = status.code()
        && let Some(directory) = cache_dir
    {
        publish_result(
            directory,
            digest,
            exit_code,
            if status.success() {
                action.outputs()
            } else {
                &[]
            },
            &output,
        )?;
    }
    Ok(ActionResult {
        exit_code: status.code(),
        output,
        executed: true,
    })
}

// Failed compiler results contain diagnostics and an exit status, never output
// artifacts. Filenames are ordinals, never paths supplied by the record.
#[derive(Deserialize, Serialize)]
struct Record {
    action: String,
    exit_code: i32,
    outputs: Vec<StoredOutput>,
    diagnostics: [u8; 32],
}

#[derive(Deserialize, Serialize)]
struct StoredOutput {
    path: PathBuf,
    digest: [u8; 32],
}

fn restore_result(
    directory: &Path,
    digest: ActionDigest,
    outputs: &[PathBuf],
) -> anyhow::Result<Option<ActionResult>> {
    let result = directory.join("result");
    let record = match fs::read(result.join("record.json")) {
        Ok(bytes) => match serde_json::from_slice::<Record>(&bytes) {
            Ok(record) => record,
            Err(_) => return Ok(None),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let outputs = if record.exit_code == 0 { outputs } else { &[] };
    if record.action != digest.to_hex()
        || !(0..=125).contains(&record.exit_code)
        || record.outputs.len() != outputs.len()
        || record
            .outputs
            .iter()
            .zip(outputs)
            .any(|(stored, output)| stored.path != *output)
    {
        return Ok(None);
    }
    // Validate the complete result before replacing any project outputs. A
    // truncated, missing, or corrupt blob is a miss, never a partial hit.
    let mut blobs = Vec::new();
    for (index, stored) in record.outputs.iter().enumerate() {
        let path = result.join(index.to_string());
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if *blake3::hash(&bytes).as_bytes() != stored.digest {
            return Ok(None);
        }
        blobs.push((bytes, fs::metadata(path)?.permissions()));
    }
    let diagnostics = match fs::read(result.join("diagnostics")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if *blake3::hash(&diagnostics).as_bytes() != record.diagnostics {
        return Ok(None);
    }
    for ((bytes, permissions), output) in blobs.into_iter().zip(outputs) {
        // Avoid touching already-correct outputs. In particular, sharing results
        // with a waiting invocation must not trigger timestamp-based consumers.
        if fs::read(output).is_ok_and(|existing| existing == bytes) {
            continue;
        }
        let parent = output.parent().context("action output has no parent")?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().set_permissions(permissions)?;
        file.persist(output)?;
    }
    Ok(Some(ActionResult {
        exit_code: Some(record.exit_code),
        output: diagnostics,
        executed: false,
    }))
}

fn publish_result(
    directory: &Path,
    digest: ActionDigest,
    exit_code: i32,
    outputs: &[PathBuf],
    diagnostics: &[u8],
) -> anyhow::Result<()> {
    let staging = tempfile::Builder::new()
        .prefix("staging-")
        .tempdir_in(directory)?;
    let mut stored = Vec::new();
    for (index, output) in outputs.iter().enumerate() {
        let metadata = fs::symlink_metadata(output)
            .with_context(|| format!("successful action did not produce {}", output.display()))?;
        if !metadata.is_file() {
            bail!(
                "hash cache currently requires regular file outputs: {}",
                output.display()
            );
        }
        let blob = staging.path().join(index.to_string());
        fs::copy(output, &blob)?;
        let mut hash = blake3::Hasher::new();
        hash.update_reader(fs::File::open(&blob)?)?;
        stored.push(StoredOutput {
            path: output.clone(),
            digest: *hash.finalize().as_bytes(),
        });
    }
    fs::write(staging.path().join("diagnostics"), diagnostics)?;
    let record = Record {
        action: digest.to_hex(),
        exit_code,
        outputs: stored,
        diagnostics: *blake3::hash(diagnostics).as_bytes(),
    };
    fs::write(
        staging.path().join("record.json"),
        serde_json::to_vec(&record)?,
    )?;
    let result = directory.join("result");
    match fs::remove_dir_all(&result) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // All files precede publication; the stable lock directory is never renamed
    // or removed. A killed writer leaves a miss for the next lock owner.
    fs::rename(staging.path(), result)?;
    Ok(())
}
