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

fn register_file(graph: &mut N2Graph, path: &Path) -> FileId {
    // nah, n2 accepts strings but we're mainly working with `PathBuf`s, so
    // a lot of copying is happening here -- but shouldn't be perf bottleneck
    graph
        .files
        .id_from_canonical(path.to_string_lossy().into_owned())
}

/// Binary paths for Moon tools.
pub(super) struct Binaries {
    pub moonc: PathBuf,
    pub moonbuild: PathBuf,
    pub moondoc: PathBuf,
    pub mooninfo: PathBuf,
}

impl Binaries {
    /// Locate Moon tool binaries
    pub fn locate() -> Self {
        let moonc = try_locate_binary("moonc");
        let moonbuild = try_locate_binary("moon");
        let moondoc = try_locate_binary("moondoc");
        let mooninfo = try_locate_binary("mooninfo");
        Self {
            moonc,
            moonbuild,
            moondoc,
            mooninfo,
        }
    }
}

/// Try to locate a binary in standard locations:
///
/// - around the current executable
/// - in MOON_HOME/bin
/// - in PATH
/// - no prefix, assume exists
fn try_locate_binary(bin_name: &str) -> PathBuf {
    let current_exe = std::env::current_exe().ok();
    if let Some(mut path) = current_exe {
        path.pop(); // exe name
        path.push(bin_name);
        if path.exists() {
            return path;
        }
    }

    if let Ok(moon_home) = std::env::var("MOON_HOME") {
        let mut path = PathBuf::from(moon_home);
        path.push("bin");
        path.push(bin_name);
        if path.exists() {
            return path;
        }
    }

    if let Ok(paths) = std::env::var("PATH") {
        for p in std::env::split_paths(&paths) {
            let mut candidate = p.clone();
            candidate.push(bin_name);
            if candidate.exists() {
                return candidate;
            }
        }
    }

    PathBuf::from(bin_name)
}
