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

use anyhow::{Context, bail};

use crate::Ci;

pub(crate) fn run(_ci: &Ci) -> anyhow::Result<()> {
    let checks = [
        Check {
            label: "moon check",
            command: Command {
                program: "cargo",
                args: &[
                    "run",
                    "--bin",
                    "moon",
                    "--",
                    "check",
                    "--manifest-path",
                    "crates/moonbuild/template/test_driver_project/moon.mod.json",
                ],
            },
            fix_script: None,
        },
        Check {
            label: "moon fmt --check",
            command: Command {
                program: "cargo",
                args: &[
                    "run",
                    "--bin",
                    "moon",
                    "--",
                    "fmt",
                    "--check",
                    "--manifest-path",
                    "crates/moonbuild/template/test_driver_project/moon.mod.json",
                ],
            },
            fix_script: Some(
                "cargo run --bin moon -- fmt --manifest-path crates/moonbuild/template/test_driver_project/moon.mod.json",
            ),
        },
        Check {
            label: "cargo fmt -- --check",
            command: Command {
                program: "cargo",
                args: &["fmt", "--", "--check"],
            },
            fix_script: Some("cargo fmt"),
        },
        Check {
            label: "cargo clippy --all-targets --all-features -- -D warnings",
            command: Command {
                program: "cargo",
                args: &[
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
            },
            fix_script: Some(
                "cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged",
            ),
        },
    ];

    let mut failures = Vec::new();
    for check in &checks {
        if let Err(err) = run_command(&check.command) {
            eprintln!("error: {err:#}");
            failures.push(check);
        }
    }

    if failures.is_empty() {
        return Ok(());
    }

    let mut suggested = Vec::new();
    let mut has_manual_only = false;
    for failed in &failures {
        match failed.fix_script {
            Some(script) => {
                if !suggested.contains(&script) {
                    suggested.push(script);
                }
            }
            None => {
                has_manual_only = true;
            }
        }
    }

    if !suggested.is_empty() {
        eprintln!("hint: copy/paste and run:");
        for script in &suggested {
            eprintln!("{script}");
        }
    }
    if has_manual_only {
        eprintln!(
            "hint: some failed checks have no automatic fix command and need manual changes."
        );
    }

    let failed_labels = failures.iter().map(|c| c.label).collect::<Vec<_>>();
    bail!("failed commands: {}", failed_labels.join(", "))
}

struct Check {
    label: &'static str,
    command: Command,
    fix_script: Option<&'static str>,
}

struct Command {
    program: &'static str,
    args: &'static [&'static str],
}

fn run_command(command: &Command) -> anyhow::Result<()> {
    let cmdline = format!("{} {}", command.program, command.args.join(" "));
    println!("+ {cmdline}");
    let status = std::process::Command::new(command.program)
        .args(command.args)
        .status()
        .with_context(|| format!("failed to run `{cmdline}`"))?;
    if !status.success() {
        bail!("command `{cmdline}` failed: {status}");
    }
    Ok(())
}
