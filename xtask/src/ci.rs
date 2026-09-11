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
                    "-C",
                    "crates/moonbuild/template/test_driver_project",
                    "check",
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
                    "-C",
                    "crates/moonbuild/template/test_driver_project",
                    "fmt",
                    "--check",
                ],
            },
            fix_script: Some(
                "cargo run --bin moon -- -C crates/moonbuild/template/test_driver_project fmt",
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
            label: "cargo clippy --workspace --exclude moonrun --all-targets --all-features -- -D warnings",
            command: Command {
                program: "cargo",
                args: &[
                    "clippy",
                    "--workspace",
                    "--exclude",
                    "moonrun",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ],
            },
            fix_script: Some(
                "cargo clippy --fix --workspace --exclude moonrun --all-targets --all-features --allow-dirty --allow-staged",
            ),
        },
        Check {
            label: "cargo clippy -p moonrun --all-targets -- -D warnings",
            command: Command {
                program: "cargo",
                args: &[
                    "clippy",
                    "-p",
                    "moonrun",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
            },
            fix_script: Some(
                "cargo clippy --fix -p moonrun --all-targets --allow-dirty --allow-staged",
            ),
        },
        Check {
            label: "cargo clippy -p moonrun --all-targets --no-default-features --features wasmtime -- -D warnings",
            command: Command {
                program: "cargo",
                args: &[
                    "clippy",
                    "-p",
                    "moonrun",
                    "--all-targets",
                    "--no-default-features",
                    "--features",
                    "wasmtime",
                    "--",
                    "-D",
                    "warnings",
                ],
            },
            fix_script: Some(
                "cargo clippy --fix -p moonrun --all-targets --no-default-features --features wasmtime --allow-dirty --allow-staged",
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
