use anyhow::{bail, Ok};
use clap::{arg, ArgAction, Command};
use xshell::{cmd, Shell};

fn build_moon(is_release: bool) -> anyhow::Result<()> {
    let sh = Shell::new()?;
    if is_release {
        cmd!(sh, "cargo build --package moon --release").run()?;
    } else {
        cmd!(sh, "cargo build --package moon").run()?;
    }
    Ok(())
}

fn cli() -> Command {
    clap::Command::new("xtask")
        .about("Some xtasks")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("build")
                .about("build")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("moon")
                        .about("build moon")
                        .arg(arg!(--release <bool>).action(ArgAction::SetTrue)),
                ),
        )
}

fn main() -> anyhow::Result<()> {
    let matches = cli().get_matches();

    match matches.subcommand() {
        Some(("build", sub_matches)) => match sub_matches.subcommand() {
            Some(("moon", sub_sub_matches)) => {
                let release = sub_sub_matches.get_flag("release");
                build_moon(release)?;
            }
            _ => unreachable!(),
        },
        _ => bail!("Invalid arguments"),
    }
    Ok(())
}
