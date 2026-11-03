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

//! Run `ipc_execution host`, then use its printed socket path with
//! `ipc_execution SOCKET PROGRAM.wasm [ARGS...] < input > output 2> errors`.
//! This one-invocation example is not a daemon or a replacement moonrun CLI.

#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    use moonrun::ipc::{ClientRun, Invocation, serve};
    use moonrun::{Engine, RunOptions, RunOutcome};
    use std::fs::File;
    use std::io::Write;
    use std::os::fd::AsFd;
    use std::os::unix::net::{UnixListener, UnixStream};

    let mut args = std::env::args_os().skip(1);
    let entry = args
        .next()
        .context("expected host or SOCKET PROGRAM.wasm [ARGS...]")?;
    if entry == "host" {
        // The private directory is the admission mechanism for this example:
        // peers with access execute with the current user's host authority.
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("moonrun.sock");
        let listener = UnixListener::bind(&path)?;
        println!("{}", path.display());
        std::io::stdout().flush()?;
        return serve(
            &Engine::default(),
            listener.accept()?.0,
            RunOptions::default(),
        );
    }
    let program = std::fs::canonicalize(args.next().context("expected PROGRAM.wasm")?)?;
    // Consume reserved inherited handles before capturing ordinary Env.
    let inherited_policy = moonutil::policy_transport::PolicyTransfer::take_from_env()?
        .map(|transfer| transfer.read())
        .transpose()?;
    let request = Invocation {
        program,
        args: args
            .map(|arg| {
                arg.into_string()
                    .map_err(|_| anyhow::anyhow!("program arguments must be UTF-8"))
            })
            .collect::<anyhow::Result<_>>()?,
        working_directory: std::env::current_dir()?,
        environment: std::env::vars_os().collect(),
        inherited_policy,
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let streams = [stdin.as_fd(), stdout.as_fd(), stderr.as_fd()]
        .map(|fd| rustix::io::fcntl_dupfd_cloexec(fd, 3).map(File::from));
    let [stdin, stdout, stderr] = streams;
    let result = ClientRun::submit(
        UnixStream::connect(entry)?,
        request,
        [stdin?, stdout?, stderr?],
    )?
    .wait()?;
    let code = match result {
        RunOutcome::Completed => 0,
        RunOutcome::Exited(code) => code,
        RunOutcome::KilledBySignal(signal) => 128 + signal,
    };
    // This example presents signal outcomes as shell-style status only.
    // A real CLI adapter must define signal forwarding and native presentation.
    std::process::exit(code);
}

#[cfg(not(unix))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("the experimental IPC example currently requires Unix")
}
