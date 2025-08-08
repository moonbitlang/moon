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

//! Run tests and interpret the results

use std::collections::HashMap;

use anyhow::Context;
use moonbuild::entry::TestArgs;
use moonbuild_rupes_recta::model::{BuildPlanNode, TargetAction};
use moonutil::common::MooncGenTestInfo;

use crate::{rr_build::CompileOutput, run::default_rt};

/// Run the tests compiled in this session. Returns `true` if all tests passed,
/// `false` if any test failed. TODO: actual test info display
pub fn run_tests(ret: &CompileOutput) -> anyhow::Result<bool> {
    // Gathering artifacts
    let mut pending = HashMap::new();
    let mut results = vec![]; // (executable, metadata)
    for artifacts in &ret.artifacts {
        let corresponding = match artifacts.node.action {
            TargetAction::MakeExecutable => TargetAction::GenerateTestInfo,
            TargetAction::GenerateTestInfo => TargetAction::MakeExecutable,
            _ => unreachable!(),
        };
        let removed = pending.remove(&BuildPlanNode {
            target: artifacts.node.target,
            action: corresponding,
        });
        let artifact = match artifacts.node.action {
            // FIXME: artifact index relies on implementation of append_artifact_of
            TargetAction::MakeExecutable => &artifacts.artifacts[0],
            TargetAction::GenerateTestInfo => &artifacts.artifacts[1],
            _ => unreachable!(),
        };

        let (exec, meta) = match (removed, artifacts.node.action) {
            (Some(other), TargetAction::GenerateTestInfo) => (other, artifact),
            (Some(other), TargetAction::MakeExecutable) => (artifact, other),
            _ => {
                pending.insert(artifacts.node, artifact);
                continue;
            }
        };
        results.push((artifacts.node.target, exec, meta));
    }

    let rt = default_rt().context("Failed to create runtime")?;
    for (node, executable, metadata_path) in results {
        let metadata =
            std::fs::File::open(metadata_path).context("Failed to open test metadata")?;
        let metadata = serde_json_lenient::from_reader::<_, MooncGenTestInfo>(metadata)
            .with_context(|| {
                format!(
                    "Failed to parse test metadata at {}",
                    metadata_path.display()
                )
            })?;

        let pkgname = ret
            .resolve_output
            .pkg_dirs
            .get_package(node.package)
            .fqn
            .to_string();

        // Convert MooncGenTestInfo to TestArgs format
        let mut test_args = TestArgs {
            package: pkgname,
            file_and_index: vec![],
        };

        // TODO: add file filtering
        for lists in [
            &metadata.no_args_tests,
            &metadata.with_args_tests,
            &metadata.with_bench_args_tests,
        ] {
            for (filename, test_infos) in lists {
                if !test_infos.is_empty() {
                    let max_index = test_infos.iter().map(|t| t.index).max().unwrap_or(0);
                    test_args
                        .file_and_index
                        .push((filename.to_string(), 0..(max_index + 1)));
                }
            }
        }

        let cmd = crate::run::command_for(ret.target_backend, executable, Some(&test_args));
        rt.block_on(crate::run::run(&mut [], false, cmd))
            .context("Failed to run test")?; // TODO
    }

    Ok(true)
}
