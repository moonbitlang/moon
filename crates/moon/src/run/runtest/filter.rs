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

use std::{collections::BTreeSet, ops::Range};

use indexmap::IndexMap;
use moonbuild::test_utils::indices_to_ranges;
use moonbuild_rupes_recta::{
    cond_comp::FileTestKind,
    model::{BuildTarget, PackageId, TargetKind},
};
use moonutil::common::{MbtTestInfo, MooncGenTestInfo};

use crate::run::TestIndex;

/// Leaf-level filter over test indices within a file.
///
/// Notice that we have distinguished between different kinds of tests in
/// [`PackageFilter`], and files within a single kind of test have only one
/// numbering sequence, so there's no need to distinguish between regular tests
/// and doc tests here.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct IndexFilter(pub BTreeSet<u32>);

/// File-level filter within a package.
/// - Key: file path (exact match).
/// - Value:
///   - None => wildcard (all indices allowed in that file).
///   - Some(IndexFilter) => only the indices listed are allowed.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FileFilter(pub IndexMap<String, Option<IndexFilter>>);

/// Package-level filter for a module.
/// - Key: package full name (exact match).
/// - Value:
///   - None => wildcard (all files and indices under the package are allowed).
///   - Some(FileFilter) => only listed files/indices are allowed.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct PackageFilter(pub IndexMap<BuildTarget, Option<FileFilter>>);

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
        if let Some(v) = self.0.get_mut(&pkg) {
            match (file, v) {
                (None, v) => *v = None, // wildcard package, nothing more to do
                (Some(_), None) => {}   // already wildcarded
                (Some(f), Some(ff)) => {
                    ff.add_one(f, index);
                }
            }
        } else {
            let v = if let Some(f) = file {
                let mut ff = FileFilter::default();
                ff.add_one(f, index);
                Some(ff)
            } else {
                None // wildcard package
            };
            self.0.insert(pkg, v);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FileFilter {
    pub fn add_one(&mut self, file: &str, index: Option<u32>) {
        if let Some(v) = self.0.get_mut(file) {
            match (index, v) {
                (None, v) => *v = None,
                (Some(_), None) => {} // already wildcarded
                (Some(i), Some(ixf)) => {
                    ixf.0.insert(i);
                }
            }
        } else {
            self.0.insert(
                file.to_string(),
                index.map(|i| IndexFilter([i].into_iter().collect())),
            );
        }
    }
}

fn all_ranges(infos: &[MbtTestInfo]) -> Vec<Range<u32>> {
    // Use actual indices from test metadata instead of assuming contiguous 0..max_index
    let actual_indices: Vec<u32> = infos.iter().map(|t| t.index).collect();
    indices_to_ranges(actual_indices)
}

pub fn apply_filter(
    file_filt: Option<&FileFilter>,
    meta: &MooncGenTestInfo,
    files_and_index: &mut Vec<(String, Vec<std::ops::Range<u32>>)>,
) {
    let lists = [
        &meta.no_args_tests,
        &meta.with_args_tests,
        &meta.async_tests,
    ];

    match file_filt {
        // If there is no file filter, we can simply add all files and indices
        None => {
            for test_list in lists {
                for (filename, test_infos) in test_list {
                    let this_file_index = all_ranges(test_infos);
                    files_and_index.push((filename.clone(), this_file_index));
                }
            }
        }

        // If there is a list of files to filter, we can only access these files
        Some(filt) => {
            for (k, v) in &filt.0 {
                let mut this_file_index = vec![];
                // Filter files from lists
                for test_list in lists {
                    if let Some(tests) = test_list.get(k) {
                        match v {
                            None => {
                                // Wildcard, add all indices
                                this_file_index.extend(all_ranges(tests));
                            }
                            Some(ixf) => {
                                for t in tests {
                                    if ixf.0.contains(&t.index) {
                                        this_file_index.push(t.index..t.index + 1);
                                    }
                                }
                            }
                        }
                    }
                }
                files_and_index.push((k.clone(), this_file_index));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use expect_test::expect;
    use moonutil::common::{MbtTestInfo, MooncGenTestInfo};

    fn example_meta() -> MooncGenTestInfo {
        MooncGenTestInfo {
            no_args_tests: [
                (
                    "file1.mbt".into(),
                    vec![
                        MbtTestInfo {
                            index: 0,
                            func: "test_zero".into(),
                            name: Some("zero".into()),
                            line_number: Some(10),
                        },
                        MbtTestInfo {
                            index: 1,
                            func: "test_one".into(),
                            name: Some("one".into()),
                            line_number: Some(20),
                        },
                        // Noncontiguous index to demonstrate gaps (e.g., missing 1..3)
                        MbtTestInfo {
                            index: 4,
                            func: "test_four".into(),
                            name: Some("four".into()),
                            line_number: Some(40),
                        },
                    ],
                ),
                (
                    "file2.mbt".into(),
                    vec![MbtTestInfo {
                        index: 2,
                        func: "test_two".into(),
                        name: Some("two".into()),
                        line_number: Some(30),
                    }],
                ),
                (
                    "doc_tests.mbt".into(),
                    vec![
                        MbtTestInfo {
                            index: 0,
                            func: "doctest_0".into(),
                            name: Some("doctest a".into()),
                            line_number: Some(5),
                        },
                        MbtTestInfo {
                            index: 1,
                            func: "doctest_1".into(),
                            name: Some("doctest b".into()),
                            line_number: Some(15),
                        },
                    ],
                ),
            ]
            .into_iter()
            .collect(),
            // Note: "file1.mbt" also appears here to demonstrate the same file
            // having tests in both no_args and with_args lists.
            with_args_tests: [
                (
                    "file1.mbt".into(),
                    vec![MbtTestInfo {
                        index: 2,
                        func: "file1_with_args".into(),
                        name: Some("file1 param".into()),
                        line_number: Some(25),
                    }],
                ),
                ("my_file.mbt".into(), vec![]),
                (
                    "param_file.mbt".into(),
                    vec![MbtTestInfo {
                        index: 0,
                        func: "param_test".into(),
                        name: Some("param".into()),
                        line_number: Some(12),
                    }],
                ),
            ]
            .into_iter()
            .collect(),
            with_bench_args_tests: Default::default(),
            async_tests: Default::default(),
        }
    }

    #[test]
    fn test_no_file_filter() {
        let meta = example_meta();
        let mut out = vec![];
        super::apply_filter(None, &meta, &mut out);

        expect![[r#"[("file1.mbt", [0..2, 4..5]), ("file2.mbt", [2..3]), ("doc_tests.mbt", [0..2]), ("file1.mbt", [2..3]), ("my_file.mbt", []), ("param_file.mbt", [0..1])]"#]]
        .assert_eq(&format!("{:?}", out));
    }

    #[test]
    fn test_file_filter_wildcard_single_file() {
        let meta = example_meta();
        let mut ff = super::FileFilter::default();
        // Wildcard for a single file should include all indices from both no_args and with_args lists for that file
        ff.add_one("file1.mbt", None);

        expect![[r#"FileFilter({"file1.mbt": None})"#]].assert_eq(&format!("{:?}", ff));

        let mut out = vec![];
        super::apply_filter(Some(&ff), &meta, &mut out);

        expect![[r#"[("file1.mbt", [0..2, 4..5, 2..3])]"#]].assert_eq(&format!("{:?}", out));
    }

    #[test]
    fn test_file_filter_specific_indices_single_file() {
        let meta = example_meta();
        let mut ff = super::FileFilter::default();
        // Only allow specific indices for file1.mbt; others should be excluded
        ff.add_one("file1.mbt", Some(1));
        ff.add_one("file1.mbt", Some(4));

        expect![[r#"FileFilter({"file1.mbt": Some(IndexFilter({1, 4}))})"#]]
            .assert_eq(&format!("{:?}", ff));

        let mut out = vec![];
        super::apply_filter(Some(&ff), &meta, &mut out);

        expect![[r#"[("file1.mbt", [1..2, 4..5])]"#]].assert_eq(&format!("{:?}", out));
    }

    #[test]
    fn test_file_filter_multiple_files_mixed() {
        let meta = example_meta();
        let mut ff = super::FileFilter::default();

        // Mixed: specific indices for one file, wildcard for another, and a with-args file with a single allowed index
        ff.add_one("file1.mbt", Some(0)); // allow only index 0 in file1.mbt
        ff.add_one("doc_tests.mbt", None); // allow all in doc_tests.mbt
        ff.add_one("param_file.mbt", Some(0)); // allow only index 0 in param_file.mbt

        expect![[r#"FileFilter({"file1.mbt": Some(IndexFilter({0})), "doc_tests.mbt": None, "param_file.mbt": Some(IndexFilter({0}))})"#]]
        .assert_eq(&format!("{:?}", ff));

        let mut out = vec![];
        super::apply_filter(Some(&ff), &meta, &mut out);

        expect![[
            r#"[("file1.mbt", [0..1]), ("doc_tests.mbt", [0..2]), ("param_file.mbt", [0..1])]"#
        ]]
        .assert_eq(&format!("{:?}", out));
    }

    #[test]
    fn test_file_filter_empty_filter_excludes_all() {
        let meta = example_meta();
        let ff = super::FileFilter::default(); // no files listed => exclude everything

        expect!["FileFilter({})"].assert_eq(&format!("{:?}", ff));

        let mut out = vec![];
        super::apply_filter(Some(&ff), &meta, &mut out);

        expect!["[]"].assert_eq(&format!("{:?}", out));
    }

    #[test]
    fn test_file_filter_wildcard_file_with_no_tests() {
        let meta = example_meta();
        let mut ff = super::FileFilter::default();
        // my_file.mbt exists only in with_args_tests with an empty list; should still appear with empty indices
        ff.add_one("my_file.mbt", None);

        expect![[r#"FileFilter({"my_file.mbt": None})"#]].assert_eq(&format!("{:?}", ff));

        let mut out = vec![];
        super::apply_filter(Some(&ff), &meta, &mut out);

        expect![[r#"[("my_file.mbt", [])]"#]].assert_eq(&format!("{:?}", out));
    }
}
