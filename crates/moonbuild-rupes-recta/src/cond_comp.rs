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
    common::{DOT_MBT_DOT_MD, TargetBackend},
    cond_expr::{CompileCondition as MetadataCompileCondition, CondExpr, OptLevel},
    package::MoonPkg,
};

/// Classify files that are available for the current backend and optimization
/// level, without projecting them into a specific target kind.
pub(crate) fn classify_files<'a, P: AsRef<Path> + 'a>(
    pkg: &'a MoonPkg,
    files: impl Iterator<Item = P> + 'a,
    optlevel: OptLevel,
    backend: TargetBackend,
) -> impl Iterator<Item = (P, FileTestKind)> + 'a {
    files.filter_map(move |file| {
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
            should_compile_using_pkg_cond_expr_without_test_kind(
                &str_filename,
                expect_cond,
                optlevel,
                backend,
            )
        } else {
            should_compile_using_filename_without_test_kind(&str_filename, backend)
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
        let without_mbt = if let Some(stripped) = str_filename.strip_suffix(".mbt") {
            stripped
        } else if let Some(stripped) = str_filename.strip_suffix(DOT_MBT_DOT_MD) {
            stripped
        } else {
            panic!(
                "File name '{}' does not end with '.mbt' or '.mbt.md'",
                str_filename
            );
        };

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

fn should_compile_using_pkg_cond_expr_without_test_kind(
    name: &str,
    cond_expr: &CondExpr,
    optlevel: OptLevel,
    backend: TargetBackend,
) -> Option<FileTestKind> {
    if !cond_expr.eval(optlevel, backend) {
        None
    } else if let Some(stripped) = name.strip_suffix(".mbt") {
        Some(get_file_test_kind(stripped))
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

fn should_compile_using_filename_without_test_kind(
    name: &str,
    actual_backend: TargetBackend,
) -> Option<FileTestKind> {
    let filename = if let Some(mbt_stripped) = name.strip_suffix(".mbt") {
        mbt_stripped
    } else if let Some(mbt_md_stripped) = name.strip_suffix(DOT_MBT_DOT_MD) {
        mbt_md_stripped
    } else {
        panic!("File name '{}' does not end with '.mbt' or '.mbt.md'", name);
    };

    let (backend, remaining) = get_file_target_backend(filename);
    if let Some(backend) = backend
        && backend != actual_backend
    {
        return None;
    }

    Some(get_file_test_kind(remaining))
}

/// Get the test kind of the file by checking its suffix, and also handles if
/// the file has not been stripped of `.mbt` or target backend suffix.
pub fn get_file_test_kind_full(file_name: &str) -> FileTestKind {
    let stripped_name = if let Some(stripped) = file_name.strip_suffix(".mbt") {
        stripped
    } else if file_name.ends_with(DOT_MBT_DOT_MD) {
        return FileTestKind::Blackbox;
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

/// Which kind of test does this file represent
#[derive(Debug, Clone, Copy)]
pub enum FileTestKind {
    NoTest,
    Whitebox,
    Blackbox,
}
