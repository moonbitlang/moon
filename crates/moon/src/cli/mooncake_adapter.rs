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

pub fn execute_cli_with_inherit_stdin<T: Serialize>(
    _cli: UniversalFlags,
    _cmd: T,
    args: &[&str],
) -> anyhow::Result<i32> {
    let mut child = call_mooncake()
        .args(args)
        .env("MOONCAKE_ALLOW_DIRECT", "1")
        .stdout(Stdio::inherit())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let status = child.wait()?;
    if status.success() {
        Ok(0)
    } else {
        bail!("failed to run `moon {}`", args.join(" "))
    }
}

pub fn login_cli(cli: UniversalFlags, cmd: LoginSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(cli, MooncakeSubcommands::Login(cmd), &["login"])
}

pub fn register_cli(cli: UniversalFlags, cmd: RegisterSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(cli, MooncakeSubcommands::Register(cmd), &["register"])
}

pub fn publish_cli(cli: UniversalFlags, cmd: PublishSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
    )
}
