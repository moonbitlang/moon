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

//! Defines the Package File Set and owns its filesystem traversal rules.

use std::path::{Path, PathBuf};

use moonutil::constants::{
    MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, PackageSourceFileKind,
    is_ignored_directory_name, package_source_file_kind,
};
use walkdir::WalkDir;

#[derive(Default)]
pub(super) struct DiscoveredPackageFiles {
    pub(super) source_files: Vec<PathBuf>,
    pub(super) mbt_lex_files: Vec<PathBuf>,
    pub(super) mbt_yacc_files: Vec<PathBuf>,
    pub(super) mbt_md_files: Vec<PathBuf>,
    pub(super) mbtp_files: Vec<PathBuf>,
    pub(super) c_stub_header_files: Vec<PathBuf>,
}

impl DiscoveredPackageFiles {
    /// Discovers the filesystem inputs inferred from one package's layout.
    ///
    /// MoonBit sources belong directly to the package root. C headers are
    /// recursive when the package declares C stubs, but follow the same
    /// package boundaries. The declared application data subtree is never
    /// part of the Package File Set.
    pub(super) fn discover(
        package_root: &Path,
        collect_c_stub_headers: bool,
        application_data_root: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let walker = WalkDir::new(package_root).sort_by_file_name();
        let walker = if collect_c_stub_headers {
            walker
        } else {
            walker.max_depth(1)
        };
        let mut entries = walker.into_iter();
        let mut files = Self::default();

        while let Some(entry) = entries.next() {
            let entry = entry?;
            if entry.depth() != 0 && entry.file_type().is_dir() {
                let path = entry.path();
                if application_data_root.is_some_and(|root| root == path)
                    || is_ignored_directory_name(entry.file_name())
                    || [MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON]
                        .iter()
                        .any(|manifest| path.join(manifest).exists())
                {
                    entries.skip_current_dir();
                    continue;
                }
            }

            if !entry.file_type().is_file() && !entry.file_type().is_symlink() {
                continue;
            }

            let path = entry.path();
            if entry.depth() == 1 {
                let filename = path
                    .file_name()
                    .expect("a direct package file should have a name")
                    .to_string_lossy();
                match package_source_file_kind(&filename) {
                    Some(PackageSourceFileKind::Mbt) => files.source_files.push(path.to_owned()),
                    Some(PackageSourceFileKind::MbtMd) => files.mbt_md_files.push(path.to_owned()),
                    Some(PackageSourceFileKind::Mbtp) => files.mbtp_files.push(path.to_owned()),
                    Some(PackageSourceFileKind::Mbl) => files.mbt_lex_files.push(path.to_owned()),
                    Some(PackageSourceFileKind::Mby) => files.mbt_yacc_files.push(path.to_owned()),
                    None => {}
                }
            }

            if collect_c_stub_headers
                && path.extension().is_some_and(|extension| {
                    matches!(extension.to_str(), Some("h" | "hh" | "hpp" | "hxx"))
                })
            {
                files.c_stub_header_files.push(path.to_owned());
            }
        }

        files.source_files.sort();
        files.mbt_lex_files.sort();
        files.mbt_yacc_files.sort();
        files.mbt_md_files.sort();
        files.mbtp_files.sort();
        files.c_stub_header_files.sort();
        Ok(files)
    }
}
