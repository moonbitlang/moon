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

use std::collections::{HashMap, HashSet};

use moonbuild_rupes_recta::{
    cond_comp::FileTestKind,
    model::{BuildTarget, PackageId, TargetKind},
};

use crate::run::TestIndex;

/// Leaf-level filter over test indices within a file.
///
/// Notice that we have distinguished between different kinds of tests in
/// [`PackageFilter`], and files within a single kind of test have only one
/// numbering sequence, so there's no need to distinguish between regular tests
/// and doc tests here.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct IndexFilter(pub HashSet<u32>);

/// File-level filter within a package.
/// - Key: file path (exact match).
/// - Value:
///   - None => wildcard (all indices allowed in that file).
///   - Some(IndexFilter) => only the indices listed are allowed.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FileFilter(pub HashMap<String, Option<IndexFilter>>);

/// Package-level filter for a module.
/// - Key: package full name (exact match).
/// - Value:
///   - None => wildcard (all files and indices under the package are allowed).
///   - Some(FileFilter) => only listed files/indices are allowed.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PackageFilter(pub HashMap<BuildTarget, Option<FileFilter>>);

/// Root filter used by the test runner.
/// `filter == None` means no restriction (allow everything).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct TestFilter {
    pub filter: Option<PackageFilter>,
}

impl TestFilter {
    /// Mutably add one optional path (package, file, index) into the filter.
    ///
    /// A broader wildcard overwrites narrower entries:
    /// - (pkg, None, _) => package wildcard; discards any file/index subfilters
    /// - (pkg, Some(file), None) => file wildcard; discards any index set for that file
    /// - (pkg, Some(file), Some(index)) => adds that index unless the path is already wildcarded
    pub fn add_one(&mut self, pkg: Option<BuildTarget>, file: Option<&str>, index: Option<u32>) {
        if let Some(pkg) = pkg {
            let pf = self.filter.get_or_insert_with(Default::default);
            pf.add_one(pkg, file, index);
        } else {
            self.filter = None; // Global wildcard, drop existing ones.
        }
    }

    /// Like [`Self::add_one`], but automatically determines which test
    /// target of the package to use based on the file name and index.
    pub fn add_autodetermine_target(
        &mut self,
        pkg: PackageId,
        file: Option<&str>,
        index: Option<TestIndex>,
    ) {
        if let Some(file) = file {
            let kind = moonbuild_rupes_recta::cond_comp::get_file_test_kind_full(file);
            let targets: &[TargetKind] = match (kind, index) {
                // A regular source file should be tested for both its inline
                // tests and doc tests
                (FileTestKind::NoTest, None) => &[TargetKind::InlineTest, TargetKind::BlackboxTest],
                // Specific test/doctest indices
                (FileTestKind::NoTest, Some(TestIndex::Regular(_))) => &[TargetKind::InlineTest],
                (FileTestKind::NoTest, Some(TestIndex::DocTest(_))) => &[TargetKind::BlackboxTest],
                // Others are just direct mappings
                (FileTestKind::Whitebox, _) => &[TargetKind::WhiteboxTest],
                (FileTestKind::Blackbox, _) => &[TargetKind::BlackboxTest],
            };
            for &target in targets {
                self.add_one(
                    Some(pkg.build_target(target)),
                    Some(file),
                    index.map(TestIndex::value),
                );
            }
        } else {
            // No file wildcard, test for all targets
            for &target in TargetKind::all_tests() {
                self.add_one(
                    Some(pkg.build_target(target)),
                    None,
                    index.map(TestIndex::value),
                );
            }
        }
    }

    /// Check package-level membership.
    ///
    /// Returns (is_in_filter, next-level filter to check if any):
    /// - Top-level None => (true, None) i.e., no more checks needed (global wildcard).
    /// - Absent package => (false, None).
    /// - Present with None => (true, None) (package wildcard).
    /// - Present with Some(FileFilter) => (true, Some(FileFilter)).
    #[must_use]
    pub fn check_package(&self, package: BuildTarget) -> (bool, Option<&FileFilter>) {
        match &self.filter {
            None => (true, None),
            Some(pf) => match pf.0.get(&package) {
                None => (false, None),
                Some(None) => (true, None),
                Some(Some(ff)) => (true, Some(ff)),
            },
        }
    }
}

/// Package-level helpers for constructing filters.
impl PackageFilter {
    pub fn add_one(&mut self, pkg: BuildTarget, file: Option<&str>, index: Option<u32>) {
        let entry = self.0.entry(pkg).or_default();
        if let Some(file) = file {
            if let Some(ff) = entry {
                ff.add_one(file, index);
            } else {
                // Package is already wildcarded; do nothing.
            }
        } else {
            // Wildcard the package; discard any existing subfilters.
            *entry = None;
        }
    }
}

impl FileFilter {
    /// Check file-level membership.
    ///
    /// Returns (is_in_filter, next-level filter to check if any):
    /// - File not present => (false, None).
    /// - Present with None => (true, None) (file wildcard).
    /// - Present with Some(IndexFilter) => (true, Some(IndexFilter)).
    #[must_use]
    pub fn check_file<'a>(&'a self, file: &str) -> (bool, Option<&'a IndexFilter>) {
        match self.0.get(file) {
            None => (false, None),
            Some(None) => (true, None),
            Some(Some(ixf)) => (true, Some(ixf)),
        }
    }

    pub fn add_one(&mut self, file: &str, index: Option<u32>) {
        let entry = if let Some(v) = self.0.get_mut(file) {
            v
        } else {
            self.0.entry(file.to_string()).or_default()
        };
        if let Some(index) = index {
            if let Some(ixf) = entry {
                ixf.0.insert(index);
            } else {
                // File is already wildcarded; do nothing.
            }
        } else {
            // Wildcard the file; discard any existing subfilters.
            *entry = None;
        }
    }
}
