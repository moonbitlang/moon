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

use moonutil::module::ModuleDB;
use moonutil::moon_dir;
use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::collections::HashSet;

use moonutil::common::{MoonbuildOpt, MooncOpt, RunMode, TargetBackend};

/// Normalize a path for stable cross-platform comparison.
/// - Converts backslashes to forward slashes
/// - Removes trailing slashes
/// - Handles empty paths consistently
fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    // Remove trailing slash unless it's the root
    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.trim_end_matches('/').to_string()
    } else {
        normalized
    }
}

pub fn print_commands(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
    let moonc_opt = &MooncOpt {
        render: false,
        ..moonc_opt.clone()
    };

    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let in_same_dir = target_dir.starts_with(source_dir);
    let mode = moonbuild_opt.run_mode;

    let state = match mode {
        RunMode::Build | RunMode::Run => {
            crate::build::load_moon_proj(module, moonc_opt, moonbuild_opt)?
        }
        RunMode::Check => crate::check::normal::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Test | RunMode::Bench => {
            crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?
        }
        RunMode::Bundle => crate::bundle::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Format => crate::fmt::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
    };
    log::debug!("{:#?}", state);
    if !state.default.is_empty() {
        let mut sorted_default = state.default.clone();
        sorted_default.sort_by_key(|a| a.index());
        let builds: Vec<BuildId> = stable_toposort_graph(
            &state.graph,
            &sorted_default,
            &source_dir.to_string_lossy(),
            &target_dir.to_string_lossy(),
        );
        for b in builds.iter() {
            let build = &state.graph.builds[*b];
            if let Some(cmdline) = &build.cmdline {
                if in_same_dir {
                    // TODO: this replace is not safe
                    println!(
                        "{}",
                        cmdline.replace(&source_dir.display().to_string(), ".")
                    );
                } else {
                    println!("{cmdline}");
                }
            }
        }
        if mode == RunMode::Run {
            for fid in sorted_default.iter() {
                let mut watfile = state.graph.file(*fid).name.clone();
                let cmd = match moonc_opt.link_opt.target_backend {
                    TargetBackend::Wasm | TargetBackend::WasmGC => "moonrun ",
                    TargetBackend::Js => "node ",
                    TargetBackend::Native | TargetBackend::LLVM => {
                        // stub.o would be default for native and llvm, skip them
                        if !watfile.ends_with(".exe") {
                            continue;
                        }
                        ""
                    }
                };
                if in_same_dir {
                    watfile = watfile.replacen(&source_dir.display().to_string(), ".", 1);
                }

                let mut moonrun_command = format!("{cmd}{watfile}");
                if !moonbuild_opt.args.is_empty() {
                    moonrun_command =
                        format!("{moonrun_command} -- {}", moonbuild_opt.args.join(" "));
                }

                println!("{moonrun_command}");
            }
        }
    }
    Ok(0)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PathKind {
    InTarget,
    InSource,
    InHome,
}

/// A sortable path to create a stable sorting order from file names.
/// Uses normalized paths to ensure consistent ordering across platforms.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortablePath {
    kind: PathKind,
    /// Path normalized and converted to lowercase
    normalized_path: String,
}

/// Perform an iteration over the build graph to get the total list of build
/// commands that corresponds to the given inputs.
///
/// This function should have a stable output order based on the file names and
/// the structure of the build graph, but irrelevant of the actual insertion
/// order of the graph.
fn stable_toposort_graph(
    graph: &Graph,
    inputs: &[FileId],
    source_dir: &str,
    raw_target_dir: &str,
) -> Vec<BuildId> {
    // Normalize the source directory for consistent comparison
    let normalized_target_dir = normalize_path(raw_target_dir);
    let normalized_source_dir = normalize_path(source_dir);
    let normalized_moon_home = normalize_path(&moon_dir::MOON_DIRS.moon_home.to_string_lossy());

    // Get file name of file ID with platform-agnostic path handling
    let by_file_name = |k: &FileId| {
        let name = &graph.file(*k).name;
        let normalized_name = normalize_path(name);
        let mk_sortable_path = |kind, path: &str| {
            let stripped = path.strip_prefix('/').unwrap_or(path);
            SortablePath {
                kind,
                normalized_path: stripped.to_lowercase(),
            }
        };
        if let Some(stripped) = normalized_name.strip_prefix(&normalized_target_dir) {
            mk_sortable_path(PathKind::InTarget, stripped)
        } else if let Some(stripped) = normalized_name.strip_prefix(&normalized_source_dir) {
            mk_sortable_path(PathKind::InSource, stripped)
        } else if let Some(stripped) = normalized_name.strip_prefix(&normalized_moon_home) {
            mk_sortable_path(PathKind::InHome, stripped)
        } else {
            panic!(
                "file {} is outside both source ({}), target ({}) and MOON_HOME ({}) directories",
                name,
                source_dir,
                raw_target_dir,
                moon_dir::MOON_DIRS.moon_home.display()
            );
        }
    };

    // Sort the input files by the filenames
    let mut input_order = Vec::new();
    input_order.extend_from_slice(inputs);
    input_order.sort_by_cached_key(by_file_name);

    // DFS stack
    // (file_id, is_pop)
    let mut stack = Vec::<(FileId, bool)>::new();
    stack.extend(input_order.into_iter().map(|x| (x, false)));
    // Result
    let mut res = vec![];
    // Visited builds set
    let mut vis = HashSet::new();
    // Scratch vec for sorting input. Leave empty when unused.
    let mut sort_in_scratch = vec![];

    while let Some((fid, pop)) = stack.pop() {
        let file = graph.file(fid);
        if let Some(bid) = file.input {
            if !pop {
                if vis.insert(bid) {
                    let build = &graph.builds[bid];

                    // Push the build back to be used when popping
                    stack.push((fid, true));

                    // Push input files in sorted order
                    debug_assert!(sort_in_scratch.is_empty());
                    sort_in_scratch.extend_from_slice(build.explicit_ins());
                    sort_in_scratch.sort_by_cached_key(by_file_name);
                    stack.extend(sort_in_scratch.iter().copied().map(|x| (x, false)));
                    sort_in_scratch.clear();
                }
            } else {
                res.push(bid);
            }
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        // Test backslash to forward slash conversion
        assert_eq!(
            normalize_path("C:\\Users\\test\\file.txt"),
            "C:/Users/test/file.txt"
        );
        assert_eq!(normalize_path("src/main.rs"), "src/main.rs");

        // Test trailing slash removal
        assert_eq!(normalize_path("src/"), "src");
        assert_eq!(normalize_path("src/test/"), "src/test");

        // Root paths should keep their slash
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("C:/"), "C:");

        // Empty and single char paths
        assert_eq!(normalize_path(""), "");
        assert_eq!(normalize_path("a"), "a");
    }

    #[test]
    fn test_sortable_path_ordering() {
        // InSource should always come before Outside
        let in_source = SortablePath {
            kind: PathKind::InSource,
            normalized_path: "zzz.txt".to_string(),
        };
        let outside = SortablePath {
            kind: PathKind::InHome,
            normalized_path: "aaa.txt".to_string(),
        };
        assert!(in_source < outside);

        // InTarget should come before InSource
        let in_target = SortablePath {
            kind: PathKind::InTarget,
            normalized_path: "zzz.txt".to_string(),
        };
        assert!(in_target < in_source);

        // Within same category, should be case-insensitive alphabetical
        // Note: normalized_path is already lowercase in the actual implementation
        let path1 = SortablePath {
            kind: PathKind::InSource,
            normalized_path: "file.txt".to_string(), // lowercase
        };
        let path2 = SortablePath {
            kind: PathKind::InSource,
            normalized_path: "file.txt".to_string(), // lowercase
        };
        let path3 = SortablePath {
            kind: PathKind::InSource,
            normalized_path: "another.txt".to_string(), // lowercase
        };

        // path1 and path2 should be equal (same normalized path)
        assert_eq!(path1.cmp(&path2), std::cmp::Ordering::Equal);

        // path3 should come before both (alphabetically)
        assert!(path3 < path1);
        assert!(path3 < path2);
    }

    #[test]
    fn test_path_prefix_stripping() {
        // Test that Windows-style and Unix-style paths are handled consistently
        let test_cases = vec![
            // (source_dir, file_path, expected_kind, expected_normalized_lowercase)
            (
                "C:\\project",
                "C:\\project\\src\\main.rs",
                PathKind::InSource,
                "src/main.rs",
            ),
            (
                "C:/project",
                "C:/project/src/main.rs",
                PathKind::InSource,
                "src/main.rs",
            ),
            (
                "/home/user/project",
                "/home/user/project/src/main.rs",
                PathKind::InSource,
                "src/main.rs",
            ),
            (
                "./project",
                "./project/src/main.rs",
                PathKind::InSource,
                "src/main.rs",
            ),
            (
                "C:\\project",
                "D:\\external\\lib.rs",
                PathKind::InHome,
                "d:/external/lib.rs", // Note: lowercase due to to_lowercase() in implementation
            ),
        ];

        for (source_dir, file_path, expected_kind, expected_normalized) in test_cases {
            let normalized_source_dir = normalize_path(source_dir);
            let normalized_file_path = normalize_path(file_path);

            let result =
                if let Some(stripped) = normalized_file_path.strip_prefix(&normalized_source_dir) {
                    let stripped_clean = stripped.strip_prefix('/').unwrap_or(stripped);
                    SortablePath {
                        kind: PathKind::InSource,
                        normalized_path: stripped_clean.to_lowercase(), // Match implementation
                    }
                } else {
                    SortablePath {
                        kind: PathKind::InHome,
                        normalized_path: normalized_file_path.to_lowercase(), // Match implementation
                    }
                };

            assert_eq!(
                result.kind, expected_kind,
                "Failed for source_dir: {}, file_path: {}",
                source_dir, file_path
            );
            assert_eq!(
                result.normalized_path, expected_normalized,
                "Failed normalization for source_dir: {}, file_path: {}",
                source_dir, file_path
            );
        }
    }

    #[test]
    fn test_case_insensitive_sorting() {
        // Test that paths with different cases are sorted consistently
        let paths = [
            SortablePath {
                kind: PathKind::InSource,
                normalized_path: "File.TXT".to_lowercase(),
            },
            SortablePath {
                kind: PathKind::InSource,
                normalized_path: "file.txt".to_lowercase(),
            },
            SortablePath {
                kind: PathKind::InSource,
                normalized_path: "FILE.txt".to_lowercase(),
            },
        ];

        // All should be equal since they normalize to the same lowercase string
        for i in 0..paths.len() {
            for j in i + 1..paths.len() {
                assert_eq!(
                    paths[i].cmp(&paths[j]),
                    std::cmp::Ordering::Equal,
                    "Paths with different cases should be equal after normalization"
                );
            }
        }
    }

    #[test]
    fn test_cross_platform_path_consistency() {
        // Test that equivalent paths on different platforms produce the same result
        let test_cases = vec![
            // Windows vs Unix equivalent paths
            ("C:\\project", "C:\\project\\src\\Main.MV", "src/main.mv"),
            ("C:/project", "C:/project/src/Main.MV", "src/main.mv"),
            ("/project", "/project/src/Main.MV", "src/main.mv"),
        ];

        for (source_dir, file_path, expected_normalized) in test_cases {
            let normalized_source_dir = normalize_path(source_dir);
            let normalized_file_path = normalize_path(file_path);

            if let Some(stripped) = normalized_file_path.strip_prefix(&normalized_source_dir) {
                let stripped_clean = stripped.strip_prefix('/').unwrap_or(stripped);
                let result = SortablePath {
                    kind: PathKind::InSource,
                    normalized_path: stripped_clean.to_lowercase(),
                };

                assert_eq!(
                    result.normalized_path, expected_normalized,
                    "Cross-platform path normalization failed for: {} -> {}",
                    file_path, expected_normalized
                );
            }
        }
    }
}
