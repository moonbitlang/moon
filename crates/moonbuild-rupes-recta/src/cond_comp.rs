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

//! Solves conditional compilation directives

use std::path::Path;

use moonutil::{
    common::TargetBackend,
    cond_expr::{CompileCondition as MetadataCompileCondition, CondExpr, OptLevel},
    package::MoonPkg,
};

use crate::model;

/// Which kind of test (if any) are we compiling the current package for.
#[derive(Clone, Copy)]
pub enum TestKind {
    Inline,
    Whitebox,
    Blackbox,
}

impl From<model::TargetKind> for Option<TestKind> {
    fn from(value: model::TargetKind) -> Self {
        match value {
            model::TargetKind::Source => None,
            model::TargetKind::WhiteboxTest => Some(TestKind::Whitebox),
            model::TargetKind::BlackboxTest => Some(TestKind::Blackbox),
            model::TargetKind::InlineTest => Some(TestKind::Inline),
            model::TargetKind::SubPackage => None,
        }
    }
}

/// A collection of conditions that affect compilation behavior.
pub(crate) struct CompileCondition {
    pub optlevel: OptLevel,
    pub test_kind: Option<TestKind>,
    pub backend: TargetBackend,
}

/// Get the list of files that should get included in the compile list under
/// the given condition.
///
/// Note: `pkg_file_path` is purely for error reporting, as required by
/// [`moonutil::cond_expr::parse_cond_expr`].
pub(crate) fn filter_files<'a, P: AsRef<Path> + 'a>(
    pkg: &'a MoonPkg,
    files: impl Iterator<Item = P> + 'a,
    cond: &'a CompileCondition,
) -> impl Iterator<Item = (P, FileTestKind)> + 'a {
    files.filter_map(|file| {
        let filename = file
            .as_ref()
            .file_name()
            .expect("Input source file should have a filename");
        let str_filename = filename.to_string_lossy();

        let should_include = if let Some(expect_cond) = pkg
            .targets
            .as_ref()
            .and_then(|targets| targets.get(&*str_filename))
        {
            // We have a condition for this file
            should_compile_using_pkg_cond_expr(&str_filename, expect_cond, cond)
        } else {
            // We don't, evaluate file name
            should_compile_using_filename(&str_filename, cond)
        };

        should_include.map(|kind| (file, kind))
    })
}

/// Get the test kind and compile condition metadata for each file, without
/// actually filtering.
///
/// This function is used for generating metadata that feeds into other tools.
pub(crate) fn file_metadatas<'a>(
    pkg: &'a MoonPkg,
    files: impl Iterator<Item = &'a Path> + 'a,
) -> impl Iterator<Item = (&'a Path, FileTestKind, MetadataCompileCondition)> + 'a {
    files.map(|path| {
        let filename = path
            .file_name()
            .expect("Input source file should have a filename");
        let str_filename = filename.to_string_lossy();
        let without_mbt = str_filename
            .strip_suffix(".mbt")
            .expect("Input source file should end with .mbt");

        let (cond, stem) = if let Some(expect_cond) = pkg
            .targets
            .as_ref()
            .and_then(|targets| targets.get(&*str_filename))
        {
            (expect_cond.to_compile_condition(), without_mbt)
        } else {
            let (backend, remaining) = get_file_target_backend(without_mbt);
            let cond = MetadataCompileCondition {
                backend: backend.map_or_else(|| TargetBackend::all().into(), |x| vec![x]),
                optlevel: OptLevel::all().to_vec(),
            };
            (cond, remaining)
        };
        let test_info = get_file_test_kind(stem);

        (path, test_info, cond)
    })
}

fn should_compile_using_pkg_cond_expr(
    name: &str,
    cond_expr: &CondExpr,
    actual: &CompileCondition,
) -> Option<FileTestKind> {
    // TODO: Put the parsing earlier, not here
    if !cond_expr.eval(actual.optlevel, actual.backend) {
        None // Fails the condition in pkg.json
    } else if let Some(stripped) = name.strip_suffix(".mbt") {
        let spec = get_file_test_kind(stripped);
        let include = check_test_suffix(spec, actual.test_kind);
        if include {
            Some(spec)
        } else {
            None
        }
    } else {
        panic!("File name '{}' does not end with '.mbt'", name);
    }
}

/// Get the target backend specified in the file name, if any. Returns that
/// and the stripped filename. Expects a name already stripped of `.mbt`.
pub fn get_file_target_backend(stripped_filename: &str) -> (Option<TargetBackend>, &str) {
    match stripped_filename.rsplit_once('.') {
        Some((prev, suffix)) => match TargetBackend::str_to_backend(suffix) {
            Ok(backend) => (Some(backend), prev), // has backend -- chop it off
            Err(_) => (None, stripped_filename),  // has does not look like backend -- retain as-is
        },
        None => (None, stripped_filename),
    }
}

/// Check the file name to determine if it should be included. If true,
/// returns `Some(file_test_kind)`, otherwise `None`.
fn should_compile_using_filename(name: &str, actual: &CompileCondition) -> Option<FileTestKind> {
    let Some(filename) = name.strip_suffix(".mbt") else {
        panic!("File name '{}' does not end with '.mbt'", name);
    };

    // Target backend checking -- check the suffix of the file name
    let (backend, remaining) = get_file_target_backend(filename);
    if let Some(backend) = backend {
        if backend != actual.backend {
            return None; // Wrong backend, returning
        }
    }

    let spec = get_file_test_kind(remaining);
    let include = check_test_suffix(spec, actual.test_kind);
    if include {
        Some(spec)
    } else {
        None
    }
}

/// Get the test kind of the file by checking its suffix, and also handles if
/// the file has not been stripped of `.mbt` or target backend suffix.
pub fn get_file_test_kind_full(file_name: &str) -> FileTestKind {
    let stripped_name = if let Some(stripped) = file_name.strip_suffix(".mbt") {
        stripped
    } else {
        file_name
    };
    let (_backend, remaining) = get_file_target_backend(stripped_name);
    get_file_test_kind(remaining)
}

/// Get the test kind of the file by checking its suffix. Expects a name already
/// stripped of `.mbt` and any target backend suffix.
pub fn get_file_test_kind(stripped_name: &str) -> FileTestKind {
    if stripped_name.ends_with("_wbtest") {
        FileTestKind::Whitebox
    } else if stripped_name.ends_with("_test") {
        FileTestKind::Blackbox
    } else {
        FileTestKind::NoTest
    }
}

/// Check the suffix of the stripped filename against the actual test condition
fn check_test_suffix(file_test_spec: FileTestKind, test_kind: Option<TestKind>) -> bool {
    use FileTestKind::*;

    // White box tests are implemented with compiling with source code, and
    // black box tests are implemented without
    #[allow(clippy::match_like_matches_macro)] // This is more readable
    match (test_kind, file_test_spec) {
        (None, NoTest) => true,
        (Some(TestKind::Inline), NoTest) => true,
        (Some(TestKind::Whitebox), NoTest | Whitebox) => true,
        // Black box tests return no test files for doctest compilation
        // FIXME: might not be the best way to handle this
        (Some(TestKind::Blackbox), NoTest | Blackbox) => true,
        _ => false,
    }
}

/// Which kind of test does this file represent
#[derive(Debug, Clone, Copy)]
pub enum FileTestKind {
    NoTest,
    Whitebox,
    Blackbox,
}
