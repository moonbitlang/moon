use std::process::Stdio;

use anyhow::bail;
use moonutil::{
    cli::UniversalFlags,
    mooncake_bin::call_mooncake,
    mooncakes::{LoginSubcommand, MooncakeSubcommands, PublishSubcommand, RegisterSubcommand},
};
use serde::Serialize;

pub fn execute_cli<T: Serialize>(
    cli: UniversalFlags,
    cmd: T,
    args: &[&str],
) -> anyhow::Result<i32> {
    let mut child = call_mooncake()
        .args(args)
        .stdout(Stdio::inherit())
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        let data = (cli, cmd);
        serde_json::ser::to_writer(&mut stdin, &data)?;
    } else {
        eprintln!("failed to open stdin");
    }

    let status = child.wait()?;
    if status.success() {
        Ok(0)
    } else {
        bail!("failed to run")
    }
}

pub fn login_cli(cli: UniversalFlags, cmd: LoginSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Login(cmd),
        &["--read-args-from-stdin"],
    )
}

pub fn register_cli(cli: UniversalFlags, cmd: RegisterSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Register(cmd),
        &["--read-args-from-stdin"],
    )
}

pub fn publish_cli(cli: UniversalFlags, cmd: PublishSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
    )
}
