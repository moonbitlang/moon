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

//! Filesystem path comparisons that accept equivalent Windows prefixes.

use std::path::Path;

/// Compare containment without treating ordinary and verbatim Windows drive/UNC
/// prefixes as different roots. This does not change paths used for I/O, resolve
/// dot components, or require paths (such as removed watch events) to exist.
pub fn starts_with_equivalent_prefix(path: &Path, base: &Path) -> bool {
    strip_equivalent_prefix(path, base).is_some()
}

/// Strip a path prefix while accepting ordinary and verbatim Windows drive/UNC
/// spellings at an external-input or display boundary. Internal paths should
/// still be constructed consistently before comparison. Like `strip_prefix`,
/// this matches complete components without resolving symlinks or dot segments.
pub fn strip_equivalent_prefix<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    base.components()
        .all(|base| match (components.next(), base) {
            (Some(Component::Prefix(path)), Component::Prefix(base)) => {
                match (path.kind(), base.kind()) {
                    (
                        Prefix::Disk(path) | Prefix::VerbatimDisk(path),
                        Prefix::Disk(base) | Prefix::VerbatimDisk(base),
                    ) => path == base,
                    (
                        Prefix::UNC(path_server, path_share)
                        | Prefix::VerbatimUNC(path_server, path_share),
                        Prefix::UNC(base_server, base_share)
                        | Prefix::VerbatimUNC(base_server, base_share),
                    ) => path_server == base_server && path_share == base_share,
                    (path, base) => path == base,
                }
            }
            (Some(path), base) => path == base,
            (None, _) => false,
        })
        .then_some(components.as_path())
}

#[cfg(windows)]
#[test]
fn strip_equivalent_prefix_preserves_long_suffixes_and_component_boundaries() {
    let suffix = format!("{}moonc.exe", r"segment\".repeat(40));
    for (ordinary, verbatim) in [
        (r"C:\Users\test", r"\\?\C:\Users\test"),
        (r"\\server\share\test", r"\\?\UNC\server\share\test"),
    ] {
        for root in [ordinary, verbatim] {
            let path = Path::new(root).join(&suffix);
            for base in [ordinary, verbatim] {
                assert_eq!(
                    strip_equivalent_prefix(&path, Path::new(base)),
                    Some(Path::new(&suffix))
                );
                assert_eq!(
                    strip_equivalent_prefix(Path::new(root), Path::new(base)),
                    Some(Path::new(""))
                );
                assert_eq!(
                    strip_equivalent_prefix(Path::new(&format!("{root}-other")), Path::new(base)),
                    None
                );
            }
        }
    }
    assert_eq!(
        strip_equivalent_prefix(
            Path::new(r"\\?\D:\Users\test\moonc.exe"),
            Path::new(r"C:\Users\test")
        ),
        None
    );
    assert_eq!(
        strip_equivalent_prefix(
            Path::new(r"\\?\UNC\server\other\test\moonc.exe"),
            Path::new(r"\\server\share\test")
        ),
        None
    );
}
