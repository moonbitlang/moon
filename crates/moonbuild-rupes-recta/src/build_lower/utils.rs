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

//! Utilities for N2 graph manipulation.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    rc::Rc,
};

use n2::graph::{BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};

/// Create a [`n2::graph::BuildIns`] with all explicit input (because why not?).
pub fn build_ins(
    graph: &mut N2Graph,
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> BuildIns {
    // this might hint the vec with iterator size
    let file_ids: Vec<_> = paths
        .into_iter()
        .map(|x| register_file(graph, x.as_ref()))
        .collect();
    BuildIns {
        explicit: file_ids.len(),
        ids: file_ids,
        implicit: 0,
        order_only: 0,
    }
}

/// Create a [`n2::graph::BuildOuts`] with all explicit output.
pub fn build_outs(
    graph: &mut N2Graph,
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> BuildOuts {
    // this might hint the vec with iterator size
    let file_ids: Vec<_> = paths
        .into_iter()
        .map(|x| register_file(graph, x.as_ref()))
        .collect();
    BuildOuts {
        explicit: file_ids.len(),
        ids: file_ids,
    }
}

/// Create a dummy [`FileLoc`] for the given file name. This is a little bit
/// wasteful in terms of memory usage, but should do the job.
pub fn build_n2_fileloc(name: impl Into<PathBuf>) -> FileLoc {
    FileLoc {
        filename: Rc::new(name.into()),
        line: 0,
    }
}

/// Return the trackable MoonBit toolchain binaries used by a structured command.
///
/// This intentionally tracks only known MoonBit-owned tools. Other binaries,
/// such as C compilers or shell utilities, have platform-specific resolution
/// behavior and are left for future work.
pub fn command_tool_inputs(args: &[String]) -> Vec<PathBuf> {
    command_tool_inputs_with_extra(args, std::iter::empty::<PathBuf>())
}

/// Return the trackable MoonBit toolchain binaries used by a structured
/// command, including extra tools used by wrapper commands.
pub fn command_tool_inputs_with_extra(
    args: &[String],
    extra_tools: impl IntoIterator<Item = impl AsRef<Path>>,
) -> Vec<PathBuf> {
    let mut inputs = BTreeSet::new();
    let moon_tools = [
        &*moonutil::BINARIES.moonbuild,
        &*moonutil::BINARIES.moonc,
        &*moonutil::BINARIES.mooncake,
        &*moonutil::BINARIES.moon_ide,
        &*moonutil::BINARIES.moondoc,
        &*moonutil::BINARIES.moonfmt,
        &*moonutil::BINARIES.mooninfo,
        &*moonutil::BINARIES.moonlex,
        &*moonutil::BINARIES.moonrun,
        &*moonutil::BINARIES.moonyacc,
        &*moonutil::BINARIES.moon_cram,
        &*moonutil::BINARIES.moon_cove_report,
    ];
    for arg in args {
        let arg_path = Path::new(arg);
        if moon_tools.iter().any(|tool| arg_path == tool.as_path()) {
            insert_trackable_tool_input(&mut inputs, arg_path);
        }
    }

    for tool in extra_tools {
        let tool = tool.as_ref();
        if moon_tools
            .iter()
            .any(|moon_tool| tool == moon_tool.as_path())
        {
            insert_trackable_tool_input(&mut inputs, tool);
        }
    }

    inputs.into_iter().collect()
}

fn register_file(graph: &mut N2Graph, path: &Path) -> FileId {
    // nah, n2 accepts strings but we're mainly working with `PathBuf`s, so
    // a lot of copying is happening here -- but shouldn't be perf bottleneck
    graph
        .files
        .id_from_canonical(path.to_string_lossy().into_owned())
}

fn insert_trackable_tool_input(inputs: &mut BTreeSet<PathBuf>, path: &Path) {
    // Explicit env overrides may still be bare names; n2 can only track paths.
    if path.is_absolute() || path.components().count() > 1 {
        inputs.insert(path.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::{command_tool_inputs, command_tool_inputs_with_extra};
    use std::path::PathBuf;

    #[test]
    fn command_tool_inputs_tracks_toolchain_programs() {
        let moonfmt = moonutil::BINARIES.moonfmt.clone();
        let inputs = command_tool_inputs(&[
            moonfmt.to_string_lossy().into_owned(),
            "/tmp/source.mbt".to_string(),
            "-o".to_string(),
            "/tmp/output.mbt".to_string(),
        ]);

        assert_eq!(inputs, expected_trackable_tools([moonfmt]));
    }

    #[test]
    fn command_tool_inputs_ignores_external_programs() {
        let inputs = command_tool_inputs(&[
            "/toolchain/bin/cc".to_string(),
            "/tmp/source".to_string(),
            "/tmp/output".to_string(),
        ]);

        assert!(inputs.is_empty());
    }

    #[test]
    fn command_tool_inputs_ignores_bare_external_programs() {
        let inputs = command_tool_inputs(&[
            "cp".to_string(),
            "/tmp/source".to_string(),
            "/tmp/output".to_string(),
        ]);

        assert!(inputs.is_empty());
    }

    #[test]
    fn command_tool_inputs_tracks_known_toolchain_payloads() {
        let moonrun = moonutil::BINARIES.moonrun.clone();
        let moonlex = moonutil::BINARIES.moonlex.clone();
        let inputs = command_tool_inputs(&[
            moonrun.to_string_lossy().into_owned(),
            moonlex.to_string_lossy().into_owned(),
            "--".to_string(),
        ]);

        assert_eq!(inputs, expected_trackable_tools([moonrun, moonlex]));
    }

    #[test]
    fn command_tool_inputs_accepts_wrapper_tools() {
        let moon = moonutil::BINARIES.moonbuild.clone();
        let moonfmt = moonutil::BINARIES.moonfmt.clone();
        let inputs = command_tool_inputs_with_extra(
            &[
                moon.to_string_lossy().into_owned(),
                "tool".to_string(),
                "format-and-diff".to_string(),
            ],
            [moonfmt.clone()],
        );

        assert_eq!(inputs, expected_trackable_tools([moon, moonfmt]));
    }

    #[test]
    fn command_tool_inputs_ignores_external_wrapper_tools() {
        let moon = moonutil::BINARIES.moonbuild.clone();
        let inputs = command_tool_inputs_with_extra(
            &[
                moon.to_string_lossy().into_owned(),
                "tool".to_string(),
                "format-workspace".to_string(),
            ],
            [PathBuf::from("/usr/bin/git")],
        );

        assert_eq!(inputs, expected_trackable_tools([moon]));
    }

    fn expected_trackable_tools(tools: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        let mut tools = tools
            .into_iter()
            .filter(|path| path.is_absolute() || path.components().count() > 1)
            .collect::<Vec<_>>();
        tools.sort();
        tools
    }
}
